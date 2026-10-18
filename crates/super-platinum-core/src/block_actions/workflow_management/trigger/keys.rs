//! Bounded key transforms from ijf8/pctx. Acquired raw values are retained.
//! Non-ASCII transformed keys need Lodash's Unicode/deburr contract, so they
//! remain unavailable rather than being rewritten approximately.
use super::*;

fn words(key: &str) -> Result<Vec<String>, Error> {
    if !key.is_ascii() {
        return Err(invalid());
    }
    let chars = key.bytes().filter(|c| *c != b'\'').collect::<Vec<_>>();
    // Lodash treats English ordinals as special words. Until the full Unicode
    // word matcher is available, refuse these rather than splitting 1st as 1_st.
    if chars.windows(3).any(|v| {
        v[0].is_ascii_digit()
            && matches!(
                (v[1].to_ascii_lowercase(), v[2].to_ascii_lowercase()),
                (b's', b't') | (b'n', b'd') | (b'r', b'd') | (b't', b'h')
            )
    }) {
        return Err(invalid());
    }
    let mut out = Vec::new();
    let mut word = String::new();
    for (i, c) in chars.iter().copied().enumerate() {
        if !c.is_ascii_alphanumeric() {
            if !word.is_empty() {
                out.push(std::mem::take(&mut word));
            }
            continue;
        }
        let previous = i.checked_sub(1).map(|n| chars[n]);
        let next = chars.get(i + 1).copied();
        let boundary = previous.is_some_and(|p| {
            p.is_ascii_alphanumeric()
                && (p.is_ascii_digit() != c.is_ascii_digit()
                    || p.is_ascii_lowercase() && c.is_ascii_uppercase()
                    || p.is_ascii_uppercase()
                        && c.is_ascii_uppercase()
                        && next.is_some_and(|n| n.is_ascii_lowercase()))
        });
        if boundary && !word.is_empty() {
            out.push(std::mem::take(&mut word));
        }
        word.push((c as char).to_ascii_lowercase());
    }
    if !word.is_empty() {
        out.push(word);
    }
    Ok(out)
}
fn snake(key: &str) -> Result<String, Error> {
    Ok(words(key)?.join("_"))
}
fn camel(key: &str) -> Result<String, Error> {
    let mut result = String::new();
    for (i, word) in words(key)?.into_iter().enumerate() {
        if i == 0 {
            result.push_str(&word);
        } else {
            let mut chars = word.chars();
            if let Some(c) = chars.next() {
                result.push(c.to_ascii_uppercase());
                result.extend(chars);
            }
        }
    }
    Ok(result)
}
fn map(
    value: &Value,
    depth: usize,
    limit: usize,
    ignored: &[String],
    is_camel: bool,
) -> Result<Value, Error> {
    if depth > limit {
        return Ok(value.clone());
    }
    match value {
        Value::Array(values) => values
            .iter()
            .map(|v| map(v, depth + 1, limit, ignored, is_camel))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(values) => {
            let mut result = Map::new();
            for (key, value) in values {
                let mapped = if ignored.contains(key) {
                    key.clone()
                } else if is_camel {
                    camel(key)?
                } else {
                    snake(key)?
                };
                if result
                    .insert(mapped, map(value, depth + 1, limit, ignored, is_camel)?)
                    .is_some()
                {
                    return Err(invalid());
                }
            }
            Ok(Value::Object(result))
        }
        _ => Ok(value.clone()),
    }
}
pub(super) fn camelize(value: &Value, depth: usize) -> Result<Value, Error> {
    map(value, depth, 5, &[], true)
}
pub(super) fn wire(value: &Value) -> Result<Value, Error> {
    let mut ignored = Vec::new();
    for pointer in ["/inputs", "/webhook/schema", "/shortcut/schema"] {
        if let Some(object) = value.pointer(pointer).and_then(Value::as_object) {
            ignored.extend(object.keys().cloned());
        }
    }
    map(
        value,
        if value.get("external").is_some() {
            2
        } else {
            3
        },
        5,
        &ignored,
        false,
    )
}
