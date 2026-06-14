/*!
[Sketch](super) Digital Touch effect: a freehand drawing of colored strokes.
*/

use protobuf::Message;

use crate::{
    error::digital_touch::DigitalTouchError,
    message_types::digital_touch::{
        digital_touch_proto::{BaseMessage, SketchMessage},
        models::{Color, DigitalTouchMessage, Point, decode_strokes, pluralize},
        svg::Canvas,
    },
};

/// Width of a sketched stroke, in canvas user units.
const STROKE_WIDTH: usize = 8;

/// A freehand sketch made of one or more colored strokes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigitalTouchSketch {
    /// Unique identifier for the message.
    pub id: String,
    /// Strokes, each a polyline of points sharing a single color.
    pub strokes: Vec<Vec<Point<Color>>>,
}

impl DigitalTouchSketch {
    /// Parse the [`SketchMessage`] carried by `base` into a [`DigitalTouchMessage`].
    pub(super) fn from_payload(
        base: &BaseMessage,
    ) -> Result<DigitalTouchMessage, DigitalTouchError> {
        let msg = SketchMessage::parse_from_bytes(&base.TouchPayload)
            .map_err(DigitalTouchError::ProtobufError)?;

        let colors = Color::decode_all(&msg.Colors);
        let count = usize::try_from(msg.StrokesCount).unwrap_or(0);
        let strokes = decode_strokes(&msg.Strokes, count)?
            .into_iter()
            .enumerate()
            .map(|(stroke, points)| {
                let color = colors.get(stroke).copied().unwrap_or(Color::WHITE);
                points
                    .into_iter()
                    .map(|(x, y)| Point { x, y, extra: color })
                    .collect()
            })
            .collect();

        Ok(DigitalTouchMessage::Sketch(DigitalTouchSketch {
            id: base.ID.clone(),
            strokes,
        }))
    }

    /// One-line summary, e.g. `"Digital Touch Sketch (1 stroke, 81 points)"`.
    pub(super) fn summary(&self) -> String {
        let points: usize = self.strokes.iter().map(Vec::len).sum();
        format!(
            "Digital Touch Sketch ({}, {})",
            pluralize(self.strokes.len(), "stroke"),
            pluralize(points, "point"),
        )
    }

    /// Draw each stroke as a colored polyline, or a dot for a single-point stroke.
    pub(super) fn append_svg(&self, canvas: &mut Canvas) {
        for stroke in &self.strokes {
            let Some(first) = stroke.first() else {
                continue;
            };
            let color = first.extra.css();

            // A single-point stroke has no segment to stroke, and SVG does not
            // render a one-vertex polyline. Draw it as a filled dot the size of
            // the round stroke cap instead.
            if stroke.len() == 1 {
                canvas.push(&format!(
                    r#"<circle cx="{}" cy="{}" r="{}" fill="{color}" />"#,
                    canvas.fit_x(first.x),
                    canvas.fit_y(first.y),
                    STROKE_WIDTH / 2,
                ));
                continue;
            }

            let points = stroke
                .iter()
                .map(|p| format!("{},{}", canvas.fit_x(p.x), canvas.fit_y(p.y)))
                .collect::<Vec<_>>()
                .join(" ");
            canvas.push(&format!(
                r#"<polyline points="{points}" fill="none" stroke="{color}" stroke-width="{STROKE_WIDTH}" stroke-linecap="round" stroke-linejoin="round" />"#
            ));
        }
    }
}
