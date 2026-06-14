/*!
 Translation metadata stored in `message_summary_info`.
*/
use plist::Value;
use std::io::Cursor;

use crate::{
    error::plist::PlistParseError,
    util::plist::{
        extract_array_key, extract_bytes_key, extract_dict_idx, extract_dictionary,
        extract_string_key, parse_ns_keyed_archiver, plist_as_dictionary,
    },
};

/// Parsed translation metadata for a message.
#[derive(Debug, PartialEq, Eq)]
pub struct Translation {
    /// Translated text.
    pub translated_text: String,
    /// Target language identifier.
    pub translation_lang: String,
    /// Source language identifier.
    pub source_lang: String,
}

impl Translation {
    /// Parse translation metadata from a `message_summary_info` payload.
    pub fn from_payload(plist: &Value) -> Result<Self, PlistParseError> {
        let translation_data = extract_bytes_key(plist_as_dictionary(plist)?, "tmp")?;

        let inner_plist = parse_ns_keyed_archiver(
            &Value::from_reader(Cursor::new(translation_data))
                .map_err(|_| PlistParseError::NoPayload)?,
        )?;

        // The summary wraps the translation data in a nested archive.
        // Yep, this is really how it is designed
        let translation_data = extract_dict_idx(
            extract_array_key(
                extract_dictionary(plist_as_dictionary(&inner_plist)?, "0")?,
                "0",
            )?,
            0,
        )?;

        Ok(Self {
            // A translation carries no attachment data, so any object-replacement
            // placeholders (U+FFFC) mirroring the original's inline stickers are
            // orphaned noise. Strip them so the text renders cleanly.
            translated_text: extract_string_key(
                extract_dictionary(translation_data, "translatedText")?,
                "NSString",
            )?
            .replace('\u{FFFC}', ""),
            translation_lang: extract_string_key(translation_data, "translationLanguage")?
                .to_string(),
            source_lang: extract_string_key(translation_data, "sourceLanguage")?.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{env::current_dir, fs::File};

    use plist::Value;

    use crate::message_types::translation::Translation;

    #[test]
    fn test_parse_translation_with_inline_stickers() {
        // The original message carried two inline stickers (a Memoji + a genmoji)
        // and a trailing emoji ("This is a test ￼￼🫪"). The translation has no
        // attachment data, so the two sticker placeholders are stripped and only
        // the text + regular emoji remain.
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/app_message/TranslatedWithInlineStickersAndEmojiSummaryInfo.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();

        let translation = Translation::from_payload(&plist).unwrap();

        assert_eq!(translation.translated_text, "Ceci est un test 🫪");
        assert_eq!(translation.source_lang, "en_US");
        assert_eq!(translation.translation_lang, "fr_FR");
    }

    #[test]
    fn test_parse_translation() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/app_message/Translation.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();

        let translation = Translation::from_payload(&plist).unwrap();

        println!("{:#?}", translation);
        assert_eq!(
            translation.translated_text,
            "Oh, il a traduit ce que j'ai envoyé !"
        );
        assert_eq!(translation.source_lang, "en_US");
        assert_eq!(translation.translation_lang, "fr_FR");
    }

    #[test]
    fn test_parse_translation_received() {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/app_message/TranslationReceived.plist");
        let plist_data = File::open(plist_path).unwrap();
        let plist = Value::from_reader(plist_data).unwrap();

        let translation = Translation::from_payload(&plist).unwrap();

        println!("{:#?}", translation);
        assert_eq!(translation.translated_text, "I want chicken");
        assert_eq!(translation.source_lang, "fr_FR");
        assert_eq!(translation.translation_lang, "en_US");
    }
}
