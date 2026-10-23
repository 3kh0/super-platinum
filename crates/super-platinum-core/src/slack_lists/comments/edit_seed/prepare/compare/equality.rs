//! JSON-domain f66B default (non-strict) deepEqual. It is not serde equality.
use super::*;
use std::collections::BTreeMap;

pub(in super::super) fn deep(a: Option<&Value>, b: Option<&Value>) -> Result<bool> {
    compare(a, b, 0)
}
fn compare(a: Option<&Value>, b: Option<&Value>, depth: usize) -> Result<bool> {
    if depth > 128 {
        return Err(SeedUnavailable::Limit);
    }
    let object = |v: Option<&Value>| v.is_some_and(|v| v.is_array() || v.is_object());
    if truthy(a) && truthy(b) && (object(a) || object(b)) {
        if !object(a) || !object(b) {
            return Ok(false);
        }
        let (a, b) = (a.unwrap(), b.unwrap());
        // A raw JSON snapshot has no cross-document JavaScript object-identity
        // witness for an object-valued own prototype. Do not invent one.
        let (ap, bp) = (a.get("prototype"), b.get("prototype"));
        if object(ap) || object(bp) {
            return Err(SeedUnavailable::MalformedMessage);
        }
        if !strict(ap, bp) {
            return Ok(false);
        }
        let a = keys(a);
        let b = keys(b);
        if a.len() != b.len() || a.keys().ne(b.keys()) {
            return Ok(false);
        }
        let mut ordered: Vec<_> = a.into_iter().collect();
        // Array#sort compares UTF-16 code units, then f66B walks backwards.
        ordered.sort_by(|(a, _), (b, _)| a.encode_utf16().cmp(b.encode_utf16()));
        for (key, value) in ordered.into_iter().rev() {
            if !compare(Some(value), b.get(&key).copied(), depth + 1)? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    primitive(a, b)
}
fn keys(value: &Value) -> BTreeMap<String, &Value> {
    match value {
        Value::Array(values) => values
            .iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v))
            .collect(),
        Value::Object(values) => values.iter().map(|(k, v)| (k.clone(), v)).collect(),
        _ => BTreeMap::new(),
    }
}
fn strict(a: Option<&Value>, b: Option<&Value>) -> bool {
    match (a, b) {
        (None, None) | (Some(Value::Null), Some(Value::Null)) => true,
        (Some(Value::Bool(a)), Some(Value::Bool(b))) => a == b,
        (Some(Value::String(a)), Some(Value::String(b))) => a == b,
        (Some(Value::Number(a)), Some(Value::Number(b))) => a.as_f64() == b.as_f64(),
        _ => false,
    }
}
fn primitive(a: Option<&Value>, b: Option<&Value>) -> Result<bool> {
    if strict(a, b) {
        return Ok(true);
    }
    let nullish = |v: Option<&Value>| matches!(v, None | Some(Value::Null));
    if nullish(a) || nullish(b) {
        return Ok(nullish(a) && nullish(b));
    }
    let (a, b) = (a.unwrap(), b.unwrap());
    if a.is_object() || a.is_array() {
        return object_falsy(a, b);
    }
    if b.is_object() || b.is_array() {
        return object_falsy(b, a);
    }
    if a.is_string() && b.is_string() {
        return Ok(false);
    }
    Ok(number(a)? == number(b)?)
}
fn number(value: &Value) -> Result<f64> {
    match value {
        Value::Bool(value) => Ok(if *value { 1.0 } else { 0.0 }),
        Value::Number(value) => value.as_f64().ok_or(SeedUnavailable::MalformedMessage),
        Value::String(value) => string_number(value),
        _ => Ok(f64::NAN),
    }
}
fn whitespace(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
fn string_number(value: &str) -> Result<f64> {
    let text = value.trim_matches(whitespace);
    if text.is_empty() {
        return Ok(0.0);
    }
    if matches!(text, "Infinity" | "+Infinity") {
        return Ok(f64::INFINITY);
    }
    if text == "-Infinity" {
        return Ok(f64::NEG_INFINITY);
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0b", 2),
        ("0B", 2),
        ("0o", 8),
        ("0O", 8),
    ] {
        if let Some(digits) = text.strip_prefix(prefix) {
            if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                return Ok(f64::NAN);
            }
            // Beyond u128, a separate exact JS integer rounding implementation
            // would be needed. Refuse rather than silently miscompare.
            return u128::from_str_radix(digits, radix)
                .map(|n| n as f64)
                .map_err(|_| SeedUnavailable::Limit);
        }
    }
    let bytes = text.as_bytes();
    let mut i = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let mut digits = 0;
    while bytes.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
        digits += 1
    }
    if bytes.get(i) == Some(&b'.') {
        i += 1;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
            digits += 1
        }
    }
    if digits == 0 {
        return Ok(f64::NAN);
    }
    if matches!(bytes.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(bytes.get(i), Some(b'+' | b'-')) {
            i += 1
        }
        let start = i;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1
        }
        if start == i {
            return Ok(f64::NAN);
        }
    }
    if i != bytes.len() {
        return Ok(f64::NAN);
    }
    text.parse::<f64>()
        .map_err(|_| SeedUnavailable::MalformedMessage)
}
// Only a falsy primitive can reach default deepEqual's loose object comparison;
// truthy primitive/object pairs already fail objEquiv's typeof check above.
fn object_falsy(object: &Value, primitive: &Value) -> Result<bool> {
    let (empty, zero) = string_class(object, 0)?;
    match primitive {
        Value::String(s) if s.is_empty() => Ok(empty),
        Value::Bool(false) => Ok(zero),
        Value::Number(n) if n.as_f64() == Some(0.0) => Ok(zero),
        _ => Ok(false),
    }
}
// Classify Array#toString for the only two comparisons reached here (""/0).
// Ordinary objects stringify to the inherited tag; own noncallable toString
// causes ToPrimitive to throw, including when nested in Array#join.
fn string_class(value: &Value, depth: usize) -> Result<(bool, bool)> {
    if depth > 128 {
        return Err(SeedUnavailable::Limit);
    }
    match value {
        Value::Array(values) => {
            let mut only = (true, true);
            for value in values {
                only = string_class(value, depth + 1)?;
            }
            Ok(match values.len() {
                0 => (true, true),
                1 => only,
                _ => (false, false),
            })
        }
        Value::Object(value) => {
            if value.contains_key("toString") {
                return Err(SeedUnavailable::MalformedMessage);
            }
            Ok((false, false))
        }
        Value::Null => Ok((true, true)),
        Value::String(s) => Ok((s.is_empty(), string_number(s)? == 0.0)),
        Value::Number(n) => Ok((false, n.as_f64() == Some(0.0))),
        Value::Bool(_) => Ok((false, false)),
    }
}

// AttributeMap.diff reads ordinary objects, so absent Object.prototype function
// names are not undefined. A string can equal Function#toString under f66B,
// requiring an engine-specific observation we do not retain. Refuse that case.
// The __proto__ output setter is a separate unsupported boundary.
pub(super) fn attribute(key: &str, a: Option<&Value>, b: Option<&Value>) -> Result<bool> {
    if key == "__proto__" {
        return Err(SeedUnavailable::MalformedMessage);
    }
    if a.is_none() != b.is_none()
        && matches!(
            key,
            "constructor"
                | "toString"
                | "toLocaleString"
                | "valueOf"
                | "hasOwnProperty"
                | "isPrototypeOf"
                | "propertyIsEnumerable"
                | "__defineGetter__"
                | "__defineSetter__"
                | "__lookupGetter__"
                | "__lookupSetter__"
        )
    {
        if a.or(b).is_some_and(Value::is_string) {
            return Err(SeedUnavailable::MalformedMessage);
        }
        return Ok(false);
    }
    deep(a, b)
}

// serde does not retain AttributeMap's insertion order. Validate every value
// before comparison so a different earlier key cannot mask a source throw or
// unavailable identity. This intentionally refuses some source-accepted maps.
pub(in super::super) fn attribute_values(
    attributes: &serde_json::Map<String, Value>,
) -> Result<()> {
    if attributes.contains_key("__proto__") {
        return Err(SeedUnavailable::MalformedMessage);
    }
    for value in attributes.values() {
        attribute_value(value, 0)?;
    }
    Ok(())
}
pub(super) fn attribute_value(value: &Value, depth: usize) -> Result<()> {
    if depth > 128 {
        return Err(SeedUnavailable::Limit);
    }
    match value {
        Value::Object(values) => {
            if values.contains_key("toString")
                || values
                    .get("prototype")
                    .is_some_and(|v| v.is_object() || v.is_array())
            {
                return Err(SeedUnavailable::MalformedMessage);
            }
            for value in values.values() {
                attribute_value(value, depth + 1)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                attribute_value(value, depth + 1)?;
            }
        }
        // The numeric coercion implementation has an explicit u128 radix bound.
        // Check it independently of lost top-level traversal order too.
        Value::String(value) => {
            string_number(value)?;
        }
        _ => {}
    }
    Ok(())
}
