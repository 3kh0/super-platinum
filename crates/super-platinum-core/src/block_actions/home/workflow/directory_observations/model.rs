use super::{Error, response};
use crate::composer_emoji::model::Model;
use serde_json::Value;
use std::collections::BTreeMap;

/// None is an observed absent model, for which l24D selects its named fallback.
/// Fetch failure is handled by the owner; malformed data is never that fallback.
pub fn decode_model(value: &Value) -> Result<Option<Model>, Error> {
    response(value)?;
    let raw = &value["model"];
    if raw.is_null() || raw == false || raw.as_f64() == Some(0.0) || raw == "" {
        return Ok(None);
    }
    let object = raw.as_object().ok_or(Error::Model)?;
    let mut weights = BTreeMap::new();
    for (key, value) in object {
        // Slack feature identifiers are ASCII. Refuse ambiguous case collisions
        // rather than guessing source insertion order after JSON map decoding.
        if !key.is_ascii()
            || weights
                .insert(key.to_ascii_uppercase(), number(value)?)
                .is_some()
        {
            return Err(Error::Model);
        }
    }
    Model::from_weights(weights)
        .map(Some)
        .map_err(|_| Error::Model)
}

fn whitespace(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

fn string_number(s: &str) -> Result<f64, Error> {
    let s = s.trim_matches(whitespace);
    if s.is_empty() {
        return Ok(0.0);
    }
    let radix = [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ];
    for (prefix, radix) in radix {
        if let Some(digits) = s.strip_prefix(prefix) {
            if digits.is_empty() {
                return Err(Error::Model);
            }
            let mut result = 0_u64;
            for c in digits.chars() {
                let digit = c
                    .to_digit(radix)
                    .filter(|_| c.is_ascii())
                    .ok_or(Error::Model)?;
                result = result
                    .checked_mul(u64::from(radix))
                    .and_then(|v| v.checked_add(u64::from(digit)))
                    .ok_or(Error::Model)?;
            }
            return Ok(result as f64);
        }
    }
    // Rust accepts inf/NaN spellings that JS Number does not. All are refused,
    // as are source Infinity results; Model only admits finite weights.
    let result = s.parse::<f64>().map_err(|_| Error::Model)?;
    result.is_finite().then_some(result).ok_or(Error::Model)
}

fn array_string(value: &Value) -> Result<String, Error> {
    match value {
        Value::Null => Ok(String::new()),
        Value::String(s) => Ok(s.clone()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Array(a) => a
            .iter()
            .map(array_string)
            .collect::<Result<Vec<_>, _>>()
            .map(|s| s.join(",")),
        Value::Object(_) => Err(Error::Model),
    }
}

fn number(value: &Value) -> Result<f64, Error> {
    match value {
        Value::Null => Ok(0.0),
        Value::Bool(v) => Ok(if *v { 1.0 } else { 0.0 }),
        Value::Number(v) => v.as_f64().filter(|v| v.is_finite()).ok_or(Error::Model),
        Value::String(v) => string_number(v),
        Value::Array(_) => string_number(&array_string(value)?),
        Value::Object(_) => Err(Error::Model),
    }
}
