//! Parameter value text shared by every export format.
//!
//! CLAP and the native table (VST3, AUv2, AAX, LV2) format and parse values
//! with these functions, so hosts show the same text everywhere.
use crate::product::{Parameter, ParameterRole};
use ravet_foundation::ParameterScale;
use std::fmt::{self, Write};

/// Whether a parameter is an on/off switch: the bypass parameter, or a stepped
/// parameter with exactly the values 0 and 1 and no unit.
fn is_switch(parameter: &Parameter<'_>) -> bool {
    let range = parameter.range();
    parameter.role() == ParameterRole::Bypass
        || (range.intervals() == Some(1)
            && range.minimum() == 0.0
            && range.maximum() == 1.0
            && parameter.unit().is_empty())
}

/// Write a physical value as text: clamped and snapped first, then `On`/`Off`
/// for switches (the bypass parameter and unitless 0/1 toggles). Otherwise a
/// number: whole for integral stepped values and logarithmic values of 1,000 or
/// more, one decimal (dropped when it is zero) from 100, and two decimals below
/// 100, so a value that is not whole never reads as one. The unit follows after
/// a space when the parameter has one. Nonfinite values fail.
pub fn write_value(parameter: &Parameter<'_>, value: f64, output: &mut impl Write) -> fmt::Result {
    let range = parameter.range();
    let value = range.canonicalize(value as f32).ok_or(fmt::Error)?;
    if is_switch(parameter) {
        return output.write_str(if value >= 0.5 { "On" } else { "Off" });
    }
    let decimals = match (range.intervals(), range.scale()) {
        (Some(_), _) if value.fract() == 0.0 => 0,
        (_, ParameterScale::Logarithmic) if value.abs() >= 1000.0 => 0,
        _ if value.abs() >= 100.0 => 1,
        _ => 2,
    };
    let mut number = format!("{value:.decimals$}");
    if decimals == 1 && number.ends_with(".0") {
        number.truncate(number.len() - 2);
    }
    output.write_str(&number)?;
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
/// optional unit suffix, or `On`/`Off` for switches. The result is
/// clamped and snapped to the parameter's range.
pub fn parse_value(parameter: &Parameter<'_>, text: &str) -> Option<f32> {
    let text = text.trim();
    let number = text.strip_suffix(parameter.unit()).unwrap_or(text).trim();
    let value = match (is_switch(parameter), number) {
        (true, "On" | "on") => 1.0,
        (true, "Off" | "off") => 0.0,
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
        assert_eq!(format_value(&tone, 100.3125).unwrap(), "100.3 Hz");
        assert_eq!(format_value(&tone, 820.0).unwrap(), "820 Hz");
        let mix = Parameter::linear("mix", "Mix", 0.0, 100.0, 50.0).with_unit("%");
        assert_eq!(format_value(&mix, 100.0).unwrap(), "100 %");
        assert_eq!(format_value(&mix, 45.0).unwrap(), "45.00 %");
        assert_eq!(format_value(&tone, 55.123).unwrap(), "55.12 Hz");

        let steps = Parameter::stepped("division", "Division", 0.0, 9.0, 9, 3.0);
        assert_eq!(format_value(&steps, 3.4).unwrap(), "3");
        assert_eq!(parse_value(&steps, "3.6"), Some(4.0));

        let bypass = Parameter::bypass("bypass", "Bypass");
        assert_eq!(format_value(&bypass, 1.0).unwrap(), "On");
        assert_eq!(format_value(&bypass, 0.0).unwrap(), "Off");
        assert_eq!(parse_value(&bypass, "on"), Some(1.0));
        assert_eq!(parse_value(&bypass, "Off"), Some(0.0));

        let toggle = Parameter::toggle("sync", "Sync", true);
        assert_eq!(format_value(&toggle, 1.0).unwrap(), "On");
        assert_eq!(format_value(&toggle, 0.0).unwrap(), "Off");
        assert_eq!(parse_value(&toggle, "off"), Some(0.0));
        assert_eq!(parse_value(&toggle, "1"), Some(1.0));
        // A two-value stepped parameter with a unit stays numeric.
        let pair = Parameter::stepped("octave", "Octave", 0.0, 1.0, 1, 0.0).with_unit("oct");
        assert_eq!(format_value(&pair, 1.0).unwrap(), "1 oct");
    }
}
