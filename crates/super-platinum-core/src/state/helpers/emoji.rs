use std::collections::HashMap;

use crate::slack::models::Emoji;

use super::files::is_browser_url;

#[derive(Debug)]
pub struct StandardEmoji {
    pub name: &'static str,
    pub glyph: &'static str,
    pub aliases: String,
}

/// Build the searchable, offline catalog once, only when an emoji UI opens.
pub fn standard_emoji_catalog() -> &'static [StandardEmoji] {
    static CATALOG: std::sync::OnceLock<Vec<StandardEmoji>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        emojis::iter()
            .filter_map(|emoji| {
                Some(StandardEmoji {
                    name: emoji.shortcode()?,
                    glyph: emoji.as_str(),
                    aliases: emoji.shortcodes().collect::<Vec<_>>().join(" "),
                })
            })
            .collect()
    })
}

pub fn reaction_summary(reaction: &crate::slack::models::Reaction) -> String {
    format!("{} {}", emoji_glyph(&reaction.name), reaction.count.max(1))
}

/// Map a Slack emoji name to its unicode glyph, falling back to `:name:` for
/// custom/unknown emoji. Slack appends skin-tone modifiers like
/// `thumbsup::skin-tone-3`; the base name resolves the glyph.
pub fn emoji_glyph(name: &str) -> String {
    let base = name.split("::").next().unwrap_or(name);
    emojis::get_by_shortcode(base)
        .map(|e| e.as_str().to_owned())
        .unwrap_or_else(|| format!(":{name}:"))
}

pub fn is_standard_emoji(name: &str) -> bool {
    let base = name.split("::").next().unwrap_or(name);
    emojis::get_by_shortcode(base).is_some()
}

pub fn emoji_text_to_display(text: &str) -> String {
    emoji_text_tokens(text)
        .into_iter()
        .map(|token| match token {
            EmojiTextToken::Text(text) => text,
            EmojiTextToken::Emoji(name) => emoji_glyph(&name),
        })
        .collect()
}

pub fn emoji_preview_key(team: &str, name: &str) -> String {
    format!("{team}:{name}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmojiTextToken {
    Text(String),
    Emoji(String),
}

pub fn emoji_text_tokens(text: &str) -> Vec<EmojiTextToken> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(':') {
        let (before, after_start) = rest.split_at(start);
        if !before.is_empty() {
            out.push(EmojiTextToken::Text(before.to_owned()));
        }

        let after_start = &after_start[1..];
        let Some(end) = after_start.find(':') else {
            out.push(EmojiTextToken::Text(":".to_owned()));
            rest = after_start;
            continue;
        };
        let name = &after_start[..end];
        if is_emoji_name(name) {
            out.push(EmojiTextToken::Emoji(name.to_owned()));
            rest = &after_start[end + 1..];
        } else {
            out.push(EmojiTextToken::Text(":".to_owned()));
            rest = after_start;
        }
    }
    if !rest.is_empty() {
        out.push(EmojiTextToken::Text(rest.to_owned()));
    }
    merge_text_tokens(out)
}

pub fn emoji_names_in_text(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    visit_emoji_names_in_text(text, |name| names.push(name.to_owned()));
    names
}

pub fn visit_emoji_names_in_text<'a>(text: &'a str, mut visit: impl FnMut(&'a str)) {
    let mut rest = text;
    while let Some(start) = rest.find(':') {
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find(':') else {
            break;
        };
        let name = &after_start[..end];
        if is_emoji_name(name) {
            visit(name);
            rest = &after_start[end + 1..];
        } else {
            rest = after_start;
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum EmojiSource<'a> {
    Url(&'a str),
    Unicode(&'static str),
}

/// Follow aliases, including standard emoji. Reject cycles and unsafe URLs.
pub fn custom_emoji_source<'a>(
    emojis: &'a HashMap<String, Emoji>,
    name: &str,
) -> Option<EmojiSource<'a>> {
    let mut current = name;
    let mut seen = std::collections::HashSet::new();
    loop {
        if !seen.insert(current) {
            return None;
        }
        let Some(emoji) = emojis.get(current) else {
            return emojis::get_by_shortcode(current)
                .map(|emoji| EmojiSource::Unicode(emoji.as_str()));
        };
        if let Some(alias) = emoji.value.strip_prefix("alias:") {
            current = alias;
            continue;
        }
        return is_browser_url(&emoji.value).then_some(EmojiSource::Url(emoji.value.as_str()));
    }
}

pub(crate) fn custom_emoji_url<'a>(
    emojis: &'a HashMap<String, Emoji>,
    name: &str,
) -> Option<&'a str> {
    match custom_emoji_source(emojis, name)? {
        EmojiSource::Url(url) => Some(url),
        EmojiSource::Unicode(_) => None,
    }
}

fn is_emoji_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+'))
}

fn merge_text_tokens(tokens: Vec<EmojiTextToken>) -> Vec<EmojiTextToken> {
    let mut merged = Vec::new();
    for token in tokens {
        match (merged.last_mut(), token) {
            (Some(EmojiTextToken::Text(existing)), EmojiTextToken::Text(next)) => {
                existing.push_str(&next);
            }
            (_, token) => merged.push(token),
        }
    }
    merged
}
