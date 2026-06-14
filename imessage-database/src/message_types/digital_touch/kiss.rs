/*!
[Kiss](super) Digital Touch effect: one or more placed, rotated kisses.
*/

use std::f64::consts::PI;

use protobuf::Message;

use crate::{
    error::digital_touch::DigitalTouchError,
    message_types::digital_touch::{
        digital_touch_proto::{BaseMessage, KissMessage},
        models::{DigitalTouchMessage, Point, decode_points, decode_u16s},
        svg::Canvas,
    },
};

/// Silhouette of a pair of lips (a kiss mark), centered on the origin and
/// spanning roughly `-50..=50` horizontally. The top edge traces the two peaks
/// and central notch of a cupid's bow; the bottom edge is the fuller lower lip.
const LIPS_PATH: &str = "M -50,2 \
    C -40,-16 -22,-18 -10,-10 C -4,-14 -2,-14 0,-7 \
    C 2,-14 4,-14 10,-10 C 22,-18 40,-16 50,2 \
    C 44,11 30,17 18,22 C 8,27 -8,27 -18,22 C -30,17 -44,11 -50,2 Z";

/// Per-kiss data: its delay and its rotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KissData {
    /// Delay before this kiss, in milliseconds.
    pub delay_ms: u16,
    /// Rotation of the kiss, in milliradians (thousandths of a radian).
    pub rotation_milliradians: u16,
}

impl KissData {
    /// Rotation in degrees, suitable for an SVG `rotate(…)` transform. Negated
    /// because screen-space y grows downward.
    #[must_use]
    pub fn degrees(&self) -> f64 {
        -(f64::from(self.rotation_milliradians) / 1000.0 * 180.0 / PI)
    }
}

/// A series of kisses, each placed and rotated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigitalTouchKiss {
    /// Unique identifier for the message.
    pub id: String,
    /// The kisses, in order.
    pub kisses: Vec<Point<KissData>>,
}

impl DigitalTouchKiss {
    /// Parse the [`KissMessage`] carried by `base` into a [`DigitalTouchMessage`].
    pub(super) fn from_payload(
        base: &BaseMessage,
    ) -> Result<DigitalTouchMessage, DigitalTouchError> {
        let msg = KissMessage::parse_from_bytes(&base.TouchPayload)
            .map_err(DigitalTouchError::ProtobufError)?;

        let delays = decode_u16s(&msg.Delays);
        let rotations = decode_u16s(&msg.Rotations);
        if delays.len() != rotations.len() {
            return Err(DigitalTouchError::ArraysDoNotMatch(
                "delays",
                delays.len(),
                "rotations",
                rotations.len(),
            ));
        }

        let extras = delays
            .into_iter()
            .zip(rotations)
            .map(|(delay_ms, rotation_milliradians)| KissData {
                delay_ms,
                rotation_milliradians,
            })
            .collect();

        Ok(DigitalTouchMessage::Kiss(DigitalTouchKiss {
            id: base.ID.clone(),
            kisses: decode_points(&msg.Points, extras)?,
        }))
    }

    /// One-line summary, e.g. `"Digital Touch Kiss (1 kiss)"`.
    pub(super) fn summary(&self) -> String {
        let count = self.kisses.len();
        let noun = if count == 1 { "kiss" } else { "kisses" };
        format!("Digital Touch Kiss ({count} {noun})")
    }

    /// Draw each kiss as a rotated pair of lips.
    pub(super) fn append_svg(&self, canvas: &mut Canvas) {
        // Scale the ~100-unit-wide lips to a recognizable size on the canvas.
        let scale = canvas.width() as f64 / 293.0;
        for kiss in &self.kisses {
            let x = canvas.fit_x(kiss.x);
            let y = canvas.fit_y(kiss.y);
            let deg = kiss.extra.degrees();
            canvas.push(&format!(
                r#"<path d="{LIPS_PATH}" transform="translate({x}, {y}) rotate({deg:.1}) scale({scale:.3})" fill="red" stroke="red" stroke-width="2" stroke-linejoin="round" />"#
            ));
        }
    }
}
