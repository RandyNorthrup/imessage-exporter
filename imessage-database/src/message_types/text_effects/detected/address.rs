use crate::util::data_detected::{FromScannerResult, ScannerResult};

/// Postal address metadata emitted for a detected range in message text.
///
/// Apple's `DataDetectorsCore` framework tags addresses under the
/// `__kIMAddressAttributeName` attribute as a `FullAddress` [`ScannerResult`]
/// whose children decompose the match into its components. The tree is nested:
/// `StreetNumber` and `StreetName` sit under `Street`, and `CountryCode` sits
/// under `Country`. This means the structured fields can be read from the
/// archived detector payload instead of inferred from the original text.
///
/// Only [`full`](Self::full) is guaranteed; the remaining components are present
/// only when the detector resolved them (e.g. a bare street with no city).
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct DetectedAddress {
    /// The complete matched address, e.g. `1 Infinite Loop, Cupertino, CA 95014`.
    pub full: String,
    /// The street line, e.g. `1 Infinite Loop`.
    pub street: Option<String>,
    /// The street number, e.g. `1`.
    pub street_number: Option<String>,
    /// The street name, e.g. `Infinite Loop`.
    pub street_name: Option<String>,
    /// The city, e.g. `Cupertino`.
    pub city: Option<String>,
    /// The state or province, as matched, e.g. `CA` or `California`.
    pub state: Option<String>,
    /// The postal (ZIP) code, e.g. `95014`.
    pub zip: Option<String>,
    /// The country as it appeared, e.g. `United States`.
    pub country: Option<String>,
    /// The ISO country code, e.g. `US`.
    ///
    /// Unlike the other fields this is read from the `CountryCode` result's
    /// value rather than its matched text, since the match is the country name
    /// while the value is the normalized code.
    pub country_code: Option<String>,
}

impl FromScannerResult for DetectedAddress {
    /// Accept only `FullAddress` scanner results and extract the useful fields.
    ///
    /// Walks the direct children for `Street`, `City`, `State`, `ZipCode`, and
    /// `Country`, descending into `Street` for the number/name split and into
    /// `Country` for the ISO code. When the detector emits more than one street
    /// number or name (an occasional artifact of a garbled match) the first of
    /// each is kept; the full street line is always available in
    /// [`street`](Self::street).
    fn from_scanner_result(result: &ScannerResult<'_>) -> Option<Self> {
        if result.kind()? != "FullAddress" {
            return None;
        }
        let mut address = Self {
            full: result.matched()?.to_string(),
            street: None,
            street_number: None,
            street_name: None,
            city: None,
            state: None,
            zip: None,
            country: None,
            country_code: None,
        };
        for child in result.children() {
            match child.kind() {
                Some("Street") => {
                    address.street = child.matched().map(str::to_string);
                    for part in child.children() {
                        match part.kind() {
                            Some("StreetNumber") if address.street_number.is_none() => {
                                address.street_number = part.matched().map(str::to_string);
                            }
                            Some("StreetName") if address.street_name.is_none() => {
                                address.street_name = part.matched().map(str::to_string);
                            }
                            _ => {}
                        }
                    }
                }
                Some("City") => address.city = child.matched().map(str::to_string),
                Some("State") => address.state = child.matched().map(str::to_string),
                Some("ZipCode") => address.zip = child.matched().map(str::to_string),
                Some("Country") => {
                    address.country = child.matched().map(str::to_string);
                    address.country_code = child
                        .children()
                        .find(|part| part.kind() == Some("CountryCode"))
                        .and_then(|part| part.value())
                        .map(str::to_string);
                }
                _ => {}
            }
        }
        Some(address)
    }
}

#[cfg(test)]
mod tests {
    use std::{env::current_dir, fs::File};

    use plist::Value;

    use super::DetectedAddress;
    use crate::util::data_detected::{FromScannerResult, ScannerResult};

    /// Exercise the real archived payload before building a `DetectedAddress`.
    fn parse_address(plist: &Value) -> Option<DetectedAddress> {
        ScannerResult::root(plist).and_then(|result| DetectedAddress::from_scanner_result(&result))
    }

    fn address_property(name: &str) -> Value {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/address_properties")
            .join(name);
        Value::from_reader(File::open(plist_path).unwrap()).unwrap()
    }

    #[test]
    fn parses_full_address_with_country() {
        let plist = address_property("FullAddressWithCountry.plist");
        assert_eq!(
            parse_address(&plist),
            Some(DetectedAddress {
                full: "1 Infinite Loop, Cupertino, CA 95014, United States".to_string(),
                street: Some("1 Infinite Loop".to_string()),
                street_number: Some("1".to_string()),
                street_name: Some("Infinite Loop".to_string()),
                city: Some("Cupertino".to_string()),
                state: Some("CA".to_string()),
                zip: Some("95014".to_string()),
                country: Some("United States".to_string()),
                // Read from the CountryCode value, not its matched text.
                country_code: Some("US".to_string()),
            })
        );
    }

    #[test]
    fn ignores_non_address_result() {
        let plist = address_property("FullAddressWithCountry.plist");
        let root = ScannerResult::root(&plist).unwrap();
        // A child result (`Street`) is not itself a `FullAddress`.
        let street = root.children().next().unwrap();
        assert_eq!(DetectedAddress::from_scanner_result(&street), None);
    }
}
