//! Embedded system webview used to render the HTML message preview in-app, so
//! the preview shows exactly what the device shows: color emoji, stickers,
//! inline images, link cards, and playable video/voice attachments.
//!
//! The webview is a native child of the app window (via [`wry`]), positioned
//! over the preview pane each frame. It composes with the winit-based `eframe`
//! window on macOS and Windows; on other platforms it is unsupported and the GUI
//! falls back to the egui bubble preview and the "Open HTML preview" button.

use std::path::Path;

use eframe::egui;

/// Whether an embedded webview can be created on this platform.
pub const SUPPORTED: bool = cfg!(any(target_os = "macos", target_os = "windows"));

/// Shown before any conversation has been previewed.
#[cfg(any(target_os = "macos", target_os = "windows"))]
const PLACEHOLDER_HTML: &str = "<!DOCTYPE html><html><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"></head>\
<body style=\"margin:0;height:100vh;display:flex;align-items:center;justify-content:center;\
font-family:-apple-system,Segoe UI,Roboto,sans-serif;color:#535d6a;background:#fafbfd;\">\
Select a conversation to preview it here.</body></html>";

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use imp::PreviewWebview;

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod imp {
    use super::{Path, PreviewBounds, PLACEHOLDER_HTML};
    use eframe::egui;
    use wry::{
        dpi::{PhysicalPosition, PhysicalSize},
        Rect, WebViewBuilder,
    };

    /// A native webview parented to the app window.
    pub struct PreviewWebview {
        view: wry::WebView,
    }

    impl PreviewWebview {
        /// Create the child webview parented to the eframe window. Returns `None`
        /// if the platform webview could not be created.
        pub fn create(frame: &eframe::Frame) -> Option<Self> {
            let view = WebViewBuilder::new()
                .with_bounds(Rect {
                    position: PhysicalPosition::new(0, 0).into(),
                    size: PhysicalSize::new(0, 0).into(),
                })
                .with_html(PLACEHOLDER_HTML)
                // Links (and mailto:/tel:) open in the default app, not in the
                // preview pane; only the local preview file (and its in-page
                // anchors) navigate inside the webview.
                .with_navigation_handler(|url| {
                    if super::is_local_preview_url(&url) {
                        true
                    } else {
                        let _ = open::that_detached(&url);
                        false
                    }
                })
                // target="_blank" links request a new window; open externally too.
                .with_new_window_req_handler(|url, _features| {
                    let _ = open::that_detached(&url);
                    wry::NewWindowResponse::Deny
                })
                .build_as_child(frame)
                .ok()?;
            Some(Self { view })
        }

        /// Position the webview over `rect` (egui points), converting to physical
        /// pixels with `pixels_per_point`.
        pub fn set_bounds(&self, rect: egui::Rect, pixels_per_point: f32) {
            let b = PreviewBounds::from_rect(rect, pixels_per_point);
            let _ = self.view.set_bounds(Rect {
                position: PhysicalPosition::new(b.x, b.y).into(),
                size: PhysicalSize::new(b.w, b.h).into(),
            });
        }

        pub fn set_visible(&self, visible: bool) {
            let _ = self.view.set_visible(visible);
        }

        /// Load a local HTML file into the webview.
        pub fn load_file(&self, path: &Path) {
            let _ = self.view.load_url(&super::file_url(path));
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub use stub::PreviewWebview;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod stub {
    use super::Path;
    use eframe::egui;

    /// No-op webview for platforms where embedding is unsupported.
    pub struct PreviewWebview;

    impl PreviewWebview {
        pub fn create(_frame: &eframe::Frame) -> Option<Self> {
            None
        }
        pub fn set_bounds(&self, _rect: egui::Rect, _pixels_per_point: f32) {}
        pub fn set_visible(&self, _visible: bool) {}
        pub fn load_file(&self, _path: &Path) {}
    }
}

/// Webview bounds in physical pixels, derived from an egui rect.
#[cfg(any(target_os = "macos", target_os = "windows"))]
struct PreviewBounds {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl PreviewBounds {
    fn from_rect(rect: egui::Rect, pixels_per_point: f32) -> Self {
        let scale = pixels_per_point.max(0.0);
        Self {
            x: (rect.min.x * scale).round() as i32,
            y: (rect.min.y * scale).round() as i32,
            w: (rect.width() * scale).round().max(0.0) as u32,
            h: (rect.height() * scale).round().max(0.0) as u32,
        }
    }
}

/// Build a `file://` URL for a local path.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn file_url(path: &Path) -> String {
    format!("file://{}", path.display())
}

/// Whether a URL refers to the local preview document (so it should load inside
/// the webview) rather than an external link (which opens in the default app).
/// In-page anchors keep the `file://` scheme, so threaded replies still work.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn is_local_preview_url(url: &str) -> bool {
    url.starts_with("file://") || url.starts_with("about:") || url.starts_with("data:")
}
