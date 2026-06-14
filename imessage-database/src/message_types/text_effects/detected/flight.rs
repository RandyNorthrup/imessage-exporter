use crate::util::data_detected::{FromScannerResult, ScannerResult};

/// Flight metadata emitted for a detected range in message text.
///
/// Apple's `DataDetectorsCore` framework tags flights under the shared
/// `__kIMDataDetectedAttributeName` attribute as a `FlightInformation`
/// [`ScannerResult`] whose children carry the airline and flight number.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Flight {
    /// The airline's IATA code, e.g. `UA`.
    ///
    /// Read from the `AirlineCode` result's value when present (the normalized
    /// code) and otherwise its matched text, since the match may be the airline
    /// name (`Alaska`, `Allegiant Air`) while the value holds the code
    /// (`AS`, `G4`). `None` when no airline was resolved.
    pub airline: Option<String>,
    /// The flight number exactly as it appeared, e.g. `100`.
    pub number: String,
}

impl FromScannerResult for Flight {
    /// Flights use the shared data-detector attribute, so the raw payload is
    /// checked for `FlightInformation` before plist parsing.
    const MARKERS: &[&[u8]] = &[b"FlightInformation"];

    fn from_scanner_result(result: &ScannerResult<'_>) -> Option<Self> {
        if result.kind()? != "FlightInformation" {
            return None;
        }
        let mut airline = None;
        let mut number = None;
        for child in result.children() {
            match child.kind() {
                // Prefer the normalized code in the value over the matched name.
                Some("AirlineCode") => {
                    airline = child
                        .value()
                        .or_else(|| child.matched())
                        .map(str::to_string);
                }
                Some("FlightNumber") => number = child.matched().map(str::to_string),
                _ => {}
            }
        }
        Some(Self {
            airline,
            number: number?,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{env::current_dir, fs::File};

    use plist::Value;

    use super::Flight;
    use crate::util::data_detected::{FromScannerResult, ScannerResult};

    fn parse(name: &str) -> Option<Flight> {
        let path = current_dir()
            .unwrap()
            .join("test_data/flight_properties")
            .join(name);
        let plist = Value::from_reader(File::open(path).unwrap()).unwrap();
        ScannerResult::root(&plist).and_then(|root| Flight::from_scanner_result(&root))
    }

    #[test]
    fn parses_flight_with_airline_code() {
        assert_eq!(
            parse("FlightCode.plist"),
            Some(Flight {
                airline: Some("UA".to_string()),
                number: "1111".to_string(),
            })
        );
    }

    #[test]
    fn prefers_airline_code_value_over_name() {
        assert_eq!(
            parse("FlightNamedAirline.plist"),
            Some(Flight {
                airline: Some("G4".to_string()),
                number: "1111".to_string(),
            })
        );
    }
}
