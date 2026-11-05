//! Parameter value text shared by every export format.
//!
//! CLAP and the native table (VST3, AUv2, AAX, LV2) format and parse values
//! with these functions, so hosts show the same text everywhere.
use crate::product::{Parameter, ParameterRole};
use ravet_foundation::ParameterScale;
use std::fmt::{self, Write};

/// Write a physical value as text: clamped and snapped first, then `On`/`Off`
/// for the bypass parameter, whole numbers for integral stepped values and
/// logarithmic values of 100 or more, two decimals otherwise, followed by a
/// space and the unit when the parameter has one. Nonfinite values fail.
pub fn write_value(parameter: &Parameter<'_>, value: f64, output: &mut impl Write) -> fmt::Result {
    let range = parameter.range();
    let value = range.canonicalize(value as f32).ok_or(fmt::Error)?;
    if parameter.role() == ParameterRole::Bypass {
        return output.write_str(if value >= 0.5 { "On" } else { "Off" });
    }
    let decimals = match (range.intervals(), range.scale()) {
        (Some(_), _) if value.fract() == 0.0 => 0,
        (_, ParameterScale::Logarithmic) if value.abs() >= 100.0 => 0,
        _ => 2,
    };
    write!(output, "{value:.decimals$}")?;
    if !parameter.unit().is_empty() {
        write!(output, " {}", parameter.unit())?;
    }
    Ok(())
}

/// [`write_value`] into a new string.
pub fn format_value(parameter: &Parameter<'_>, value: f64) -> Option<String> {
    let mut text = String::new();
    write_value(parameter, value, &mut text).ok()?;
    Some(text)
}

/// Parse text written by [`write_value`] or typed by a user: a number with an
/// optional unit suffix, or `On`/`Off` for the bypass parameter. The result is
/// clamped and snapped to the parameter's range.
pub fn parse_value(parameter: &Parameter<'_>, text: &str) -> Option<f32> {
    let text = text.trim();
    let number = text.strip_suffix(parameter.unit()).unwrap_or(text).trim();
    let value = match (parameter.role(), number) {
        (ParameterRole::Bypass, "On" | "on") => 1.0,
        (ParameterRole::Bypass, "Off" | "off") => 0.0,
        _ => number.parse::<f32>().ok()?,
    };
    parameter.range().canonicalize(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trips() {
        let gain = Parameter::linear("gain", "Gain", -60.0, 12.0, 0.0).with_unit("dB");
        assert_eq!(format_value(&gain, -6.5).unwrap(), "-6.50 dB");
        assert_eq!(format_value(&gain, 99.0).unwrap(), "12.00 dB");
        assert_eq!(parse_value(&gain, " -6.50 dB "), Some(-6.5));
        assert_eq!(parse_value(&gain, "-100"), Some(-60.0));
        assert_eq!(parse_value(&gain, "loud"), None);
        assert!(format_value(&gain, f64::NAN).is_none());

        let tone = Parameter::logarithmic("tone", "Tone", 20.0, 20_000.0, 1000.0).with_unit("Hz");
        assert_eq!(format_value(&tone, 1234.4).unwrap(), "1234 Hz");
        assert_eq!(format_value(&tone, 55.123).unwrap(), "55.12 Hz");

        let steps = Parameter::stepped("division", "Division", 0.0, 9.0, 9, 3.0);
        assert_eq!(format_value(&steps, 3.4).unwrap(), "3");
        assert_eq!(parse_value(&steps, "3.6"), Some(4.0));

        let bypass = Parameter::bypass("bypass", "Bypass");
        assert_eq!(format_value(&bypass, 1.0).unwrap(), "On");
        assert_eq!(format_value(&bypass, 0.0).unwrap(), "Off");
        assert_eq!(parse_value(&bypass, "on"), Some(1.0));
        assert_eq!(parse_value(&bypass, "Off"), Some(0.0));
    }
}
