/*!
 Structured entities Apple detects inside message body text.

 These differ from sender-applied formatting in the parent module
 ([`animation`] and [`style`]): the message text triggers
 `DataDetectorsCore`, which stores a `DDScannerResult` archive on the
 attributed range. Each type owns the parser for the corresponding detector
 shape via [`FromScannerResult`](crate::util::data_detected::FromScannerResult).

 [`animation`]: crate::message_types::text_effects::animation
 [`style`]: crate::message_types::text_effects::style
*/

/// Detected postal addresses.
pub mod address;
/// Detected currency amounts.
pub mod currency;
/// Detected flight references.
pub mod flight;
/// Detected package-tracking numbers.
pub mod shipment_tracking;
/// Detected unit conversions.
pub mod unit;
