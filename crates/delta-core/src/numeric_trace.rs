//! Rendering of bound report figures (ADR-0055).
//!
//! A report refers to a computed value as `{{result:<path>#<key>|<format>}}`. This module parses
//! such a binding and renders a raw value with its format. Rounding is round half up (a 5 in the
//! first dropped digit rounds away from zero), applied to the shortest decimal form that reads
//! back as the same double, so 2.675 renders as 2.68 at two decimals. Binary floating-point
//! artefacts and banker's rounding are not used.

/// The formats a binding may declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Fixed decimals, `%.Nf`.
    Fixed(usize),
    /// Whole number, `%.0f`.
    Integer,
}

/// A parsed binding: the result artifact path, the key inside it, and the format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub path: String,
    pub key: String,
    pub format: Format,
}

/// Parse a binding such as `{{result:out/shelf_life.json#shelf_life_months|%.0f}}`.
pub fn parse_binding(text: &str) -> Result<Binding, String> {
    let inner = text
        .trim()
        .strip_prefix("{{result:")
        .and_then(|rest| rest.strip_suffix("}}"))
        .ok_or_else(|| "a binding has the form {{result:<path>#<key>|<format>}}".to_owned())?;
    let (target, format) = inner
        .rsplit_once('|')
        .ok_or_else(|| "a binding names its format after '|'".to_owned())?;
    let (path, key) = target
        .split_once('#')
        .ok_or_else(|| "a binding names its key after '#'".to_owned())?;
    if path.trim().is_empty() || key.trim().is_empty() {
        return Err("a binding names a result path and a key".to_owned());
    }
    Ok(Binding {
        path: path.to_owned(),
        key: key.to_owned(),
        format: parse_format(format)?,
    })
}

/// Parse a format: `%.Nf` for N decimals (0 to 12), or `%.0f`/`integer`.
pub fn parse_format(text: &str) -> Result<Format, String> {
    let text = text.trim();
    if text == "integer" {
        return Ok(Format::Integer);
    }
    let decimals = text
        .strip_prefix("%.")
        .and_then(|rest| rest.strip_suffix('f'))
        .ok_or_else(|| format!("unknown format: {text}"))?;
    let decimals: usize = decimals
        .parse()
        .map_err(|_| format!("unknown format: {text}"))?;
    if decimals > 12 {
        return Err(format!("too many decimals in {text}"));
    }
    Ok(if decimals == 0 {
        Format::Integer
    } else {
        Format::Fixed(decimals)
    })
}

/// Render `value` in `format`. The value must be finite.
pub fn render(value: f64, format: Format) -> Result<String, String> {
    if !value.is_finite() {
        return Err("a bound value must be a finite number".to_owned());
    }
    let decimals = match format {
        Format::Fixed(decimals) => decimals,
        Format::Integer => 0,
    };
    Ok(round_half_up(value, decimals))
}

/// Round `value` to `decimals` places, half up, on the shortest decimal form of the double.
pub fn round_half_up(value: f64, decimals: usize) -> String {
    // Rust's Display prints the shortest decimal that reads back as the same double, and never
    // uses an exponent, so the digits below are exactly the decimal the value is written as.
    let text = format!("{value}");
    let negative = text.starts_with('-');
    let unsigned = text.trim_start_matches('-');
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let mut digits: Vec<u8> = whole
        .bytes()
        .chain(fraction.bytes())
        .map(|b| b - b'0')
        .collect();
    let point = whole.len();
    let keep = point + decimals;
    let round_up = digits.get(keep).is_some_and(|digit| *digit >= 5);
    digits.truncate(keep);
    while digits.len() < keep {
        digits.push(0);
    }
    if round_up {
        let mut index = digits.len();
        loop {
            if index == 0 {
                digits.insert(0, 1);
                break;
            }
            index -= 1;
            if digits[index] == 9 {
                digits[index] = 0;
            } else {
                digits[index] += 1;
                break;
            }
        }
    }
    // After a carry the integer part may have one more digit.
    let point = digits.len() - decimals;
    let mut out = String::new();
    if negative && digits.iter().any(|digit| *digit != 0) {
        out.push('-');
    }
    for (index, digit) in digits.iter().enumerate() {
        if index == point && decimals > 0 {
            out.push('.');
        }
        out.push((b'0' + digit) as char);
    }
    if out.starts_with('.') {
        out.insert(0, '0');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_half_up_on_the_shortest_decimal_form() {
        assert_eq!(round_half_up(2.675, 2), "2.68");
        assert_eq!(round_half_up(1.005, 2), "1.01");
        assert_eq!(round_half_up(0.125, 2), "0.13");
        assert_eq!(round_half_up(99.995, 2), "100.00");
        assert_eq!(round_half_up(2.5, 0), "3");
        assert_eq!(round_half_up(2.4999, 0), "2");
    }

    #[test]
    fn rounds_negative_values_away_from_zero_on_a_half() {
        assert_eq!(round_half_up(-2.5, 0), "-3");
        assert_eq!(round_half_up(-0.125, 2), "-0.13");
        // Rounding a negative value to zero drops the sign.
        assert_eq!(round_half_up(-0.001, 2), "0.00");
    }

    #[test]
    fn keeps_the_requested_number_of_decimals() {
        assert_eq!(round_half_up(4.0, 2), "4.00");
        assert_eq!(round_half_up(0.5, 0), "1");
        assert_eq!(round_half_up(0.0, 3), "0.000");
    }

    #[test]
    fn parses_a_binding_and_its_format() {
        let binding =
            parse_binding("{{result:out/shelf_life.json#shelf_life_months|%.0f}}").unwrap();
        assert_eq!(binding.path, "out/shelf_life.json");
        assert_eq!(binding.key, "shelf_life_months");
        assert_eq!(binding.format, Format::Integer);
        assert_eq!(
            parse_binding("{{result:a.json#mean|%.3f}}").unwrap().format,
            Format::Fixed(3)
        );
    }

    #[test]
    fn rejects_malformed_bindings_and_unknown_formats() {
        assert!(parse_binding("{{result:a.json|%.2f}}").is_err());
        assert!(parse_binding("{{result:#mean|%.2f}}").is_err());
        assert!(parse_binding("{{mean}}").is_err());
        assert!(parse_format("%.2e").is_err());
        assert!(parse_format("%.13f").is_err());
    }

    #[test]
    fn a_rendered_value_is_the_formatted_raw_value() {
        assert_eq!(render(2.675, Format::Fixed(2)).unwrap(), "2.68");
        assert_eq!(render(12.0, Format::Integer).unwrap(), "12");
        assert!(render(f64::NAN, Format::Fixed(2)).is_err());
    }
}
