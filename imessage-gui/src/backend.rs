//! Backend worker thread.
//!
//! The iMessage database connection (and the `Config` that owns it) is not
//! `Sync`, so all database work happens on a single dedicated thread. The UI
//! thread communicates with it exclusively through [`Command`]/[`Event`]
//! channels and never touches the connection directly.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{channel, Receiver, Sender},
        Arc,
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use eframe::egui;

use imessage_database::{
    tables::{handle::Handle, messages::Message, table::Cacheable},
    util::{
        dates::{format as fmt_date, get_local_time},
        platform::Platform,
        query_context::QueryContext,
    },
};
use imessage_exporter::{
    app::{
        call_logs::{self, CallLogFilter, CallLogFormat},
        compatibility::attachment_manager::{AttachmentManager, AttachmentManagerMode},
        error::RuntimeError,
        export_type::ExportType,
        runtime::{CancelCallback, ProgressCallback},
    },
    exporters::pdf,
    Config, Options,
};

use crate::model::*;

#[derive(Clone, Debug)]
struct OpenedSource {
    root_path: PathBuf,
    platform: PlatformChoice,
}

/// Messages sent from the UI to the backend.
pub enum Command {
    Open(OpenParams),
    Preview {
        filters: Filters,
        limit: usize,
    },
    Export {
        params: ExportParams,
        cancel_token: Arc<AtomicBool>,
    },
    HtmlPreview {
        filters: Filters,
    },
    /// Render the HTML preview to a stable folder (copying attachments) for the
    /// embedded webview, without opening a browser.
    WebviewPreview {
        filters: Filters,
    },
    LoadCallLogs {
        filters: Filters,
        limit: usize,
    },
    ExportCallLogs(CallLogExportRequest),
    /// Label participants by phone number/email (`true`) or contact name
    /// (`false`) in subsequent previews, exports, and call-log views.
    SetShowNumbers(bool),
    Shutdown,
}

/// Everything the backend needs to write a filtered call-log export to disk.
pub struct CallLogExportRequest {
    pub filters: Filters,
    pub format: CallLogFormat,
    pub path: PathBuf,
    pub title: String,
}

/// Messages sent from the backend to the UI.
pub enum Event {
    Status(String),
    Opened {
        conversations: Vec<ConversationSummary>,
        date_range: Option<(String, String)>,
        total_messages: usize,
        summary: String,
        platform: PlatformChoice,
    },
    OpenFailed(String),
    Preview {
        messages: Vec<PreviewMessage>,
        total: i64,
    },
    PreviewFailed(String),
    Progress {
        current: u64,
        total: u64,
    },
    ExportDone {
        path: PathBuf,
        summary: String,
    },
    ExportCancelled,
    ExportFailed(String),
    HtmlPreviewReady(PathBuf),
    HtmlPreviewFailed(String),
    WebviewPreviewReady(PathBuf),
    WebviewPreviewFailed(String),
    CallLogsLoaded {
        entries: Vec<CallLogEntry>,
        total: i64,
        source: String,
    },
    CallLogsFailed(String),
    CallLogsExported {
        path: PathBuf,
        count: usize,
        format: CallLogFormat,
    },
    CallLogsExportFailed(String),
}

/// Spawn the backend worker, returning the command sender and event receiver.
pub fn spawn(ctx: egui::Context) -> (Sender<Command>, Receiver<Event>) {
    let (cmd_tx, cmd_rx) = channel::<Command>();
    let (evt_tx, evt_rx) = channel::<Event>();
    thread::Builder::new()
        .name("imessage-backend".into())
        .spawn(move || backend_loop(&cmd_rx, &evt_tx, &ctx))
        .expect("failed to spawn backend thread");
    (cmd_tx, evt_rx)
}

fn send(evt_tx: &Sender<Event>, ctx: &egui::Context, event: Event) {
    let _ = evt_tx.send(event);
    ctx.request_repaint();
}

fn backend_loop(cmd_rx: &Receiver<Command>, evt_tx: &Sender<Event>, ctx: &egui::Context) {
    let mut config: Option<Config> = None;
    let mut opened_source: Option<OpenedSource> = None;
    // Label participants by number (true) or contact name (false). Persisted
    // across opens since opening rebuilds the config.
    let mut show_numbers = false;
    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            Command::Shutdown => break,
            Command::SetShowNumbers(value) => {
                show_numbers = value;
                if let Some(config) = config.as_mut() {
                    config.prefer_handle_id = value;
                }
            }
            Command::Open(params) => {
                handle_open(&mut config, &mut opened_source, params, evt_tx, ctx);
                if let Some(config) = config.as_mut() {
                    config.prefer_handle_id = show_numbers;
                }
            }
            Command::Preview { filters, limit } => {
                handle_preview(config.as_ref(), &filters, limit, evt_tx, ctx)
            }
            Command::Export {
                params,
                cancel_token,
            } => handle_export(config.as_mut(), params, cancel_token, evt_tx, ctx),
            Command::HtmlPreview { filters } => {
                handle_html_preview(config.as_mut(), &filters, evt_tx, ctx)
            }
            Command::WebviewPreview { filters } => {
                handle_webview_preview(config.as_mut(), &filters, evt_tx, ctx)
            }
            Command::LoadCallLogs { filters, limit } => handle_load_call_logs(
                config.as_ref(),
                opened_source.as_ref(),
                &filters,
                limit,
                show_numbers,
                evt_tx,
                ctx,
            ),
            Command::ExportCallLogs(request) => handle_export_call_logs(
                config.as_ref(),
                opened_source.as_ref(),
                &request,
                evt_tx,
                ctx,
            ),
        }
    }
}

// MARK: Open

fn handle_open(
    config_slot: &mut Option<Config>,
    source_slot: &mut Option<OpenedSource>,
    params: OpenParams,
    evt_tx: &Sender<Event>,
    ctx: &egui::Context,
) {
    // Drop any previously open backup first so we release file handles.
    *config_slot = None;
    *source_slot = None;

    send(
        evt_tx,
        ctx,
        Event::Status(format!("Opening {} ...", params.db_path.display())),
    );

    let platform = match params.platform {
        PlatformChoice::Auto => match Platform::determine(&params.db_path) {
            Ok(p) => p,
            Err(why) => {
                send(
                    evt_tx,
                    ctx,
                    Event::OpenFailed(format!("Could not determine platform: {why}")),
                );
                return;
            }
        },
        PlatformChoice::MacOS => Platform::macOS,
        PlatformChoice::IOS => Platform::iOS,
    };
    let is_ios = matches!(platform, Platform::iOS);
    let opened_platform = if is_ios {
        PlatformChoice::IOS
    } else {
        PlatformChoice::MacOS
    };

    send(
        evt_tx,
        ctx,
        Event::Status("Building cache (this can take a moment on large databases) ...".into()),
    );

    let options = Options {
        db_path: params.db_path.clone(),
        attachment_root: params.attachment_root.clone(),
        attachment_manager: AttachmentManager::from(AttachmentManagerMode::Disabled),
        diagnostic: false,
        export_type: None,
        export_path: std::env::temp_dir().join("imessage-gui-placeholder"),
        query_context: QueryContext::default(),
        no_lazy: false,
        custom_name: None,
        use_caller_id: false,
        platform,
        ignore_disk_space: true,
        conversation_filter: None,
        cleartext_password: if is_ios {
            params.password.clone()
        } else {
            None
        },
        contacts_path: params.contacts_path.clone(),
        show_progress: false,
        export_call_logs: false,
        call_log_limit: None,
    };

    let config = match Config::new(options) {
        Ok(config) => config,
        Err(why) => {
            send(evt_tx, ctx, Event::OpenFailed(format!("{why}")));
            return;
        }
    };

    // Build the conversation list and summary stats.
    let counts = match config.message_counts_by_chat() {
        Ok(counts) => counts,
        Err(why) => {
            send(
                evt_tx,
                ctx,
                Event::OpenFailed(format!("Could not load conversation message counts: {why}")),
            );
            return;
        }
    };
    let conversations = build_conversations(&config, &counts);

    let diag = match Message::run_diagnostic(config.db()) {
        Ok(diag) => diag,
        Err(why) => {
            send(
                evt_tx,
                ctx,
                Event::OpenFailed(format!("Could not read message diagnostics: {why}")),
            );
            return;
        }
    };
    let date_range = match (diag.first_message_date, diag.last_message_date) {
        (Some(first), Some(last)) => {
            match (
                get_local_time(first, config.offset),
                get_local_time(last, config.offset),
            ) {
                (Ok(f), Ok(l)) => Some((fmt_date(&f), fmt_date(&l))),
                _ => None,
            }
        }
        _ => None,
    };
    let total_messages = diag.total_messages;

    let summary = format!(
        "{} - {} conversations - {} messages",
        if is_ios {
            "iOS backup"
        } else {
            "macOS database"
        },
        conversations.len(),
        total_messages
    );

    *source_slot = Some(OpenedSource {
        root_path: params.db_path,
        platform: opened_platform,
    });
    *config_slot = Some(config);

    send(
        evt_tx,
        ctx,
        Event::Opened {
            conversations,
            date_range,
            total_messages,
            summary,
            platform: opened_platform,
        },
    );
}

/// Resolve a handle id to a display name using the same maps the exporter uses.
fn handle_name(config: &Config, handle_id: i32) -> Option<String> {
    config
        .real_participants
        .get(&handle_id)
        .and_then(|internal| config.participants.get(internal))
        .map(|name| name.get_display_name().to_string())
}

/// Build a deduplicated conversation list from the cached `Config`.
fn build_conversations(
    config: &Config,
    counts: &std::collections::HashMap<i32, i64>,
) -> Vec<ConversationSummary> {
    // Group raw chat rowids by their deduplicated id.
    let mut groups: BTreeMap<i32, Vec<i32>> = BTreeMap::new();
    for raw in config.chatrooms.keys() {
        let real = *config.real_chatrooms.get(raw).unwrap_or(raw);
        groups.entry(real).or_default().push(*raw);
    }

    let mut out: Vec<ConversationSummary> = Vec::with_capacity(groups.len());
    for (real, raw_chat_ids) in groups {
        // Prefer an explicit display name from any underlying chat.
        let display_name = raw_chat_ids
            .iter()
            .filter_map(|id| config.chatrooms.get(id))
            .find_map(|chat| chat.display_name().map(|s| s.to_string()));

        // Union of participant handle ids across the grouped chats.
        let mut handle_ids: BTreeSet<i32> = BTreeSet::new();
        for id in &raw_chat_ids {
            if let Some(parts) = config.chatroom_participants.get(id) {
                handle_ids.extend(parts.iter().copied());
            }
        }

        let mut names: Vec<String> = handle_ids
            .iter()
            .map(|h| handle_name(config, *h).unwrap_or_else(|| "Unknown".to_string()))
            .collect();
        names.sort();
        names.dedup();
        let participants = if names.is_empty() {
            raw_chat_ids
                .iter()
                .filter_map(|id| config.chatrooms.get(id))
                .map(|chat| chat.chat_identifier.clone())
                .next()
                .unwrap_or_else(|| "Unknown".to_string())
        } else {
            names.join(", ")
        };

        let title = display_name.unwrap_or_else(|| participants.clone());
        let message_count: i64 = raw_chat_ids
            .iter()
            .map(|id| counts.get(id).copied().unwrap_or(0))
            .sum();

        out.push(ConversationSummary {
            id: real,
            raw_chat_ids,
            title,
            participants,
            message_count,
        });
    }

    out.sort_by_key(|a| a.title.to_lowercase());
    out
}

// MARK: Filters

/// Build a [`QueryContext`] from UI filters.
fn build_query_context(config: &Config, filters: &Filters) -> QueryContext {
    let mut qc = QueryContext {
        start: filters.start_ns,
        end: filters.end_ns,
        ..Default::default()
    };

    if !filters.selected_raw_chat_ids.is_empty() {
        qc.set_selected_chat_ids(filters.selected_raw_chat_ids.iter().copied().collect());
    } else if let Some(text) = filters
        .conversation_filter
        .as_ref()
        .filter(|t| !t.trim().is_empty())
    {
        config.apply_conversation_filter(&mut qc, text);
    }

    qc
}

// MARK: Preview

fn handle_preview(
    config: Option<&Config>,
    filters: &Filters,
    limit: usize,
    evt_tx: &Sender<Event>,
    ctx: &egui::Context,
) {
    let Some(config) = config else {
        send(
            evt_tx,
            ctx,
            Event::PreviewFailed("Open a backup before previewing.".into()),
        );
        return;
    };

    let qc = build_query_context(config, filters);

    match collect_preview(config, &qc, limit) {
        Ok(messages) => {
            let total = match Message::get_count(config.db(), &qc) {
                Ok(total) => total,
                Err(why) => {
                    send(
                        evt_tx,
                        ctx,
                        Event::PreviewFailed(format!("Could not count preview messages: {why}")),
                    );
                    return;
                }
            };
            send(evt_tx, ctx, Event::Preview { messages, total });
        }
        Err(why) => send(evt_tx, ctx, Event::PreviewFailed(why)),
    }
}

fn collect_preview(
    config: &Config,
    qc: &QueryContext,
    limit: usize,
) -> Result<Vec<PreviewMessage>, String> {
    pdf::collect_messages(config, qc, limit, false).map_err(|why| format!("{why}"))
}

// MARK: Call logs

fn handle_load_call_logs(
    config: Option<&Config>,
    source: Option<&OpenedSource>,
    filters: &Filters,
    limit: usize,
    show_numbers: bool,
    evt_tx: &Sender<Event>,
    ctx: &egui::Context,
) {
    let Some((config, source)) = require_ios_call_log_source(config, source, evt_tx, ctx) else {
        return;
    };

    send(evt_tx, ctx, Event::Status("Loading call logs ...".into()));

    let filter = match build_call_log_filter(config, filters) {
        Ok(filter) => filter,
        Err(why) => {
            send(evt_tx, ctx, Event::CallLogsFailed(why));
            return;
        }
    };
    match call_logs::load(config, &source.root_path, &filter, Some(limit)) {
        Ok(mut result) => {
            if !show_numbers {
                resolve_call_log_names(config, &mut result.entries);
            }
            send(
                evt_tx,
                ctx,
                Event::CallLogsLoaded {
                    entries: result.entries,
                    total: result.total,
                    source: result.source,
                },
            );
        }
        Err(why) => send(evt_tx, ctx, Event::CallLogsFailed(format!("{why}"))),
    }
}

/// Replace each call-log `address` with the contact name when one is known,
/// matching numbers loosely (trailing digits) and emails case-insensitively.
fn resolve_call_log_names(config: &Config, entries: &mut [CallLogEntry]) {
    let handle_addresses = Handle::cache(config.db()).unwrap_or_default();
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    for (handle_id, number) in &handle_addresses {
        let Some(internal) = config.real_participants.get(handle_id) else {
            continue;
        };
        let Some(name) = config.participants.get(internal) else {
            continue;
        };
        let display = name.get_display_name();
        // Only map when a real contact name (not just the raw number) exists.
        if !display.is_empty() && display != number {
            names
                .entry(contact_key(number))
                .or_insert_with(|| display.to_string());
        }
    }
    for entry in entries.iter_mut() {
        if let Some(name) = names.get(&contact_key(&entry.address)) {
            entry.address = name.clone();
        }
    }
}

/// A loose match key for an address: an email lowercased, or a phone number's
/// trailing significant digits.
fn contact_key(address: &str) -> String {
    let trimmed = address.trim();
    if trimmed.contains('@') {
        return trimmed.to_ascii_lowercase();
    }
    let digits: String = trimmed.chars().filter(char::is_ascii_digit).collect();
    if digits.len() > 10 {
        digits[digits.len() - 10..].to_string()
    } else {
        digits
    }
}

fn handle_export_call_logs(
    config: Option<&Config>,
    source: Option<&OpenedSource>,
    request: &CallLogExportRequest,
    evt_tx: &Sender<Event>,
    ctx: &egui::Context,
) {
    let Some((config, source)) = require_ios_call_log_source(config, source, evt_tx, ctx) else {
        return;
    };

    send(
        evt_tx,
        ctx,
        Event::Status(format!(
            "Exporting call logs to {} ...",
            request.path.display()
        )),
    );

    // No row limit: the export always covers the full filtered set, mirroring
    // the message exporter (the in-app table is capped, the export is not).
    let filter = match build_call_log_filter(config, &request.filters) {
        Ok(filter) => filter,
        Err(why) => {
            send(evt_tx, ctx, Event::CallLogsExportFailed(why));
            return;
        }
    };
    let result = match call_logs::load(config, &source.root_path, &filter, None) {
        Ok(result) => result,
        Err(why) => {
            send(evt_tx, ctx, Event::CallLogsExportFailed(format!("{why}")));
            return;
        }
    };

    match call_logs::write_call_logs(
        &result.entries,
        request.format,
        &request.title,
        &request.path,
    ) {
        Ok(()) => send(
            evt_tx,
            ctx,
            Event::CallLogsExported {
                path: request.path.clone(),
                count: result.entries.len(),
                format: request.format,
            },
        ),
        Err(why) => send(evt_tx, ctx, Event::CallLogsExportFailed(format!("{why}"))),
    }
}

/// Validate that an iOS backup is open, returning the config and source or
/// emitting the appropriate failure event.
fn require_ios_call_log_source<'a>(
    config: Option<&'a Config>,
    source: Option<&'a OpenedSource>,
    evt_tx: &Sender<Event>,
    ctx: &egui::Context,
) -> Option<(&'a Config, &'a OpenedSource)> {
    let fail = |evt_tx: &Sender<Event>, ctx: &egui::Context, message: &str| {
        send(evt_tx, ctx, Event::CallLogsFailed(message.to_string()));
    };

    let (Some(config), Some(source)) = (config, source) else {
        fail(evt_tx, ctx, "Open an iOS backup before loading call logs.");
        return None;
    };
    if source.platform != PlatformChoice::IOS {
        fail(
            evt_tx,
            ctx,
            "Call logs are available from iOS backup folders only.",
        );
        return None;
    }
    Some((config, source))
}

/// Resolve the GUI message filters into a [`CallLogFilter`] so the call-log view
/// and exports honor the same conversation/number and date selection as the
/// message preview. With nothing selected, the filter is unrestricted (all calls).
fn build_call_log_filter(config: &Config, filters: &Filters) -> Result<CallLogFilter, String> {
    let addresses = if !filters.selected_raw_chat_ids.is_empty() {
        addresses_for_chats(config, &filters.selected_raw_chat_ids)?
    } else if let Some(text) = filters
        .conversation_filter
        .as_ref()
        .filter(|text| !text.trim().is_empty())
    {
        text.split(',')
            .map(str::trim)
            .filter(|term| !term.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    } else {
        Vec::new()
    };

    Ok(CallLogFilter {
        addresses,
        start: filters.start_ns,
        end: filters.end_ns,
    })
}

/// Collect the participant phone numbers/emails for the given raw chat ids, plus
/// each chat's identifier (the number/email for 1:1 conversations).
fn addresses_for_chats(config: &Config, raw_chat_ids: &[i32]) -> Result<Vec<String>, String> {
    let handle_addresses = Handle::cache(config.db())
        .map_err(|why| format!("Could not resolve conversation phone numbers: {why}"))?;
    let mut seen = BTreeSet::new();
    let mut addresses = Vec::new();
    let mut push = |value: &str| {
        let value = value.trim();
        if !value.is_empty() && seen.insert(value.to_string()) {
            addresses.push(value.to_string());
        }
    };

    for chat_id in raw_chat_ids {
        if let Some(handle_ids) = config.chatroom_participants.get(chat_id) {
            for handle_id in handle_ids {
                if let Some(address) = handle_addresses.get(handle_id) {
                    push(address);
                }
            }
        }
        // 1:1 conversations carry the number/email as the chat identifier.
        if let Some(chat) = config.chatrooms.get(chat_id) {
            push(&chat.chat_identifier);
        }
    }
    Ok(addresses)
}

// MARK: Export

fn attachment_manager_mode(copy_method: CopyMethod) -> AttachmentManagerMode {
    match copy_method {
        CopyMethod::Disabled => AttachmentManagerMode::Disabled,
        CopyMethod::Clone => AttachmentManagerMode::Clone,
        CopyMethod::Basic => AttachmentManagerMode::Basic,
        CopyMethod::Full => AttachmentManagerMode::Full,
    }
}

fn apply_export_options(
    config: &mut Config,
    params: &ExportParams,
    export_type: ExportType,
    qc: QueryContext,
) {
    config.options.export_type = Some(export_type);
    config.options.export_path = params.export_path.clone();
    config.options.attachment_manager =
        AttachmentManager::from(attachment_manager_mode(params.copy_method));
    config.options.no_lazy = params.no_lazy;
    config.options.custom_name = params.custom_name.clone();
    config.options.use_caller_id = params.use_caller_id;
    config.options.ignore_disk_space = params.ignore_disk_space;
    // Filtering is fully resolved into the query context, so no text filter.
    config.options.conversation_filter = None;
    config.options.show_progress = false;
    config.options.export_call_logs = false;
    config.options.call_log_limit = None;
    config.options.query_context = qc;
}

fn handle_export(
    config: Option<&mut Config>,
    params: ExportParams,
    cancel_token: Arc<AtomicBool>,
    evt_tx: &Sender<Event>,
    ctx: &egui::Context,
) {
    let Some(config) = config else {
        send(
            evt_tx,
            ctx,
            Event::ExportFailed("Open a backup before exporting.".into()),
        );
        return;
    };

    let export_path = params.export_path.clone();
    let extension = match params.format {
        FormatChoice::Html => "html",
        FormatChoice::Txt => "txt",
        FormatChoice::Pdf => "pdf",
    };

    // Guard against appending into an export that already exists.
    if let Err(why) = ensure_export_dir_clear(&export_path, extension) {
        send(evt_tx, ctx, Event::ExportFailed(why));
        return;
    }

    let export_type = match params.format {
        FormatChoice::Html => ExportType::Html,
        FormatChoice::Txt => ExportType::Txt,
        FormatChoice::Pdf => ExportType::Pdf,
    };

    let qc = build_query_context(config, &params.filters);
    let total = match Message::get_count(config.db(), &qc) {
        Ok(total) => total,
        Err(why) => {
            send(
                evt_tx,
                ctx,
                Event::ExportFailed(format!("Could not count messages for export: {why}")),
            );
            return;
        }
    };
    apply_export_options(config, &params, export_type, qc);
    cancel_token.store(false, Ordering::Relaxed);

    // Install a progress callback that forwards to the UI.
    let etx = evt_tx.clone();
    let cctx = ctx.clone();
    let callback: ProgressCallback = Arc::new(move |current, total| {
        let _ = etx.send(Event::Progress { current, total });
        cctx.request_repaint();
    });
    config.progress_callback = Some(callback);
    let token = cancel_token.clone();
    let cancel_callback: CancelCallback = Arc::new(move || token.load(Ordering::Relaxed));
    config.cancel_callback = Some(cancel_callback);

    send(
        evt_tx,
        ctx,
        Event::Status(format!(
            "Exporting {total} messages to {} ...",
            export_path.display()
        )),
    );

    let result = config.start();
    config.progress_callback = None;
    config.cancel_callback = None;
    cancel_token.store(false, Ordering::Relaxed);

    match result {
        Ok(()) => send(
            evt_tx,
            ctx,
            Event::ExportDone {
                path: export_path.clone(),
                summary: format!("Exported {total} messages to {}", export_path.display()),
            },
        ),
        Err(RuntimeError::Cancelled) => send(evt_tx, ctx, Event::ExportCancelled),
        Err(why) => send(evt_tx, ctx, Event::ExportFailed(format!("{why}"))),
    }
}

/// Return an error if `dir` already contains files with the target extension.
fn ensure_export_dir_clear(dir: &Path, extension: &str) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("Cannot read export folder {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry
            .map_err(|e| format!("Cannot read entry in export folder {}: {e}", dir.display()))?;
        if entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case(extension))
        {
            return Err(format!(
                "Export folder {} already contains .{extension} files. Choose an empty folder to avoid mixing exports.",
                dir.display()
            ));
        }
    }
    Ok(())
}

// MARK: HTML preview

fn handle_html_preview(
    config: Option<&mut Config>,
    filters: &Filters,
    evt_tx: &Sender<Event>,
    ctx: &egui::Context,
) {
    let Some(config) = config else {
        send(
            evt_tx,
            ctx,
            Event::HtmlPreviewFailed("Open a backup before previewing.".into()),
        );
        return;
    };

    // Unique temp directory for this preview render.
    let stamp = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(why) => {
            send(
                evt_tx,
                ctx,
                Event::HtmlPreviewFailed(format!("Could not create preview timestamp: {why}")),
            );
            return;
        }
    };
    let dir = std::env::temp_dir().join(format!("imessage-gui-preview-{stamp}"));
    if let Err(why) = std::fs::create_dir_all(&dir) {
        send(
            evt_tx,
            ctx,
            Event::HtmlPreviewFailed(format!("Could not create preview folder: {why}")),
        );
        return;
    }

    send(
        evt_tx,
        ctx,
        Event::Status("Rendering HTML preview ...".into()),
    );

    match render_html_preview(config, filters, &dir, CopyMethod::Disabled) {
        Ok(file) => {
            if let Err(why) = open::that_detached(&file) {
                send(
                    evt_tx,
                    ctx,
                    Event::HtmlPreviewFailed(format!("Could not open preview: {why}")),
                );
            } else {
                send(evt_tx, ctx, Event::HtmlPreviewReady(file));
            }
        }
        Err(why) => send(evt_tx, ctx, Event::HtmlPreviewFailed(why)),
    }
}

/// Render the HTML preview into a stable folder (copying attachments out of the
/// backup so the webview can display/play them) for the embedded preview.
fn handle_webview_preview(
    config: Option<&mut Config>,
    filters: &Filters,
    evt_tx: &Sender<Event>,
    ctx: &egui::Context,
) {
    let Some(config) = config else {
        send(
            evt_tx,
            ctx,
            Event::WebviewPreviewFailed("Open a backup before previewing.".into()),
        );
        return;
    };

    // Reuse one folder so the webview can reload in place; clear stale output and
    // copied attachments first.
    let dir = std::env::temp_dir().join("imessage-gui-webview-preview");
    let _ = std::fs::remove_dir_all(&dir);

    send(evt_tx, ctx, Event::Status("Rendering preview ...".into()));

    match render_html_preview(config, filters, &dir, CopyMethod::Clone) {
        Ok(file) => send(evt_tx, ctx, Event::WebviewPreviewReady(file)),
        Err(why) => send(evt_tx, ctx, Event::WebviewPreviewFailed(why)),
    }
}

/// Render the current selection to HTML in `dir` and return the conversation
/// file (the largest `.html`). Shared by the browser and webview previews.
fn render_html_preview(
    config: &mut Config,
    filters: &Filters,
    dir: &Path,
    copy_method: CopyMethod,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir)
        .map_err(|why| format!("Could not create preview folder: {why}"))?;

    let preview_params = ExportParams {
        filters: filters.clone(),
        format: FormatChoice::Html,
        export_path: dir.to_path_buf(),
        copy_method,
        no_lazy: true,
        custom_name: config.options.custom_name.clone(),
        use_caller_id: config.options.use_caller_id,
        ignore_disk_space: true,
    };

    let qc = build_query_context(config, filters);
    apply_export_options(config, &preview_params, ExportType::Html, qc);
    config.progress_callback = None;
    config.cancel_callback = None;

    config.start().map_err(|why| format!("{why}"))?;

    match largest_html_file(dir)? {
        Some(file) => std::fs::canonicalize(&file)
            .map_err(|why| format!("Could not resolve preview file {}: {why}", file.display())),
        None => Err("No messages matched the current filters.".into()),
    }
}

/// Find the largest `.html` file in `dir` (the conversation file, as opposed to
/// the near-empty `orphaned.html`).
fn largest_html_file(dir: &Path) -> Result<Option<PathBuf>, String> {
    let mut best: Option<(u64, PathBuf)> = None;
    let entries =
        std::fs::read_dir(dir).map_err(|why| format!("Could not read preview folder: {why}"))?;
    for entry in entries {
        let entry = entry.map_err(|why| format!("Could not read preview folder entry: {why}"))?;
        let path = entry.path();
        if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("html"))
        {
            let size = entry
                .metadata()
                .map_err(|why| format!("Could not inspect preview file {}: {why}", path.display()))?
                .len();
            if best.as_ref().is_none_or(|(b, _)| size > *b) {
                best = Some((size, path));
            }
        }
    }
    Ok(best.map(|(_, p)| p))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `Config` against the bundled synthetic test database, mirroring
    /// how `handle_open` constructs one.
    fn test_config() -> Config {
        let db_path = std::env::current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("imessage-database/test_data/db/test.db");
        let options = Options {
            db_path,
            attachment_root: None,
            attachment_manager: AttachmentManager::from(AttachmentManagerMode::Disabled),
            diagnostic: false,
            export_type: None,
            export_path: std::env::temp_dir().join("imessage-gui-test"),
            query_context: QueryContext::default(),
            no_lazy: false,
            custom_name: None,
            use_caller_id: false,
            platform: Platform::macOS,
            ignore_disk_space: true,
            conversation_filter: None,
            cleartext_password: None,
            contacts_path: None,
            show_progress: false,
            export_call_logs: false,
            call_log_limit: None,
        };
        Config::new(options).expect("failed to open bundled test database")
    }

    #[test]
    fn builds_conversation_list_without_panicking() {
        let config = test_config();
        // The synthetic fixture has no chats; this must still succeed cleanly.
        let counts = config
            .message_counts_by_chat()
            .expect("message counts should load");
        let _ = build_conversations(&config, &counts);
    }

    #[test]
    fn previews_messages_from_test_db() {
        let config = test_config();
        let messages =
            collect_preview(&config, &QueryContext::default(), 100).expect("preview failed");
        assert!(
            !messages.is_empty(),
            "expected at least one preview message"
        );
        for message in &messages {
            assert!(
                !message.timestamp.is_empty(),
                "every preview message should have a timestamp"
            );
            assert!(
                !message.sender.is_empty(),
                "every preview message should have a sender"
            );
        }
    }

    #[test]
    fn date_filter_narrows_results() {
        let config = test_config();
        let all = Message::get_count(config.db(), &QueryContext::default())
            .expect("message count should load");
        // A start date far in the future should exclude everything.
        let future = QueryContext {
            start: crate::model::parse_local_timestamp("2099-01-01", "00:00").ok(),
            ..Default::default()
        };
        let none = collect_preview(&config, &future, 100).unwrap();
        assert!(all >= 1);
        assert!(
            none.is_empty(),
            "future start date should exclude all messages"
        );
    }

    #[test]
    fn largest_html_file_prefers_largest_html_file() {
        let dir = std::env::temp_dir().join(format!(
            "imessage-gui-html-preview-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be valid")
                .as_nanos()
        ));
        std::fs::create_dir(&dir).expect("create temp preview dir");
        let small = dir.join("small.html");
        let large = dir.join("large.HTML");
        std::fs::write(&small, b"small").expect("write small html");
        std::fs::write(&large, b"large html file").expect("write large html");
        std::fs::write(dir.join("style.css"), b"body {}").expect("write css");

        let found = largest_html_file(&dir)
            .expect("preview scan should succeed")
            .expect("html file should be found");

        assert_eq!(found, large);
        std::fs::remove_dir_all(&dir).expect("cleanup temp preview dir");
    }

    #[test]
    fn largest_html_file_reports_unreadable_preview_folder() {
        let missing = std::env::temp_dir().join("imessage-gui-missing-preview-folder");
        let err = largest_html_file(&missing).expect_err("missing folder should error");
        assert!(err.contains("Could not read preview folder"));
    }
}
