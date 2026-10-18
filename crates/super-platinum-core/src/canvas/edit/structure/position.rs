//! Q XuI8 interval generation; malformed/exhausted intervals fail closed.
use super::*;

pub(in crate::canvas::edit) fn canonical(value: &str) -> Result<(), Error> {
    if value.is_empty() || value.len() > 256 || !value.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Err(Error::Unsupported);
    }
    Ok(())
}
pub(in crate::canvas::edit) fn base(value: &str) -> Result<&str, Error> {
    let value = value.strip_suffix(":temp").unwrap_or(value);
    canonical(value)?;
    Ok(value)
}
pub(in crate::canvas::edit) fn modern_id(value: &str) -> bool {
    value.len() == 35
        && value.starts_with("temp:C:")
        && value.as_bytes()[7..10]
            .iter()
            .all(u8::is_ascii_alphanumeric)
        && value.as_bytes()[10..]
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
}
fn valid(before: Option<&str>, value: &str, after: Option<&str>) -> bool {
    !value.is_empty()
        && before.is_none_or(|b| b < value && value != format!("{b}A"))
        && after.is_none_or(|a| value < a)
}
fn increment(value: &str) -> String {
    let mut bytes = value.as_bytes().to_vec();
    for i in (0..bytes.len()).rev() {
        if bytes[i] == b'z' {
            bytes[i] = b'B';
        } else {
            bytes[i] = if bytes[i] == b'Z' { b'a' } else { bytes[i] + 1 };
            return String::from_utf8(bytes).expect("ASCII");
        }
    }
    String::new()
}
fn decrement(value: &str) -> String {
    let mut bytes = value.as_bytes().to_vec();
    for i in (0..bytes.len()).rev() {
        if matches!(bytes[i], b'A' | b'B') {
            bytes[i] = b'z';
        } else {
            bytes[i] = if bytes[i] == b'a' { b'Z' } else { bytes[i] - 1 };
            return String::from_utf8(bytes).expect("ASCII");
        }
    }
    String::new()
}
pub(in crate::canvas::edit) fn between(
    before: Option<&str>,
    after: Option<&str>,
) -> Result<String, Error> {
    for value in before.into_iter().chain(after) {
        canonical(value)?;
    }
    if before.zip(after).is_some_and(|(b, a)| b >= a) {
        return Err(Error::Changed);
    }
    let result = if before.is_none() && after.is_none() {
        "aaa".to_owned()
    } else {
        let mut candidates = Vec::new();
        if let Some(b) = before {
            candidates.push(increment(b));
        }
        if let Some(a) = after {
            candidates.push(decrement(a));
        }
        if let Some(b) = before {
            candidates.push(format!("{b}aaaa"));
        }
        if let Some(a) = after {
            if a.ends_with('B') && before.is_none_or(|b| a.starts_with(b)) {
                candidates.push(format!("{}Aaaaaaa", &a[..a.len() - 1]));
            }
        }
        candidates
            .into_iter()
            .find(|v| valid(before, v, after))
            .ok_or(Error::Unsupported)?
    };
    canonical(&result)?;
    if !valid(before, &result, after) {
        return Err(Error::Unsupported);
    }
    Ok(format!("{result}:temp"))
}
pub(in crate::canvas::edit) fn section_id(
    snapshot: &CanvasCollabSnapshot,
    suffix: &str,
) -> Result<String, Error> {
    if suffix.len() != 25
        || !suffix
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Unsupported);
    }
    let shard = snapshot
        .document
        .string(42)?
        .filter(|s| !s.is_empty())
        .or_else(|| snapshot.document_id().get(..3))
        .ok_or(Error::Unsupported)?;
    if shard.len() != 3 || !shard.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(Error::Unsupported);
    }
    Ok(format!("temp:C:{shard}{suffix}"))
}
