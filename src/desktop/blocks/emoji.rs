//! Raw rich-text emoji facts, before the ordinary shortcode/media projection.
//!
//! Positive Unicode uses a selected reverse map before deciding between
//! supplied artwork and a glyph. Unobserved callers retain the native glyph.
//! Standard glyphs use the native Unicode presentation already used by this app;
//! this does not reproduce Slack's sprite artwork or workspace emoji policy.

use super::{BlockCtx, RichNode, Value, text::emoji_node};

pub(super) fn node(ctx: BlockCtx<'_>, value: &Value) -> Option<RichNode> {
    let name = value.get("name").and_then(Value::as_str).unwrap_or("emoji");
    if let Some(unicode) = value.get("unicode").and_then(Value::as_str)
        && !unicode.is_empty()
    {
        // A malformed positive Unicode value must not silently paint another
        // emoji from the name. ILYW/WXwC require lowercase 4–5 digit hex groups.
        let glyph = unicode_glyph(unicode)?;
        if let Some(canonical) = ctx.standard_emoji.and_then(|map| map.unicode_name(unicode))
            && let Some(url) = display_url(value)
        {
            // qJ's reverse name replaces the raw name/alias for this branch.
            // Preserve the original URL, including standard-name collisions.
            return Some(image(ctx, canonical, url));
        }
        return Some(RichNode::Emoji {
            name: name.to_owned(),
            glyph,
        });
    }

    if let Some(node) = explicit_image(ctx, value) {
        return Some(node);
    }

    // Preserve the original custom-media path. Only a positively resolved
    // standard tone changes a name-only glyph; no preferred tone is inferred.
    let tone = value.get("skin_tone").or_else(|| value.get("skinTone"));
    let tone = match tone {
        Some(Value::String(tone)) => Some(tone.clone()),
        Some(Value::Number(tone)) if tone.is_u64() => Some(tone.to_string()),
        _ => None,
    };
    if let Some(tone) = tone {
        let toned = format!("{name}::skin-tone-{tone}");
        let glyph = super_platinum_core::state::emoji_glyph(&toned);
        if glyph != format!(":{toned}:") && ctx.workspace.custom_emoji_url(name).is_none() {
            return Some(RichNode::Emoji {
                name: name.to_owned(),
                glyph,
            });
        }
    }
    Some(emoji_node(ctx, name))
}

/// Original supplied appearance, not a current-workspace catalog entry.
/// Keep it URL-keyed and unslotted so equal aliases in different teams cannot
/// borrow each other's cached artwork. The media registry owns credentials.
fn explicit_image(ctx: BlockCtx<'_>, value: &Value) -> Option<RichNode> {
    // Positive Unicode was handled using the caller's selected reverse map.
    // This original-name/alias branch accepts only absent Unicode.
    if !value
        .get("unicode")
        .is_none_or(|v| v.is_null() || v.as_str() == Some(""))
    {
        return None;
    }
    let original = value
        .get("name")?
        .as_str()
        .filter(|name| !name.is_empty())?;
    let url = display_url(value)?;
    let name = match field(value, "display_alias", "displayAlias") {
        None | Some(Value::Null) => original,
        Some(Value::String(alias)) => alias,
        _ => return None,
    };
    Some(image(ctx, name, url))
}

fn display_url(value: &Value) -> Option<&str> {
    field(value, "display_url", "displayUrl")?
        .as_str()
        .filter(|url| super_platinum_core::state::is_browser_url(url))
}

fn image(ctx: BlockCtx<'_>, name: &str, url: &str) -> RichNode {
    RichNode::EmojiImage {
        id: ctx
            .media
            .register_image(super_platinum_core::MediaAssetKind::Emoji, url, "image/png"),
        name: name.to_owned(),
    }
}

fn field<'a>(value: &'a Value, snake: &str, camel: &str) -> Option<&'a Value> {
    value.get(snake).or_else(|| value.get(camel))
}

fn unicode_glyph(unicode: &str) -> Option<String> {
    unicode
        .split('-')
        .map(|part| {
            if !(4..=5).contains(&part.len())
                || !part
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return None;
            }
            // Rust strings cannot carry isolated UTF-16 surrogates. Refuse
            // them instead of manufacturing a replacement glyph.
            char::from_u32(u32::from_str_radix(part, 16).ok()?)
        })
        .collect()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod combined_tests;
