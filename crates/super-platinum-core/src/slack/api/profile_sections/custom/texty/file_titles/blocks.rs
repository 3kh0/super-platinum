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
    label_with_emoji_context(items, quip, canvas_fallback, None, user)
}
pub(super) fn label_with_emoji_context(
    items: &[Value],
    quip: QuipTitle<'_>,
    canvas_fallback: Option<&str>,
    emoji: Option<super::EmojiText>,
    user: &mut impl FnMut(&str) -> Result<String, Unavailable>,
) -> Result<String, Unavailable> {
    label_with_resolvers(items, quip, canvas_fallback, emoji, user, &mut |_| {
        Err(Unavailable::UnsupportedTitle)
    })
}

pub(super) fn label_with_resolvers(
    items: &[Value],
    quip: QuipTitle<'_>,
    canvas_fallback: Option<&str>,
    emoji: Option<super::EmojiText>,
    user: &mut impl FnMut(&str) -> Result<String, Unavailable>,
    channel: &mut impl FnMut(&str) -> Result<String, Unavailable>,
) -> Result<String, Unavailable> {
    label_with_group_resolver(
        items,
        quip,
        canvas_fallback,
        emoji,
        user,
        channel,
        &mut |_| Err(Unavailable::UnsupportedTitle),
    )
}

pub(super) fn label_with_group_resolver(
    items: &[Value],
    quip: QuipTitle<'_>,
    canvas_fallback: Option<&str>,
    emoji: Option<super::EmojiText>,
    user: &mut impl FnMut(&str) -> Result<String, Unavailable>,
    channel: &mut impl FnMut(&str) -> Result<String, Unavailable>,
    group: &mut impl FnMut(&str) -> Result<String, Unavailable>,
) -> Result<String, Unavailable> {
    let mut has_user = false;
    let mut blocks = Vec::with_capacity(items.len());
    for block in items {
        let mut sections = Vec::new();
        for section in elements(block, "rich_text")? {
            let mut text = String::new();
            for leaf in elements(section, "rich_text_section")? {
                match classify(leaf, emoji)? {
                    Leaf::Text(value) => text.push_str(value),
                    Leaf::User(id) => {
                        text.push_str(&user(id)?);
                        has_user = true;
                    }
                    Leaf::Channel(id) => text.push_str(&channel(id)?),
                    Leaf::UserGroup(id) => text.push_str(&group(id)?),
                    Leaf::Emoji(value) => text.push_str(&value),
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

pub(super) fn elements<'a>(value: &'a Value, kind: &str) -> Result<&'a [Value], Unavailable> {
    if value.get("type").and_then(Value::as_str) != Some(kind) {
        return Err(Unavailable::UnsupportedTitle);
    }
    value
        .get("elements")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or(Unavailable::UnsupportedTitle)
}

/// One grammar for extraction and dependency scanning; styles/opaque fields
/// remain untouched rather than normalized into the display label.
pub(super) enum Leaf<'a> {
    Text(&'a str),
    User(&'a str),
    Channel(&'a str),
    UserGroup(&'a str),
    Emoji(String),
}
pub(super) fn classify(
    leaf: &Value,
    emoji: Option<super::EmojiText>,
) -> Result<Leaf<'_>, Unavailable> {
    Ok(match leaf.get("type").and_then(Value::as_str) {
        Some("text") => Leaf::Text(
            leaf.get("text")
                .and_then(Value::as_str)
                .ok_or(Unavailable::UnsupportedTitle)?,
        ),
        Some("user") => Leaf::User(super::users::id(leaf)?),
        Some("channel") => Leaf::Channel(super::channels::id(leaf)?),
        Some("usergroup") => Leaf::UserGroup(super::groups::id(leaf)?),
        Some("link") => Leaf::Text(super::link::text(leaf)?),
        Some("emoji") => Leaf::Emoji(super::emoji::text(leaf, emoji)?),
        _ => return Err(Unavailable::UnsupportedTitle),
    })
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
