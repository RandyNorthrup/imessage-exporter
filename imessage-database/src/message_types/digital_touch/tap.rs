/*!
[Tap](super) Digital Touch effect: one or more colored bursts at points.
*/

use protobuf::Message;

use crate::{
    error::digital_touch::DigitalTouchError,
    message_types::digital_touch::{
        digital_touch_proto::{BaseMessage, TapMessage},
        models::{Color, DigitalTouchMessage, Point, decode_points, decode_u16s, pluralize},
        svg::Canvas,
    },
};

/// Per-tap data: the burst color and its delay from the previous tap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TapData {
    /// Color of the burst.
    pub color: Color,
    /// Delay before this tap, in milliseconds.
    pub delay_ms: u16,
}

/// A series of taps, each a colored burst at a point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigitalTouchTap {
    /// Unique identifier for the message.
    pub id: String,
    /// The taps, in order.
    pub taps: Vec<Point<TapData>>,
}

impl DigitalTouchTap {
    /// Parse the [`TapMessage`] carried by `base` into a [`DigitalTouchMessage`].
    pub(super) fn from_payload(
        base: &BaseMessage,
    ) -> Result<DigitalTouchMessage, DigitalTouchError> {
        let msg = TapMessage::parse_from_bytes(&base.TouchPayload)
            .map_err(DigitalTouchError::ProtobufError)?;

        let colors = Color::decode_all(&msg.Color);
        let delays = decode_u16s(&msg.Delays);
        if colors.len() != delays.len() {
            return Err(DigitalTouchError::ArraysDoNotMatch(
                "colors",
                colors.len(),
                "delays",
                delays.len(),
            ));
        }

        let extras = colors
            .into_iter()
            .zip(delays)
            .map(|(color, delay_ms)| TapData { color, delay_ms })
            .collect();

        Ok(DigitalTouchMessage::Tap(DigitalTouchTap {
            id: base.ID.clone(),
            taps: decode_points(&msg.Location, extras)?,
        }))
    }

    /// One-line summary, e.g. `"Digital Touch Tap (1 tap)"`.
    pub(super) fn summary(&self) -> String {
        format!("Digital Touch Tap ({})", pluralize(self.taps.len(), "tap"))
    }

    /// Draw each tap as a colored ring with a filled center.
    pub(super) fn append_svg(&self, canvas: &mut Canvas) {
        let ring = canvas.width() / 12;
        let dot = canvas.width() / 40;
        for tap in &self.taps {
            let cx = canvas.fit_x(tap.x);
            let cy = canvas.fit_y(tap.y);
            let color = tap.extra.color.css();
            canvas.push(&format!(
                r#"<circle cx="{cx}" cy="{cy}" r="{ring}" fill="none" stroke="{color}" stroke-width="6" />"#
            ));
            canvas.push(&format!(
                r#"<circle cx="{cx}" cy="{cy}" r="{dot}" fill="{color}" />"#
            ));
        }
    }
}
