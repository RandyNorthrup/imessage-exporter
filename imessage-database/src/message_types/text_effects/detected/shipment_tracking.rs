use crate::util::data_detected::{FromScannerResult, ScannerResult};

/// Package-tracking metadata emitted for a detected range in message text.
///
/// Apple's `DataDetectorsCore` framework tags tracking numbers under the shared
/// `__kIMDataDetectedAttributeName` attribute as a `TrackingNumber`
/// [`ScannerResult`]. The number is the result's matched text, and the carrier
/// is expressed as the *type* of the sole nested result (`UPS`, `DHL`, `USPS`,
/// `FedEx`, etc.), so it is read from that child's kind rather than a value
/// field.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ShipmentTracking {
    /// The carrier, taken from the nested result's type, e.g. `UPS`.
    ///
    /// `None` when the detector did not attribute the number to a carrier. The
    /// name is whatever `DataDetectorsCore` emitted, kept verbatim rather than
    /// matched against a fixed carrier list.
    pub carrier: Option<String>,
    /// The tracking number exactly as it appeared.
    pub number: String,
}

impl FromScannerResult for ShipmentTracking {
    /// Tracking numbers use the shared data-detector attribute, so the raw
    /// payload is checked for `TrackingNumber` before plist parsing.
    const MARKERS: &[&[u8]] = &[b"TrackingNumber"];

    fn from_scanner_result(result: &ScannerResult<'_>) -> Option<Self> {
        if result.kind()? != "TrackingNumber" {
            return None;
        }
        Some(Self {
            carrier: result
                .children()
                .next()
                .and_then(|carrier| carrier.kind())
                .map(str::to_string),
            number: result.matched()?.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{env::current_dir, fs::File};

    use plist::Value;

    use super::ShipmentTracking;
    use crate::util::data_detected::{FromScannerResult, ScannerResult};

    fn parse(name: &str) -> Option<ShipmentTracking> {
        let path = current_dir()
            .unwrap()
            .join("test_data/tracking_properties")
            .join(name);
        let plist = Value::from_reader(File::open(path).unwrap()).unwrap();
        ScannerResult::root(&plist).and_then(|root| ShipmentTracking::from_scanner_result(&root))
    }

    #[test]
    fn parses_ups_tracking() {
        assert_eq!(
            parse("TrackingUps.plist"),
            Some(ShipmentTracking {
                carrier: Some("UPS".to_string()),
                number: "1Z999AA10123456784".to_string(),
            })
        );
    }
}
