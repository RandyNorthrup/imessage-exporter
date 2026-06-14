use crate::util::data_detected::{FromScannerResult, ScannerResult};

/// Unit conversion dimension detected in message text.
///
/// `DataDetectorsCore` stores concrete unit names in the detector payload
/// (`fahrenheit`, `mile per hour`, `gallon-imperial`, etc.). The parser groups
/// those names into these broader dimensions and preserves unmapped names as
/// [`Unknown`](Self::Unknown).
///
/// Read more about unit conversions [here](https://www.macrumors.com/how-to/convert-currencies-temperatures-more-ios-16/).
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Unit {
    /// Angle unit.
    Angle,
    /// Area unit.
    Area,
    /// Distance unit.
    Distance,
    /// Duration unit.
    Duration,
    /// Energy unit.
    Energy,
    /// Fuel efficiency unit.
    FuelEfficiency,
    /// Power unit.
    Power,
    /// Pressure unit.
    Pressure,
    /// Speed unit.
    Speed,
    /// Temperature unit.
    Temperature,
    /// Timezone conversion marker.
    Timezone,
    /// Volume unit.
    Volume,
    /// Weight unit.
    Weight,
    /// Unit name that did not map to any known dimension.
    ///
    /// The embedded data is the raw unit name emitted by `DataDetectorsCore`,
    /// preserved verbatim.
    Unknown(String),
}

impl FromScannerResult for Unit {
    /// Units use the shared data-detector attribute, so the raw payload is
    /// checked for physical-amount markers before plist parsing.
    const MARKERS: &[&[u8]] = &[b"PhysicalAmount", b"Unit"];

    /// A conversion is either a bare `Unit` result or a `PhysicalAmount` that
    /// wraps one; any other scanner-result type is a negative match.
    fn from_scanner_result(result: &ScannerResult<'_>) -> Option<Self> {
        match result.kind()? {
            "Unit" => Self::from_unit_name(result.value()?),
            "PhysicalAmount" => result
                .children()
                .find_map(|child| Self::from_scanner_result(&child)),
            _ => None,
        }
    }
}

impl Unit {
    /// Currency and timezone conversions are detected through other channels
    /// (the scanner-result type and `__kIMCalendarEventAttributeName`), so they
    /// do not appear here. Imperial units arrive as `<base>-imperial`, so the
    /// suffix is stripped before matching; unrecognized names (including
    /// ambiguous composites) surface as [`Unit::Unknown`] carrying the raw,
    /// unstripped name.
    fn from_unit_name(name: &str) -> Option<Self> {
        match name.strip_suffix("-imperial").unwrap_or(name) {
            "celsius" | "fahrenheit" | "kelvin" => Some(Self::Temperature),
            "gram" | "kilogram" | "metric tonne" | "ounce" | "pound" | "stone" | "short ton" => {
                Some(Self::Weight)
            }
            "meter" | "kilometer" | "centimeter" | "millimeter" | "mile" | "yard" | "foot"
            | "inch" => Some(Self::Distance),
            "liter" | "centiliter" | "milliliter" | "cubic meter" | "cubic centimeter"
            | "cubic foot" | "cubic inch" | "gallon" | "pint" | "quart" | "cup" | "fluid ounce"
            | "tablespoon" | "teaspoon" => Some(Self::Volume),
            "watt" | "kilowatt" | "horse power" => Some(Self::Power),
            "kilometer per hour" | "mile per hour" | "meter per second" => Some(Self::Speed),
            "liter per 100 kilometers" | "mile per gallon" => Some(Self::FuelEfficiency),
            "joule" | "kilojoule" | "calorie" | "kilocalorie" | "kilowatt hour" => {
                Some(Self::Energy)
            }
            "hectare" | "acre" | "square kilometer" | "square meter" | "square centimeter"
            | "square mile" | "square yard" | "square foot" | "square inch" => Some(Self::Area),
            "degree" | "radian" | "turn" => Some(Self::Angle),
            "bar" | "kilopascal" | "hectopascal" | "millimeter of mercury" | "psi" => {
                Some(Self::Pressure)
            }
            "hour" | "minute" | "second" | "millisecond" => Some(Self::Duration),
            _ => Some(Self::Unknown(name.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{env::current_dir, fs::File};

    use plist::Value;

    use super::Unit;
    use crate::util::data_detected::{FromScannerResult, ScannerResult};

    /// Exercise the real archived payload before mapping it to a unit.
    fn parse_unit(plist: &Value) -> Option<Unit> {
        ScannerResult::root(plist).and_then(|result| Unit::from_scanner_result(&result))
    }

    fn unit_property(name: &str) -> Value {
        let plist_path = current_dir()
            .unwrap()
            .as_path()
            .join("test_data/unit_properties")
            .join(name);
        let plist_data = File::open(plist_path).unwrap();
        Value::from_reader(plist_data).unwrap()
    }

    #[test]
    fn can_parse_data_detected_physical_units() {
        for (fixture, expected) in [
            ("Temperature.plist", Unit::Temperature),
            ("Volume.plist", Unit::Volume),
            ("Weight.plist", Unit::Weight),
        ] {
            let plist = unit_property(fixture);
            assert_eq!(parse_unit(&plist), Some(expected));
        }
    }

    #[test]
    fn ignores_data_detected_non_units() {
        let plist = unit_property("HttpURL.plist");
        assert_eq!(parse_unit(&plist), None);
    }

    #[test]
    fn maps_unit_name_to_dimension() {
        for (name, expected) in [
            ("fahrenheit", Unit::Temperature),
            ("short ton", Unit::Weight),
            ("centimeter", Unit::Distance),
            ("fluid ounce", Unit::Volume),
            ("gallon-imperial", Unit::Volume), // imperial suffix stripped
            ("watt", Unit::Power),
            ("mile per hour", Unit::Speed),
            ("mile per gallon", Unit::FuelEfficiency),
            ("joule", Unit::Energy),
            ("acre", Unit::Area),
            ("radian", Unit::Angle),
            ("psi", Unit::Pressure),
            ("minute", Unit::Duration),
        ] {
            assert_eq!(Unit::from_unit_name(name), Some(expected), "{name}");
        }
        assert_eq!(
            Unit::from_unit_name("not a unit"),
            Some(Unit::Unknown("not a unit".to_string()))
        );
        // The ambiguous degree glyph arrives as a composite name; it is
        // preserved verbatim rather than dropped.
        assert_eq!(
            Unit::from_unit_name("celsius-fahrenheit-degree"),
            Some(Unit::Unknown("celsius-fahrenheit-degree".to_string()))
        );
    }
}
