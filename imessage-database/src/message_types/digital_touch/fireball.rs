/*!
[Fireball](super) Digital Touch effect: a ball of fire dragged along a path.
*/

use protobuf::Message;

use crate::{
    error::digital_touch::DigitalTouchError,
    message_types::digital_touch::{
        digital_touch_proto::{BaseMessage, FireballMessage},
        models::{DigitalTouchMessage, Point, decode_points, decode_u16s, pluralize},
        svg::Canvas,
    },
};

/// A fireball dragged across the canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct DigitalTouchFireball {
    /// Unique identifier for the message.
    pub id: String,
    /// Start offset along x, centered (roughly `-1.0..=1.0`).
    pub start_x: f32,
    /// Start offset along y, centered (roughly `-1.0..=1.0`).
    pub start_y: f32,
    /// Total duration of the animation, in seconds.
    pub duration: f32,
    /// The dragged path; each point's `extra` is its delay in milliseconds.
    pub points: Vec<Point<u16>>,
}

impl DigitalTouchFireball {
    /// Parse the [`FireballMessage`] carried by `base` into a [`DigitalTouchMessage`].
    pub(super) fn from_payload(
        base: &BaseMessage,
    ) -> Result<DigitalTouchMessage, DigitalTouchError> {
        let msg = FireballMessage::parse_from_bytes(&base.TouchPayload)
            .map_err(DigitalTouchError::ProtobufError)?;

        let delays = decode_u16s(&msg.Delays);

        Ok(DigitalTouchMessage::Fireball(DigitalTouchFireball {
            id: base.ID.clone(),
            start_x: msg.StartX,
            start_y: msg.StartY,
            duration: msg.Duration,
            points: decode_points(&msg.Points, delays)?,
        }))
    }

    /// One-line summary, e.g. `"Digital Touch Fireball (3 points, 2.08s)"`.
    pub(super) fn summary(&self) -> String {
        format!(
            "Digital Touch Fireball ({}, {:.2}s)",
            pluralize(self.points.len(), "point"),
            self.duration,
        )
    }

    /// Draw the dragged trail and a glowing ball at its end.
    pub(super) fn append_svg(&self, canvas: &mut Canvas) {
        canvas.push_def(
            r#"<radialGradient id="dt-fireball"><stop offset="0%" stop-color="white" /><stop offset="30%" stop-color="gold" /><stop offset="60%" stop-color="orange" /><stop offset="100%" stop-color="red" /></radialGradient>"#,
        );

        let Some(last) = self.points.last() else {
            return;
        };

        if self.points.len() > 1 {
            let trail = self
                .points
                .iter()
                .map(|p| format!("{},{}", canvas.fit_x(p.x), canvas.fit_y(p.y)))
                .collect::<Vec<_>>()
                .join(" ");
            let stroke_width = canvas.width() / 40;
            canvas.push(&format!(
                r#"<polyline points="{trail}" fill="none" stroke="orange" stroke-width="{stroke_width}" stroke-linecap="round" stroke-linejoin="round" opacity="0.7" />"#
            ));
        }

        let cx = canvas.fit_x(last.x);
        let cy = canvas.fit_y(last.y);
        let r = canvas.width() / 10;
        canvas.push(&format!(
            r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="url(#dt-fireball)" />"#
        ));
    }
}
