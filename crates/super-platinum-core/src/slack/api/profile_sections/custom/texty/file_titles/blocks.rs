//! Strict bQFi text-run extraction and laT0's independent Quip title branch.
use super::{QuipTitle, Unavailable, js_space};
use regress::Regex;
use serde_json::Value;
use std::sync::OnceLock;

pub(super) fn label(items: &[Value], quip: QuipTitle<'_>) -> Result<String, Unavailable> {
    label_with_users(items, quip, &mut |_| Err(Unavailable::UnsupportedTitle))
}
pub(super) fn label_with_users(
    items: &[Value],
    quip: QuipTitle<'_>,
    user: &mut impl FnMut(&str) -> Result<String, Unavailable>,
) -> Result<String, Unavailable> {
    label_with_context(items, quip, None, user)
}
pub(super) fn label_with_context(
    items: &[Value],
    quip: QuipTitle<'_>,
    canvas_fallback: Option<&str>,
    user: &mut impl FnMut(&str) -> Result<String, Unavailable>,
) -> Result<String, Unavailable> {
    let mut has_user = false;
    let mut blocks = Vec::with_capacity(items.len());
    for block in items {
        let mut sections = Vec::new();
        for section in elements(block, "rich_text")? {
            let mut text = String::new();
            for leaf in elements(section, "rich_text_section")? {
                match leaf.get("type").and_then(Value::as_str) {
                    Some("text") => text.push_str(
                        leaf.get("text")
                            .and_then(Value::as_str)
                            .ok_or(Unavailable::UnsupportedTitle)?,
                    ),
                    Some("user") => {
                        let id = super::users::id(leaf)?;
                        text.push_str(&user(id)?);
                        has_user = true;
                    }
                    Some("link") => text.push_str(super::link::text(leaf)?),
                    _ => return Err(Unavailable::UnsupportedTitle),
                }
            }
            sections.push(text);
        }
        let joined = sections.join("\n");
        let trimmed = joined.trim_matches(js_space);
        if !trimmed.is_empty() {
            blocks.push(trimmed.to_owned());
        }
    }
    let extracted = blocks.join("\n\n");
    // laT0 consults localized Untitled before its nullish override selection.
    let extracted = if extracted.is_empty() {
        canvas_fallback.ok_or(Unavailable::LocalizedFallback)?
    } else {
        &extracted
    };
    let selected = if has_user {
        extracted
    } else {
        match quip {
            QuipTitle::Nullish => extracted,
            QuipTitle::Text(value) => value,
            QuipTitle::Unavailable => return Err(Unavailable::UnsupportedTitle),
        }
    };
    Ok(trim_emoji(selected))
}

fn elements<'a>(value: &'a Value, kind: &str) -> Result<&'a [Value], Unavailable> {
    if value.get("type").and_then(Value::as_str) != Some(kind) {
        return Err(Unavailable::UnsupportedTitle);
    }
    value
        .get("elements")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or(Unavailable::UnsupportedTitle)
}

// QIRz.L uses the nonglobal original pH4l expression, only at offset zero.
// Match UTF-16 code units like the original non-u JS regex. Its character class
// is BMP-only, so stripping a match cannot bisect an admitted Unicode scalar.
fn trim_emoji(value: &str) -> String {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(include_str!("emoji-pattern.txt").trim_end_matches(['\r', '\n']))
            .expect("retained pH4l fixed expression")
    });
    let units: Vec<_> = value.encode_utf16().collect();
    if let Some(found) = pattern.find_from_ucs2(&units, 0).next()
        && found.range.start == 0
    {
        return String::from_utf16(&units[found.range.end..])
            .expect("BMP-only prefix preserves scalar boundaries")
            .trim_matches(js_space)
            .to_owned();
    }
    value.to_owned()
}

#[cfg(test)]
mod tests;
