//! Positive emoji reaction clauses owned only by the typed Messages editor.
//! The caller supplies its existing opaque-reference-aware lexical ranges.
use std::ops::Range;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fields {
    pub any: String,
    pub mine: String,
}
const KEYS: [&str; 2] = ["has", "hasmy"];
const INVALID: &str = "Use one emoji code, such as :eyes:.";

fn code(value: &str) -> Result<Option<String>, &'static str> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let name = if value.starts_with(':') || value.ends_with(':') {
        value
            .strip_prefix(':')
            .and_then(|s| s.strip_suffix(':'))
            .ok_or(INVALID)?
    } else {
        value
    };
    // A deliberately bounded native code-entry subset, not server validation or
    // emoji discovery. Names remain case-sensitive and are never resolved here.
    if name.is_empty()
        || name.len() > 100
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'+' | b'-'))
    {
        return Err(INVALID);
    }
    Ok(Some(format!(":{name}:")))
}
fn owned<'a>(token: &'a str, key: &str) -> Option<&'a str> {
    let value = token.strip_prefix(key)?.strip_prefix(':')?;
    if !value.starts_with(':') || !value.ends_with(':') {
        return None;
    }
    (code(value).ok().flatten().as_deref() == Some(value)).then_some(value)
}
pub(super) fn project(query: &str, ranges: &[Range<usize>]) -> Fields {
    let get = |key| {
        ranges
            .iter()
            .find_map(|r| owned(&query[r.clone()], key))
            .unwrap_or("")
            .to_owned()
    };
    Fields {
        any: get(KEYS[0]),
        mine: get(KEYS[1]),
    }
}
pub(super) fn edits(
    query: &str,
    ranges: &[Range<usize>],
    old: Option<&Fields>,
    new: Option<&Fields>,
) -> Result<(Vec<Range<usize>>, Vec<String>), &'static str> {
    let empty = Fields::default();
    let old = old.unwrap_or(&empty);
    let new = new.unwrap_or(&empty);
    let mut remove = Vec::new();
    let mut append = Vec::new();
    for (key, before, after) in [
        (KEYS[0], &old.any, &new.any),
        (KEYS[1], &old.mine, &new.mine),
    ] {
        if before == after {
            continue;
        }
        let next = code(after)?;
        // Unchanged multi-clause syntax is byte exact. An explicit edit owns
        // all recognized positive codes of this key; negatives, quoted text,
        // malformed codes and opaque labels remain outside these ranges.
        remove.extend(
            ranges
                .iter()
                .filter(|r| owned(&query[(*r).clone()], key).is_some())
                .cloned(),
        );
        if let Some(next) = next {
            append.push(format!("{key}:{next}"));
        }
    }
    Ok((remove, append))
}

#[cfg(test)]
mod tests;
