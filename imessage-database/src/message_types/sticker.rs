/*!
These are [sticker messages](https://support.apple.com/guide/iphone/send-stickers-iph37b0bfe7b/ios), either from the user's sticker library or sticker apps.
*/

use std::fmt::{Display, Formatter};

use crate::util::bundle_id::parse_balloon_bundle_id;

/// Bytes for `stickerEffect:type="`
const STICKER_EFFECT_PREFIX: [u8; 20] = [
    115, 116, 105, 99, 107, 101, 114, 69, 102, 102, 101, 99, 116, 58, 116, 121, 112, 101, 61, 34,
];
/// Bytes for `"/>`
const STICKER_EFFECT_SUFFIX: [u8; 3] = [34, 47, 62];

/// Source that created a sticker attachment.
#[derive(Debug, PartialEq, Eq)]
pub enum StickerSource {
    /// A [Genmoji](https://support.apple.com/guide/iphone/create-genmoji-with-apple-intelligence-iph4e76f5667/ios)
    Genmoji,
    /// A [Memoji](https://support.apple.com/en-us/111115)
    Memoji,
    /// User-created sticker.
    UserGenerated,
    /// Sticker from an iMessage sticker app.
    App(String),
}

impl StickerSource {
    /// Resolve a sticker source from the sticker app bundle ID.
    ///
    /// # Example
    ///
    /// ```rust
    /// use imessage_database::message_types::sticker::StickerSource;
    ///
    /// println!("{:?}", StickerSource::from_bundle_id("com.apple.messages.genmoji")); // Some(Genmoji)
    /// ```
    #[must_use]
    pub fn from_bundle_id(bundle_id: &str) -> Option<Self> {
        match parse_balloon_bundle_id(Some(bundle_id)) {
            Some("com.apple.messages.genmoji") => Some(StickerSource::Genmoji),
            Some(
                "com.apple.Animoji.StickersApp.MessagesExtension" | "com.apple.Jellyfish.Animoji",
            ) => Some(StickerSource::Memoji),
            Some("com.apple.Stickers.UserGenerated.MessagesExtension") => {
                Some(StickerSource::UserGenerated)
            }
            Some(other) => Some(StickerSource::App(other.to_string())),
            None => None,
        }
    }
}

/// Format-agnostic description of a sticker's source, ready for rendering by
/// callers. Produced by [`Attachment::get_sticker_decoration`](crate::tables::attachment::Attachment::get_sticker_decoration).
///
/// `None` from `get_sticker_decoration` means one of:
/// - The sticker has no readable source (missing `STICKER_USER_INFO`,
///   malformed plist, or unrecognized bundle id).
/// - The source is [`StickerSource::Genmoji`] but no description is stored.
/// - The source is [`StickerSource::UserGenerated`] but the effect blob is
///   missing or unreadable.
#[derive(Debug, PartialEq, Eq)]
pub enum StickerDecoration {
    /// A [`StickerSource::Genmoji`] with the user-supplied prompt.
    GenmojiPrompt(String),
    /// A [`StickerSource::Memoji`]; no further data.
    Memoji,
    /// A [`StickerSource::UserGenerated`] sticker with the parsed [`StickerEffect`].
    Effect(StickerEffect),
    /// A [`StickerSource::App`] sticker; the string is the resolved app name
    /// or, if that lookup fails, the bundle id.
    AppName(String),
}

/// Represents different types of [sticker effects](https://www.macrumors.com/how-to/add-effects-to-stickers-in-messages/) that can be applied to sticker iMessage balloons.
#[derive(Debug, PartialEq, Eq, Default)]
pub enum StickerEffect {
    /// Sticker sent with no effect.
    #[default]
    Normal,
    /// `stroke` effect in sticker EXIF metadata.
    Outline,
    /// Comic effect.
    Comic,
    /// Puffy effect.
    Puffy,
    /// `iridescent` effect in sticker EXIF metadata.
    Shiny,
    /// Unrecognized sticker effect name.
    Other(String),
}

impl StickerEffect {
    /// Map the raw EXIF sticker effect name to a [`StickerEffect`].
    fn from_exif(sticker_effect_type: &str) -> Self {
        match sticker_effect_type {
            "stroke" => Self::Outline,
            "comic" => Self::Comic,
            "puffy" => Self::Puffy,
            "iridescent" => Self::Shiny,
            other => Self::Other(other.to_owned()),
        }
    }
}

impl Display for StickerEffect {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            StickerEffect::Normal => write!(fmt, "Normal"),
            StickerEffect::Outline => write!(fmt, "Outline"),
            StickerEffect::Comic => write!(fmt, "Comic"),
            StickerEffect::Puffy => write!(fmt, "Puffy"),
            StickerEffect::Shiny => write!(fmt, "Shiny"),
            StickerEffect::Other(name) => write!(fmt, "{name}"),
        }
    }
}

/// Parse the sticker effect type from HEIC EXIF data.
#[must_use]
pub fn get_sticker_effect(mut heic_data: &[u8]) -> StickerEffect {
    // Find the start index and drain
    for idx in 0..heic_data.len() {
        if idx + STICKER_EFFECT_PREFIX.len() < heic_data.len() {
            let part = &heic_data[idx..idx + STICKER_EFFECT_PREFIX.len()];
            if part == STICKER_EFFECT_PREFIX {
                // Remove the start pattern from the blob
                heic_data = &heic_data[idx + STICKER_EFFECT_PREFIX.len()..];
                break;
            }
        } else {
            return StickerEffect::Normal;
        }
    }

    // Find the end index and truncate
    for idx in 1..heic_data.len() {
        if idx >= heic_data.len() - 2 {
            return StickerEffect::Other("Unknown".to_string());
        }
        let part = &heic_data[idx..idx + STICKER_EFFECT_SUFFIX.len()];

        if part == STICKER_EFFECT_SUFFIX {
            // Remove the end pattern from the string
            heic_data = &heic_data[..idx];
            break;
        }
    }
    StickerEffect::from_exif(&String::from_utf8_lossy(heic_data))
}

#[cfg(test)]
mod tests {
    use std::env::current_dir;
    use std::fs::File;
    use std::io::Read;

    use crate::message_types::sticker::{StickerEffect, get_sticker_effect};

    #[test]
    fn test_parse_sticker_normal() {
        let sticker_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/stickers/no_effect.heic");
        let mut file = File::open(sticker_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let effect = get_sticker_effect(&bytes);

        assert_eq!(effect, StickerEffect::Normal);
    }

    #[test]
    fn test_parse_sticker_outline() {
        let sticker_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/stickers/outline.heic");
        let mut file = File::open(sticker_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let effect = get_sticker_effect(&bytes);

        assert_eq!(effect, StickerEffect::Outline);
    }

    #[test]
    fn test_parse_sticker_comic() {
        let sticker_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/stickers/comic.heic");
        let mut file = File::open(sticker_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let effect = get_sticker_effect(&bytes);

        assert_eq!(effect, StickerEffect::Comic);
    }

    #[test]
    fn test_parse_sticker_puffy() {
        let sticker_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/stickers/puffy.heic");
        let mut file = File::open(sticker_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let effect = get_sticker_effect(&bytes);

        assert_eq!(effect, StickerEffect::Puffy);
    }

    #[test]
    fn test_parse_sticker_shiny() {
        let sticker_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/stickers/shiny.heic");
        let mut file = File::open(sticker_path).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();

        let effect = get_sticker_effect(&bytes);

        assert_eq!(effect, StickerEffect::Shiny);
    }
}
