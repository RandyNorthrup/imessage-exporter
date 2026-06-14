/*!
[Heartbeat](super) Digital Touch effect: a pulse at a given rate, which may
break partway through (a "heartbreak").
*/

use protobuf::Message;

use crate::{
    error::digital_touch::DigitalTouchError,
    message_types::digital_touch::{
        digital_touch_proto::{BaseMessage, HeartbeatMessage},
        models::DigitalTouchMessage,
        svg::Canvas,
    },
};

/// A whole heart, centered on the origin and pointing down, spanning roughly
/// `-40..=40` in each axis.
const HEART: &str = "M 0,-22 C -10,-40 -40,-40 -40,-12 C -40,10 -15,22 0,38 C 15,22 40,10 40,-12 C 40,-40 10,-40 0,-22 Z";
/// The left half of [`HEART`], split down the middle.
const HEART_LEFT: &str = "M 0,-22 C -10,-40 -40,-40 -40,-12 C -40,10 -15,22 0,38 Z";
/// The right half of [`HEART`], split down the middle.
const HEART_RIGHT: &str = "M 0,-22 C 10,-40 40,-40 40,-12 C 40,10 15,22 0,38 Z";

/// A heartbeat effect.
#[derive(Debug, Clone, PartialEq)]
pub struct DigitalTouchHeartbeat {
    /// Unique identifier for the message.
    pub id: String,
    /// Heart rate, in beats per minute.
    pub bpm: f32,
    /// Total duration of the animation, in seconds.
    pub duration: u64,
    /// When the heart breaks, in seconds from the start; `None` if it never does.
    pub broken_at: Option<f32>,
}

impl DigitalTouchHeartbeat {
    /// Parse the [`HeartbeatMessage`] carried by `base` into a [`DigitalTouchMessage`].
    pub(super) fn from_payload(
        base: &BaseMessage,
    ) -> Result<DigitalTouchMessage, DigitalTouchError> {
        let msg = HeartbeatMessage::parse_from_bytes(&base.TouchPayload)
            .map_err(DigitalTouchError::ProtobufError)?;

        Ok(DigitalTouchMessage::Heartbeat(DigitalTouchHeartbeat {
            id: base.ID.clone(),
            bpm: msg.BPM,
            duration: msg.Duration,
            broken_at: (msg.HeartBrokenAt > 0.0).then_some(msg.HeartBrokenAt),
        }))
    }

    /// One-line summary, e.g. `"Digital Touch Heartbeat (84 BPM, 2s)"` or
    /// `"Digital Touch Heartbreak (84 BPM, broke at 1.71s)"`.
    pub(super) fn summary(&self) -> String {
        match self.broken_at {
            Some(at) => format!(
                "Digital Touch Heartbreak ({:.0} BPM, broke at {at:.2}s)",
                self.bpm,
            ),
            None => format!(
                "Digital Touch Heartbeat ({:.0} BPM, {}s)",
                self.bpm, self.duration,
            ),
        }
    }

    /// Draw a heart (whole, or split when broken) with a rate label.
    pub(super) fn append_svg(&self, canvas: &mut Canvas) {
        let width = canvas.width();
        let height = canvas.height();
        let cx = width / 2;
        let cy = height * 2 / 5;
        let scale = width as f64 / 200.0;
        let font = width / 16;
        let label_y = height * 4 / 5;

        match self.broken_at {
            Some(at) => {
                canvas.push(&format!(
                    r#"<path d="{HEART_LEFT}" transform="translate({cx},{cy}) scale({scale:.3}) translate(-4,0) rotate(-12)" fill="red" />"#
                ));
                canvas.push(&format!(
                    r#"<path d="{HEART_RIGHT}" transform="translate({cx},{cy}) scale({scale:.3}) translate(4,0) rotate(12)" fill="red" />"#
                ));
                canvas.push(&label(
                    cx,
                    label_y,
                    font,
                    &format!("{:.0} BPM · broke at {at:.2}s", self.bpm),
                ));
            }
            None => {
                canvas.push(&format!(
                    r#"<path d="{HEART}" transform="translate({cx},{cy}) scale({scale:.3})" fill="red" />"#
                ));
                canvas.push(&label(
                    cx,
                    label_y,
                    font,
                    &format!("{:.0} BPM · {}s", self.bpm, self.duration),
                ));
            }
        }
    }
}

/// Build a centered white text label.
fn label(cx: usize, y: usize, font: usize, text: &str) -> String {
    format!(
        r#"<text x="{cx}" y="{y}" fill="white" font-family="-apple-system, Helvetica, Arial, sans-serif" font-size="{font}" text-anchor="middle">{text}</text>"#
    )
}
