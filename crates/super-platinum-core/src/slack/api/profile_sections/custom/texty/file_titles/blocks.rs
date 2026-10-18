//! Strict bQFi text-run extraction and laT0's independent Quip title branch.
use super::{QuipTitle, Unavailable, js_space};
use regress::Regex;
use serde_json::Value;
use std::sync::OnceLock;

pub(super) fn label(items: &[Value], quip: QuipTitle<'_>) -> Result<String, Unavailable> {
    if quip == QuipTitle::Unavailable {
        return Err(Unavailable::UnsupportedTitle);
    }
    let mut blocks = Vec::with_capacity(items.len());
    for block in items {
        let mut sections = Vec::new();
        for section in elements(block, "rich_text")? {
            let mut text = String::new();
            for leaf in elements(section, "rich_text_section")? {
                if leaf.get("type").and_then(Value::as_str) != Some("text") {
                    return Err(Unavailable::UnsupportedTitle);
                }
                text.push_str(
                    leaf.get("text")
                        .and_then(Value::as_str)
                        .ok_or(Unavailable::UnsupportedTitle)?,
                );
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
    // This bounded helper does not claim that independent localization input.
    if extracted.is_empty() {
        return Err(Unavailable::LocalizedFallback);
    }
    let selected = match quip {
        QuipTitle::Nullish => &extracted,
        QuipTitle::Text(value) => value,
        QuipTitle::Unavailable => unreachable!(),
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
