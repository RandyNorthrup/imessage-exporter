/*!
Minimal SVG builder shared by the Digital Touch effect renderers.

Digital Touch effects are animated on-device. We render a single, static frame
that depicts the captured data (the drawn strokes, the tap and kiss locations,
the fireball's path, a heart with its rate) onto a black canvas, matching the
black backdrop the effects are composed on in Messages. The canvas is `4:5`,
the (device-dependent) aspect ratio of the Apple Watch screen the effects are
authored and displayed on.
*/

use std::fmt::Write;

use crate::message_types::digital_touch::models::ImageBackdrop;

/// Canvas width in user units. The emitted `<svg>` scales to its container, so
/// this only fixes the internal coordinate space.
pub(super) const WIDTH: usize = 768;
/// Canvas height in user units, giving a `4:5` portrait aspect ratio.
pub(super) const HEIGHT: usize = 960;

/// Accumulates SVG markup for one effect, then wraps it in an `<svg>` root.
pub(super) struct Canvas {
    title: String,
    defs: String,
    body: String,
    /// Pre-rendered `<image>` markup drawn behind the effect, in place of the
    /// plain black canvas.
    backdrop: Option<String>,
}

impl Canvas {
    /// Create a canvas with the given accessible `<title>` and optional still-image backdrop.
    pub(super) fn new(title: impl Into<String>, background: Option<ImageBackdrop<'_>>) -> Self {
        Self {
            title: title.into(),
            defs: String::new(),
            body: String::new(),
            backdrop: background.map(|background| background.render(WIDTH, HEIGHT)),
        }
    }

    /// Whether a still-image backdrop was supplied.
    pub(super) fn has_background(&self) -> bool {
        self.backdrop.is_some()
    }

    /// Canvas width, for renderers that place elements absolutely.
    pub(super) fn width(&self) -> usize {
        WIDTH
    }

    /// Canvas height, for renderers that place elements absolutely.
    pub(super) fn height(&self) -> usize {
        HEIGHT
    }

    /// Map a normalized x coordinate (`0..=u16::MAX`, left to right) onto canvas
    /// user units.
    pub(super) fn fit_x(&self, value: u16) -> usize {
        usize::from(value) * WIDTH / usize::from(u16::MAX)
    }

    /// Map a normalized y coordinate onto canvas user units. Digital Touch uses a
    /// bottom-left origin (y grows upward), so this inverts onto SVG's top-left,
    /// y-down space.
    pub(super) fn fit_y(&self, value: u16) -> usize {
        HEIGHT - usize::from(value) * HEIGHT / usize::from(u16::MAX)
    }

    /// Append an element to the canvas body.
    pub(super) fn push(&mut self, markup: &str) {
        self.body.push_str(markup);
        self.body.push('\n');
    }

    /// Append an entry to the `<defs>` block (e.g. a gradient).
    pub(super) fn push_def(&mut self, markup: &str) {
        self.defs.push_str(markup);
        self.defs.push('\n');
    }

    /// Render the accumulated markup into a complete `<svg>` document.
    pub(super) fn finish(self) -> String {
        let mut svg = String::with_capacity(self.body.len() + self.defs.len() + 256);
        let _ = write!(
            svg,
            r#"<svg viewBox="0 0 {WIDTH} {HEIGHT}" preserveAspectRatio="xMidYMid meet" width="100%" height="100%" xmlns="http://www.w3.org/2000/svg">"#
        );
        svg.push('\n');
        let _ = writeln!(svg, "<title>{}</title>", escape(&self.title));
        if !self.defs.is_empty() {
            svg.push_str("<defs>\n");
            svg.push_str(&self.defs);
            svg.push_str("</defs>\n");
        }
        let _ = writeln!(
            svg,
            r#"<rect width="{WIDTH}" height="{HEIGHT}" fill="black" />"#
        );
        if let Some(backdrop) = &self.backdrop {
            svg.push_str(backdrop);
            svg.push('\n');
        }
        svg.push_str(&self.body);
        svg.push_str("</svg>\n");
        svg
    }
}

/// Escape the characters that are significant inside SVG/XML text content.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
