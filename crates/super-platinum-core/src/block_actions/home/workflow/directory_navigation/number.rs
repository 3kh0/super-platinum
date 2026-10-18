//! ECMAScript Number coercion for JSON-representable history timestamp values.
use serde_json::Value;

pub(super) fn number(value: Option<&Value>) -> Result<f64, ()> {
    Ok(match value {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Bool(b)) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => string_number(s),
        Some(Value::Array(a)) => string_number(&array_string(a)?),
        Some(Value::Object(map)) => {
            // JSON cannot contain callable methods. An own toString shadows
            // Object.prototype.toString; valueOf cannot then yield a primitive.
            if map.contains_key("toString") {
                return Err(());
            }
            f64::NAN
        }
    })
}
fn array_string(values: &[Value]) -> Result<String, ()> {
    Ok(values
        .iter()
        .map(|v| match v {
            Value::Null => Ok(String::new()),
            other => string(other),
        })
        .collect::<Result<Vec<_>, _>>()?
        .join(","))
}
pub(super) fn string(value: &Value) -> Result<String, ()> {
    Ok(match value {
        Value::String(s) => s.clone(),
        Value::Array(a) => array_string(a)?,
        Value::Object(map) => {
            if map.contains_key("toString") {
                return Err(());
            }
            "[object Object]".to_owned()
        }
        other => other.to_string(),
    })
}
fn whitespace(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}
fn string_number(input: &str) -> f64 {
    let s = input.trim_matches(whitespace);
    if s.is_empty() {
        return 0.0;
    }
    match s {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    for (prefixes, radix) in [(["0x", "0X"], 16u32), (["0b", "0B"], 2), (["0o", "0O"], 8)] {
        if prefixes.iter().any(|prefix| s.starts_with(prefix)) {
            let digits = &s[2..];
            if digits.is_empty() {
                return f64::NAN;
            }
            let width = radix.trailing_zeros() as usize;
            let mut bits = Vec::new();
            for c in digits.chars() {
                let Some(digit) = c.to_digit(radix).filter(|_| c.is_ascii()) else {
                    return f64::NAN;
                };
                for bit in (0..width).rev() {
                    let value = (digit >> bit) & 1;
                    if value != 0 || !bits.is_empty() {
                        bits.push(value);
                    }
                }
            }
            if bits.len() > 1024 {
                return f64::INFINITY;
            }
            let mut mantissa = bits
                .iter()
                .take(53)
                .fold(0u64, |n, bit| n * 2 + u64::from(*bit));
            if bits.len() > 53 && bits[53] == 1 && (bits[54..].contains(&1) || mantissa & 1 == 1) {
                mantissa += 1;
            }
            return mantissa as f64 * 2.0f64.powi(bits.len().saturating_sub(53) as i32);
        }
    }
    // Rust additionally accepts inf and case-insensitive infinity; JS does not.
    // Restrict its parser to the decimal StringNumericLiteral alphabet.
    if !s
        .bytes()
        .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'e' | b'E'))
    {
        return f64::NAN;
    }
    s.parse().unwrap_or(f64::NAN)
}
