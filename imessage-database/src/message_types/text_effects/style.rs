/// Traditional formatting style applied to a message text range.
///
/// Read more about text styles [here](https://www.apple.com/newsroom/2024/06/ios-18-makes-iphone-more-personal-capable-and-intelligent-than-ever/).
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Style {
    /// **Bold** text
    Bold,
    /// *Italic* text
    Italic,
    /// ~~Strikethrough~~ text
    Strikethrough,
    /// <u>Underline</u> text
    Underline,
}
