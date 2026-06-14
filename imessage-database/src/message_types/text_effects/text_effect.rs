use crate::message_types::text_effects::{
    animation::Animation,
    detected::{
        address::DetectedAddress, currency::DetectedCurrency, flight::Flight,
        shipment_tracking::ShipmentTracking, unit::Unit,
    },
    style::Style,
};

/// Formatting or detected-data marker attached to a message text range.
///
/// Each [`AttributedRange`](crate::tables::messages::models::AttributedRange)
/// carries zero or more of these values. Sender-applied formatting, links, and
/// automatically detected entities all flow through this enum.
///
/// Read more about text styles [here](https://www.apple.com/newsroom/2024/06/ios-18-makes-iphone-more-personal-capable-and-intelligent-than-ever/).
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TextEffect {
    /// Explicit marker for unstyled text.
    Default,
    /// A [mentioned](https://support.apple.com/guide/messages/mention-a-person-icht306ee34b/mac) contact in the conversation
    ///
    /// The string is the identifier stored by Messages for the mentioned
    /// participant.
    Mention(String),
    /// Clickable link assigned to the text range.
    ///
    /// The string is the URL payload, including schemes such as `https:`,
    /// `tel:`, or `mailto:`.
    Link(String),
    /// One-time code detected in the message text.
    OTP,
    /// Postal address detected in the message text.
    ///
    /// Boxed because [`DetectedAddress`] is large enough to otherwise set the
    /// size of every `TextEffect`.
    Address(Box<DetectedAddress>),
    /// Traditional text formatting styles.
    ///
    /// Multiple styles on the same text range are grouped here.
    Styles(Vec<Style>),
    /// Animation applied to the text.
    ///
    /// Messages stores at most one animation on a text range.
    Animated(Animation),
    /// Unit or timezone conversion detected in the message text.
    Conversion(Unit),
    /// Monetary amount detected in the message text.
    ///
    /// Carries the detector's symbol and amount strings.
    Currency(DetectedCurrency),
    /// Package-tracking number detected in the message text.
    ///
    /// Carries the carrier when `DataDetectorsCore` resolved one.
    Tracking(ShipmentTracking),
    /// Flight reference detected in the message text.
    ///
    /// Carries the airline code when available plus the flight number.
    Flight(Flight),
}
