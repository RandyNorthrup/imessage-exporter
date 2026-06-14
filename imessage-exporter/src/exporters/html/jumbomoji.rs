/*!
 Classifies messages for iMessage's "jumbomoji" rendering: when a message's
 visible content is purely glyphs (emoji and/or inline stickers), iMessage
 scales the bubble's font-size based on the glyph count: 1 glyph renders
 very large, 2–3 medium, 4+ at standard text size.
*/

use imessage_database::tables::messages::models::BubbleComponent;

use crate::exporters::html::view_model::GlyphSize;

/// Range check against the standard emoji Unicode blocks. Coarse on purpose:
/// the classifier bails on any non-emoji codepoint, so this only matters for
/// messages that are *entirely* glyphs.
fn is_emoji_codepoint(c: char) -> bool {
    matches!(
        c as u32,
        0x1F600..=0x1F64F  // Emoticons
        | 0x1F300..=0x1F5FF  // Misc Symbols & Pictographs
        | 0x1F680..=0x1F6FF  // Transport & Map
        | 0x1F900..=0x1F9FF  // Supplemental Symbols & Pictographs
        | 0x1FA70..=0x1FAFF  // Symbols & Pictographs Extended-A
        | 0x2300..=0x23FF    // Misc Technical (⌚, ⌛, ⏰)
        | 0x25A0..=0x25FF    // Geometric Shapes (▶, ●)
        | 0x2600..=0x26FF    // Misc Symbols
        | 0x2700..=0x27BF    // Dingbats
        | 0x2B00..=0x2BFF    // Misc Symbols & Arrows (⭐, ⬆)
    )
}

/// Bases that *can* form a keycap emoji (0-9, #, *). They are emoji-eligible
/// only when followed by the U+FE0F U+20E3 suffix, so we treat them via
/// lookahead in [`count_emoji_glyphs`] rather than including them in
/// [`is_emoji_codepoint`] (otherwise plain text like `"123"` would falsely
/// classify as pure-emoji).
fn is_keycap_base(c: char) -> bool {
    matches!(c, '0'..='9' | '#' | '*')
}

/// Modifier code points that always attach to the preceding base glyph.
fn is_attaching_modifier(c: char) -> bool {
    matches!(
        c as u32,
        0xFE0F              // VS-16
        | 0x1F3FB..=0x1F3FF // Skin tones
    )
}

/// ZWJ code point, which binds the preceding and following bases into a single glyph.
fn is_zwj(c: char) -> bool {
    c as u32 == 0x200D
}

/// Regional indicator code points, which pair up into flag glyphs.
fn is_regional_indicator(c: char) -> bool {
    matches!(c as u32, 0x1F1E6..=0x1F1FF)
}

/// Walks `text`, counting emoji glyphs. VS-16 and skin-tone modifiers attach
/// to the preceding base; ZWJ binds the next base to the previous one (so
/// `🤦‍♂️` is one glyph, not three); a pair of regional indicators collapses
/// into one flag; a keycap base followed by `U+FE0F U+20E3` (e.g. `1️⃣`)
/// counts as one glyph. Returns `None` the moment a non-emoji, non-whitespace
/// character appears, so the typical text-bearing message bails after one
/// character.
#[must_use]
pub(crate) fn count_emoji_glyphs(text: &str) -> Option<usize> {
    const KEYCAP_COMBINING: char = '\u{20E3}';
    const VS16: char = '\u{FE0F}';

    let mut count = 0usize;
    let mut prev_was_regional = false;
    let mut prev_was_zwj = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if is_zwj(c) {
            prev_was_zwj = true;
            prev_was_regional = false;
            continue;
        }
        if is_attaching_modifier(c) {
            // Attach to whatever came before; do not change the ZWJ-binding
            // state, since the bound base may itself carry a VS-16 / skin tone.
            prev_was_regional = false;
            continue;
        }
        if c.is_whitespace() {
            prev_was_regional = false;
            prev_was_zwj = false;
            continue;
        }
        if is_keycap_base(c) {
            // Only counts as emoji when followed by U+FE0F U+20E3. Anything
            // else means it's plain text, so we should bail.
            if chars.peek() == Some(&VS16) {
                chars.next();
                if chars.peek() == Some(&KEYCAP_COMBINING) {
                    chars.next();
                    if !prev_was_zwj {
                        count = count.saturating_add(1);
                    }
                    prev_was_zwj = false;
                    prev_was_regional = false;
                    continue;
                }
            }
            return None;
        }
        if is_regional_indicator(c) {
            if prev_was_regional {
                prev_was_regional = false;
            } else {
                count = count.saturating_add(1);
                prev_was_regional = true;
            }
            prev_was_zwj = false;
            continue;
        }
        if is_emoji_codepoint(c) {
            if !prev_was_zwj {
                count = count.saturating_add(1);
            }
            prev_was_zwj = false;
            prev_was_regional = false;
            continue;
        }
        return None;
    }
    Some(count)
}

/// Classifies an entire message into a [`GlyphSize`]. Returns
/// [`GlyphSize::Normal`] unless every component is pure-glyph (emoji-only
/// text + inline stickers), in which case it buckets on the total glyph count.
#[must_use]
pub(crate) fn classify_message(
    components: &[BubbleComponent],
    full_text: Option<&str>,
) -> GlyphSize {
    let mut count = 0usize;
    for component in components {
        match component {
            BubbleComponent::Run(ranges) => {
                for range in ranges {
                    if range.attachment.is_some() {
                        // Attachment range: only an inline sticker
                        // counts as one glyph. Any other attachment
                        // means the message isn't pure-glyph.
                        if !range.emoji_image {
                            return GlyphSize::Normal;
                        }
                        count = count.saturating_add(1);
                    } else {
                        // Text range: every codepoint must be emoji/whitespace.
                        let Some(text) = full_text else {
                            return GlyphSize::Normal;
                        };
                        let end = range.end.min(text.len());
                        let start = range.start.min(end);
                        let Some(slice_count) = count_emoji_glyphs(&text[start..end]) else {
                            return GlyphSize::Normal;
                        };
                        count = count.saturating_add(slice_count);
                    }
                }
            }
            BubbleComponent::App | BubbleComponent::Retracted => {
                return GlyphSize::Normal;
            }
        }
    }
    match count {
        1 => GlyphSize::Jumbo,
        2 | 3 => GlyphSize::Medium,
        _ => GlyphSize::Normal,
    }
}

#[cfg(test)]
mod tests {
    use imessage_database::{
        message_types::text_effects::text_effect::TextEffect,
        tables::messages::models::{AttachmentMeta, AttributedRange, BubbleComponent},
    };

    use super::*;

    #[test]
    fn single_emoji_counts_as_one() {
        assert_eq!(count_emoji_glyphs("🎉"), Some(1));
    }

    #[test]
    fn multiple_emoji_count_correctly() {
        assert_eq!(count_emoji_glyphs("🎉🎊🎁"), Some(3));
    }

    #[test]
    fn zwj_sequence_counts_as_one() {
        // 🤦‍♂️ = U+1F926 + ZWJ + U+2642 + VS-16
        assert_eq!(count_emoji_glyphs("🤦\u{200D}♂\u{FE0F}"), Some(1));
    }

    #[test]
    fn skin_tone_counts_as_one() {
        assert_eq!(count_emoji_glyphs("👋🏽"), Some(1));
    }

    #[test]
    fn misc_technical_block_counts() {
        // ⌚ (U+231A) lives in Misc Technical.
        assert_eq!(count_emoji_glyphs("⌚"), Some(1));
    }

    #[test]
    fn geometric_shapes_block_counts() {
        // ▶ (U+25B6) lives in Geometric Shapes.
        assert_eq!(count_emoji_glyphs("▶"), Some(1));
    }

    #[test]
    fn misc_symbols_arrows_block_counts() {
        // ⭐ (U+2B50) lives in Misc Symbols & Arrows.
        assert_eq!(count_emoji_glyphs("⭐"), Some(1));
    }

    #[test]
    fn keycap_sequence_counts_as_one() {
        // 1️⃣ = '1' + VS-16 + U+20E3
        assert_eq!(count_emoji_glyphs("1\u{FE0F}\u{20E3}"), Some(1));
    }

    #[test]
    fn keycap_hash_and_star_count_as_one() {
        assert_eq!(count_emoji_glyphs("#\u{FE0F}\u{20E3}"), Some(1));
        assert_eq!(count_emoji_glyphs("*\u{FE0F}\u{20E3}"), Some(1));
    }

    #[test]
    fn bare_digits_are_not_emoji() {
        // "123" must NOT classify as pure-emoji, keycap base requires the
        // FE0F / 20E3 suffix.
        assert_eq!(count_emoji_glyphs("123"), None);
    }

    #[test]
    fn flag_counts_as_one() {
        assert_eq!(count_emoji_glyphs("🇺🇸"), Some(1));
    }

    #[test]
    fn two_flags_count_as_two() {
        assert_eq!(count_emoji_glyphs("🇺🇸🇯🇵"), Some(2));
    }

    #[test]
    fn non_emoji_text_returns_none() {
        assert_eq!(count_emoji_glyphs("hi 🎉"), None);
    }

    #[test]
    fn whitespace_does_not_increment() {
        assert_eq!(count_emoji_glyphs("🎉 🎊"), Some(2));
    }

    #[test]
    fn empty_string_is_zero() {
        assert_eq!(count_emoji_glyphs(""), Some(0));
    }

    #[test]
    fn classify_empty_message_is_normal() {
        assert_eq!(classify_message(&[], None), GlyphSize::Normal);
    }

    #[test]
    fn classify_single_emoji_is_jumbo() {
        let text = "🎉".to_string();
        let components = vec![BubbleComponent::Run(vec![AttributedRange::text(
            0,
            text.len(),
            vec![TextEffect::Default],
        )])];
        assert_eq!(classify_message(&components, Some(&text)), GlyphSize::Jumbo);
    }

    #[test]
    fn classify_three_emoji_is_medium() {
        let text = "🎉🎊🎁".to_string();
        let components = vec![BubbleComponent::Run(vec![AttributedRange::text(
            0,
            text.len(),
            vec![TextEffect::Default],
        )])];
        assert_eq!(
            classify_message(&components, Some(&text)),
            GlyphSize::Medium
        );
    }

    #[test]
    fn classify_four_emoji_is_normal() {
        let text = "🎉🎊🎁🎀".to_string();
        let components = vec![BubbleComponent::Run(vec![AttributedRange::text(
            0,
            text.len(),
            vec![TextEffect::Default],
        )])];
        assert_eq!(
            classify_message(&components, Some(&text)),
            GlyphSize::Normal
        );
    }

    #[test]
    fn classify_emoji_with_text_is_normal() {
        let text = "Hello 👋".to_string();
        let components = vec![BubbleComponent::Run(vec![AttributedRange::text(
            0,
            text.len(),
            vec![TextEffect::Default],
        )])];
        assert_eq!(
            classify_message(&components, Some(&text)),
            GlyphSize::Normal
        );
    }

    #[test]
    fn classify_single_inline_sticker_is_jumbo() {
        // An inline sticker (the `emoji_image` hint) counts as one glyph.
        let components = vec![BubbleComponent::Run(vec![
            AttributedRange::inline_attachment(0, 3, AttachmentMeta::default()),
        ])];
        assert_eq!(classify_message(&components, None), GlyphSize::Jumbo);
    }

    #[test]
    fn classify_block_attachment_is_normal() {
        // A block attachment means the message isn't pure-glyph.
        let components = vec![BubbleComponent::Run(vec![AttributedRange::attachment(
            0,
            3,
            AttachmentMeta::default(),
        )])];
        assert_eq!(classify_message(&components, None), GlyphSize::Normal);
    }

    #[test]
    fn classify_mixed_emoji_and_inline_sticker_is_medium() {
        // Emoji text plus an inline sticker in one bubble (one Run): two glyphs.
        let text = "🎉".to_string();
        let components = vec![BubbleComponent::Run(vec![
            AttributedRange::text(0, text.len(), vec![TextEffect::Default]),
            AttributedRange::inline_attachment(
                text.len(),
                text.len() + 3,
                AttachmentMeta::default(),
            ),
        ])];
        assert_eq!(
            classify_message(&components, Some(&text)),
            GlyphSize::Medium
        );
    }
}
