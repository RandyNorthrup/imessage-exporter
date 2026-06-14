/*!
[Media](super) Digital Touch effect: a photo or video.

A still photo can be drawn on. The [`Image`](MediaKind::Image) overlay holds the
effects layered on top. A video cannot: the Digital Touch UI disables drawing
while capturing or sending video, so a video cannot carry an overlay.

The effect carries a media-type discriminator and an `NSKeyedArchiver` archive.
For a photo, the archive holds an `NSMutableArray` of overlay effects, each an
`NSData` blob containing a complete, nested Digital Touch message parsed through
the same dispatcher as a top-level message; a nested media blob is skipped so a
crafted archive cannot drive unbounded recursion. The photo or video itself is
delivered as a normal message attachment, not embedded here.
*/

use std::io::Cursor;

use plist::Value;
use protobuf::Message;

use crate::{
    error::digital_touch::DigitalTouchError,
    message_types::digital_touch::{
        digital_touch_proto::{BaseMessage, MediaMessage, TouchKind},
        models::{DigitalTouchMessage, pluralize},
        svg::Canvas,
    },
};

/// The media a [`DigitalTouchMedia`] effect carries.
///
/// Only a still image can be drawn on: the Digital Touch UI disables the drawing
/// tools while capturing or sending video, so a video never carries an overlay.
#[derive(Debug, Clone, PartialEq)]
pub enum MediaKind {
    /// A still image, with the effects drawn on top
    /// of it.
    Image {
        /// Effects layered over the image; empty when nothing was drawn.
        overlay: Vec<DigitalTouchMessage>,
    },
    /// A video
    Video,
    /// An unrecognized media-type discriminator.
    Other(u64),
}

impl MediaKind {
    /// Human-readable label.
    fn label(&self) -> String {
        match self {
            MediaKind::Image { .. } => "Image".to_string(),
            MediaKind::Video => "Video".to_string(),
            MediaKind::Other(media_type) => format!("Media (type {media_type})"),
        }
    }
}

/// A photo or video Digital Touch effect.
#[derive(Debug, Clone, PartialEq)]
pub struct DigitalTouchMedia {
    /// Unique identifier for the message.
    pub id: String,
    /// The media carried, and (for an image) the effects drawn on top of it.
    pub kind: MediaKind,
}

impl DigitalTouchMedia {
    /// Parse the [`MediaMessage`] carried by `base` into a [`DigitalTouchMessage`].
    pub(super) fn from_payload(
        base: &BaseMessage,
    ) -> Result<DigitalTouchMessage, DigitalTouchError> {
        let msg = MediaMessage::parse_from_bytes(&base.TouchPayload)
            .map_err(DigitalTouchError::ProtobufError)?;

        // Only a still image can be drawn on, so only an image decodes an overlay
        // from the archive; a video (or unknown kind) is shown on its own.
        let kind = match msg.MediaType {
            1 => MediaKind::Video,
            2 => MediaKind::Image {
                overlay: decode_overlays(&msg.Archive)?,
            },
            other => MediaKind::Other(other),
        };

        Ok(DigitalTouchMessage::Media(DigitalTouchMedia {
            id: base.ID.clone(),
            kind,
        }))
    }

    /// One-line summary, e.g. `"Digital Touch Image"` or
    /// `"Digital Touch Image with drawing (5 strokes)"`.
    pub(super) fn summary(&self) -> String {
        let MediaKind::Image { overlay } = &self.kind else {
            return format!("Digital Touch {}", self.kind.label());
        };

        let mut summary = "Digital Touch Image".to_string();
        if !overlay.is_empty() {
            // Sketch overlays expose a precise stroke count; any other overlay
            // type falls back to a generic label.
            let strokes: usize = overlay
                .iter()
                .map(|effect| match effect {
                    DigitalTouchMessage::Sketch(sketch) => sketch.strokes.len(),
                    _ => 0,
                })
                .sum();
            if strokes > 0 {
                summary.push_str(&format!(" with drawing ({})", pluralize(strokes, "stroke")));
            } else {
                summary.push_str(" with overlay");
            }
        }
        summary
    }

    /// Draw the image overlay effects (if any). When the backing photo is supplied
    /// as the canvas background it shows through beneath them.
    pub(super) fn append_svg(&self, canvas: &mut Canvas) {
        if let MediaKind::Image { overlay } = &self.kind {
            for effect in overlay {
                effect.append_svg(canvas);
            }
        }

        if !canvas.has_background() {
            let width = canvas.width();
            let font = width / 16;
            let y = canvas.height() * 9 / 10;
            canvas.push(&format!(
                r#"<text x="{}" y="{y}" fill="white" font-family="-apple-system, Helvetica, Arial, sans-serif" font-size="{font}" text-anchor="middle">{}</text>"#,
                width / 2,
                self.kind.label(),
            ));
        }
    }
}

/// Decode the overlay effects from the effect's `NSKeyedArchiver` archive.
///
/// The archive is an `NSMutableArray` of `NSData` blobs, each a complete, nested
/// Digital Touch message. Any non-media [`TouchKind`] may appear. A blob that
/// fails to parse is skipped rather than failing the whole message, since the
/// media kind is still meaningful without its overlay. A nested
/// [`Media`](DigitalTouchMessage::Media) blob is skipped so a crafted archive
/// cannot drive unbounded recursion.
fn decode_overlays(archive: &[u8]) -> Result<Vec<DigitalTouchMessage>, DigitalTouchError> {
    if archive.is_empty() {
        return Ok(Vec::new());
    }

    let value =
        Value::from_reader(Cursor::new(archive)).map_err(DigitalTouchError::ArchiveError)?;

    let Some(objects) = value
        .as_dictionary()
        .and_then(|dict| dict.get("$objects"))
        .and_then(Value::as_array)
    else {
        return Ok(Vec::new());
    };

    let mut overlays = Vec::new();
    for object in objects {
        let Some(data) = object
            .as_dictionary()
            .and_then(|dict| dict.get("NS.data"))
            .and_then(Value::as_data)
        else {
            continue;
        };

        let Ok(base) = BaseMessage::parse_from_bytes(data) else {
            continue;
        };
        // Refuse a nested media blob before dispatching, so a crafted archive
        // can't nest media-in-media and drive unbounded recursion.
        if base.TouchKind.enum_value_or_default() == TouchKind::Media {
            continue;
        }
        if let Ok(message) = DigitalTouchMessage::from_base(&base) {
            overlays.push(message);
        }
    }

    Ok(overlays)
}
