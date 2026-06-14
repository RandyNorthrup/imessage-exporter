/*!
 Compact, read-only plain-text rendering of a single message, used by front-ends
 that show a message preview (the desktop GUI's in-app preview and the PDF
 exporter's chat bubbles).

 [`Preview`] reuses the [`TXT`] exporter for every
 balloon, app, data-detector, edit, and announcement format (so every message
 feature the library supports, now and in the future, surfaces here for free)
 and customizes only two things a preview needs:

  - **Attachments** render as compact descriptors (`[Image]`, `[Video]`,
    `[Audio]`, `[Sticker]`, ...) instead of file paths, and crucially **without
    the disk side effects** the TXT exporter performs (copying/converting files).
  - The per-message timestamp/sender header is omitted; front-ends draw their own.
*/

use imessage_database::{
    message_types::edited::EditedMessage,
    tables::{
        attachment::{Attachment, MediaType},
        messages::{
            Message,
            models::{AttachmentMeta, AttributedRange, SharedLocation},
        },
    },
};

use crate::{
    app::{error::RuntimeError, runtime::Config},
    exporters::{
        formatter::{AttachmentRender, MessageFormatter, PartBodyBuilder, RenderContext},
        shared::{
            driver::ExportState,
            message::MessageContext,
            part::{AttachmentResolver, dispatch_part_body, resolve_run},
        },
        txt::TXT,
    },
};

const ATTACHMENT_MISSING_TEXT: &str = "Attachment missing!";

/// A front-end-agnostic rendering of one message: the full body text (covering
/// every component and balloon type) plus short annotation badges. Excludes the
/// timestamp/sender header, which each front-end renders itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessagePreview {
    /// The rendered message body, one line per component/part.
    pub body: String,
    /// Short badges describing context (reply, replies count, edited, effect).
    pub annotations: Vec<String>,
}

/// Renders messages into [`MessagePreview`] values. Holds an inner [`TXT`]
/// exporter (built with a scratch `ExportState` so it creates no export files)
/// and delegates all rich formatting to it, overriding only attachment handling.
pub struct Preview<'a> {
    inner: TXT<'a>,
}

impl<'a> Preview<'a> {
    /// Build a preview renderer over `config`. Creates no files in the export
    /// directory.
    pub fn new(config: &'a Config) -> Result<Self, RuntimeError> {
        Ok(Self {
            inner: TXT {
                config,
                state: ExportState::scratch()?,
            },
        })
    }

    /// Render `message` to its body text and annotations.
    pub fn render(&self, message: &Message) -> Result<MessagePreview, RuntimeError> {
        // Announcements (group renames, etc.) have no body components; render the
        // announcement line directly.
        if message.is_announcement() {
            let mut out = String::new();
            self.format_announcement(message, &mut out);
            return Ok(MessagePreview {
                body: out.trim().to_string(),
                annotations: Vec::new(),
            });
        }

        let mut ctx = MessageContext::resolve(message, self.inner.config.data_source.db())?;
        let mut resolver = AttachmentResolver::new(&ctx.attachments);

        let mut parts: Vec<String> = Vec::with_capacity(message.components.len());
        for (idx, message_part) in message.components.iter().enumerate() {
            let body = dispatch_part_body(
                self,
                message,
                idx,
                message_part,
                &mut ctx.attachments,
                &mut resolver,
            );
            let trimmed = body.trim();
            if !trimmed.is_empty() {
                parts.push(trimmed.to_string());
            }
        }

        Ok(MessagePreview {
            body: parts.join("\n"),
            annotations: annotations(message),
        })
    }
}

/// Short context badges shown alongside the body.
fn annotations(message: &Message) -> Vec<String> {
    let mut out = Vec::new();
    if message.is_reply() {
        out.push("reply".to_string());
    }
    if message.has_replies() {
        let n = message.num_replies;
        out.push(format!("{n} repl{}", if n == 1 { "y" } else { "ies" }));
    }
    if message.is_edited() {
        out.push("edited".to_string());
    }
    if message.is_expressive() {
        out.push("effect".to_string());
    }
    out
}

/// A compact, side-effect-free descriptor for an attachment or sticker.
fn attachment_descriptor(attachment: &Attachment) -> String {
    if attachment.is_sticker {
        return "[Sticker]".to_string();
    }
    match attachment.mime_type() {
        MediaType::Image(_) => "[Image]",
        MediaType::Video(_) => "[Video]",
        MediaType::Audio(_) => "[Audio]",
        MediaType::Text(_) => "[Text]",
        MediaType::Application(_) | MediaType::Other(_) | MediaType::Unknown => "[Attachment]",
    }
    .to_string()
}

impl<'a> MessageFormatter<'a> for Preview<'a> {
    // --- Customized: attachments render as descriptors, with no disk I/O. ---

    fn format_attachment(
        &self,
        attachment: &'a mut Attachment,
        _msg: &'a Message,
        _metadata: &AttachmentMeta,
    ) -> AttachmentRender {
        AttachmentRender::Embedded(attachment_descriptor(attachment))
    }

    fn format_sticker(&self, sticker: &'a mut Attachment, _msg: &'a Message) -> String {
        attachment_descriptor(sticker)
    }

    /// Mirrors [`TXT::render_run`] but resolves attachment ranges through this
    /// type's `format_attachment`/`format_sticker` (descriptors, no side effects).
    fn render_run(
        &'a self,
        message: &'a Message,
        ranges: &'a [AttributedRange],
        attachments: &'a mut Vec<Attachment>,
        resolver: &mut AttachmentResolver,
    ) -> <Self as PartBodyBuilder>::Body {
        let text = message.text.as_deref().unwrap_or_default();

        if ranges.iter().all(|range| range.attachment.is_none()) {
            let attr_text = self.format_attributes(text, ranges);
            let formatted = if attr_text.is_empty() {
                self.body_escape(text)
            } else {
                attr_text
            };
            return self.body_text_with_translation(message, formatted);
        }

        let mut lines: Vec<String> = Vec::with_capacity(ranges.len());
        for (range, idx) in resolve_run(ranges, resolver) {
            if let (Some(meta), Some(idx)) = (range.attachment.as_ref(), idx) {
                let line = match attachments.get_mut(idx) {
                    Some(attachment) if attachment.is_sticker => {
                        self.format_sticker(attachment, message)
                    }
                    Some(attachment) => match self.format_attachment(attachment, message, meta) {
                        AttachmentRender::Embedded(content) => content,
                        AttachmentRender::MissingFilename => ATTACHMENT_MISSING_TEXT.to_string(),
                        AttachmentRender::NamedFile(name) => name,
                    },
                    None => ATTACHMENT_MISSING_TEXT.to_string(),
                };
                lines.push(line);
            } else {
                let segment = self.format_attributes(text, std::slice::from_ref(range));
                if !segment.is_empty() {
                    lines.push(segment);
                }
            }
        }
        self.body_text_with_translation(message, lines.join("\n"))
    }

    // --- Delegated: reuse the TXT exporter for all other formatting. ---

    fn format_app(
        &self,
        msg: &'a Message,
        attachments: &mut Vec<Attachment>,
    ) -> Result<String, RuntimeError> {
        self.inner.format_app(msg, attachments)
    }

    fn format_tapback(&self, msg: &Message) -> Result<String, RuntimeError> {
        self.inner.format_tapback(msg)
    }

    fn format_announcement(&self, msg: &Message, out: &mut String) {
        self.inner.format_announcement(msg, out);
    }

    fn format_shareplay(&self) -> &'static str {
        self.inner.format_shareplay()
    }

    fn format_shared_location(&self, kind: SharedLocation) -> &'static str {
        self.inner.format_shared_location(kind)
    }

    fn format_edited(
        &'a self,
        msg: &'a Message,
        edited_message: &'a EditedMessage,
        message_part_idx: usize,
        attachments: &'a mut Vec<Attachment>,
        resolver: &mut AttachmentResolver,
    ) -> Option<String> {
        self.inner
            .format_edited(msg, edited_message, message_part_idx, attachments, resolver)
    }

    fn format_attributes(&self, text: &str, ranges: &[AttributedRange]) -> String {
        self.inner.format_attributes(text, ranges)
    }

    fn format_message_into(
        &self,
        message: &Message,
        context: RenderContext,
        out: &mut String,
    ) -> Result<(), RuntimeError> {
        self.inner.format_message_into(message, context, out)
    }
}

/// The TXT exporter's `Body` is a view-model (`PartBody`) rendered through a
/// template; the preview wants plain text directly, so these construct strings.
/// The incoming `content` is already the formatted body text.
impl PartBodyBuilder for Preview<'_> {
    type Body = String;

    fn body_empty(&self) -> Self::Body {
        String::new()
    }
    fn body_text_bubble(&self, content: String) -> Self::Body {
        content
    }
    fn body_text_translated(&self, translated: String, original: String) -> Self::Body {
        if translated.trim().is_empty() {
            original
        } else {
            format!("{original}\n{translated}")
        }
    }
    fn body_text_edited(&self, content: String) -> Self::Body {
        content
    }
    fn body_attachment(&self, content: String) -> Self::Body {
        content
    }
    fn body_attachment_error(&self, error: &str) -> Self::Body {
        error.to_string()
    }
    fn body_attachment_missing(&self) -> Self::Body {
        ATTACHMENT_MISSING_TEXT.to_string()
    }
    fn body_sticker(&self, content: String) -> Self::Body {
        content
    }
    fn body_app(&self, content: String) -> Self::Body {
        content
    }
    fn body_app_error(&self, _message: &Message, why: String) -> Self::Body {
        format!("Unable to format app message: {why}")
    }
    fn body_retracted(&self, content: String) -> Self::Body {
        content
    }
    fn body_escape(&self, text: &str) -> String {
        text.to_string()
    }
    fn config(&self) -> &Config {
        self.inner.config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config, Options, app::export_type::ExportType};

    fn attachment_with_mime(mime: &str, is_sticker: bool) -> Attachment {
        Attachment {
            rowid: 1,
            guid: None,
            filename: Some("path/to/secret-file.heic".to_string()),
            uti: None,
            mime_type: Some(mime.to_string()),
            transfer_name: Some("secret-file.heic".to_string()),
            total_bytes: 0,
            is_sticker,
            hide_attachment: 0,
            emoji_description: None,
            copied_path: None,
        }
    }

    #[test]
    fn attachment_descriptor_classifies_media_without_leaking_paths() {
        assert_eq!(
            attachment_descriptor(&attachment_with_mime("image/png", false)),
            "[Image]"
        );
        assert_eq!(
            attachment_descriptor(&attachment_with_mime("video/mp4", false)),
            "[Video]"
        );
        assert_eq!(
            attachment_descriptor(&attachment_with_mime("audio/x-m4a", false)),
            "[Audio]"
        );
        assert_eq!(
            attachment_descriptor(&attachment_with_mime("application/pdf", false)),
            "[Attachment]"
        );
        // A sticker is labeled regardless of its underlying MIME type.
        assert_eq!(
            attachment_descriptor(&attachment_with_mime("image/png", true)),
            "[Sticker]"
        );
        // The descriptor never exposes the file name or path on disk.
        let descriptor = attachment_descriptor(&attachment_with_mime("image/png", false));
        assert!(!descriptor.contains("secret-file"));
        assert!(!descriptor.contains('/'));
    }

    #[test]
    fn preserves_emoji_and_unicode_in_body() {
        let config = Config::fake_app(Options::fake_options(ExportType::Txt));
        let preview = Preview::new(&config).expect("build preview");

        // Emoji (incl. skin tone), accents, currency, CJK, and punctuation must
        // pass through the renderer unchanged - nothing is sanitized.
        let original = "Hi \u{1F44B}\u{1F3FD} caf\u{e9} \u{4F60}\u{597D} \u{20AC}5 \u{2705}";
        let mut message = Config::fake_message();
        message.text = Some(original.to_string());
        message.chat_id = Some(0);
        message
            .generate_text_legacy(config.data_source.db())
            .expect("generate body");

        let rendered = preview.render(&message).expect("render");
        assert_eq!(rendered.body, original);
    }

    #[test]
    fn renders_plain_text_body_without_metadata_header() {
        let config = Config::fake_app(Options::fake_options(ExportType::Txt));
        let preview = Preview::new(&config).expect("build preview");

        let mut message = Config::fake_message();
        message.text = Some("Hello world".to_string());
        message.chat_id = Some(0);
        message
            .generate_text_legacy(config.data_source.db())
            .expect("generate body");

        let rendered = preview.render(&message).expect("render");
        // Body is the message content only, with no timestamp/sender header.
        assert_eq!(rendered.body, "Hello world");
        assert!(rendered.annotations.is_empty());
    }

    #[test]
    fn annotations_flag_reply_counts() {
        let mut message = Config::fake_message();
        assert!(annotations(&message).is_empty());

        message.num_replies = 2;
        let badges = annotations(&message);
        assert!(badges.iter().any(|badge| badge == "2 replies"));
    }
}
