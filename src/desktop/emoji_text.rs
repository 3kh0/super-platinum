//! Supported inline shortcode lookup shared by desktop text surfaces.
//! No entity decoding, markup parsing, navigation or network dispatch.
use crate::state::ShellState;
use super_platinum_core::{MediaAssetId, state::EmojiSource};
mod tokens;
pub(crate) use tokens::split;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    Text(String),
    Emoji {
        text: String,
        title: Option<String>,
        image: Option<MediaAssetId>,
    },
}

pub(crate) fn project(s: &ShellState, team: &str, text: &str, tooltip: bool) -> Vec<Part> {
    split(text)
        .into_iter()
        .map(|(emoji, text)| {
            if emoji {
                single(s, team, text, tooltip)
            } else {
                Part::Text(text.into())
            }
        })
        .collect()
}

pub(crate) fn single(s: &ShellState, team: &str, raw: &str, tooltip: bool) -> Part {
    // The single-component lookup lowercases and strips one boundary colon;
    // it does not trim or tokenize. Preserve unsupported/tone-qualified values
    // verbatim until exact catalog-variant/localization rendering is integrated.
    let lowered = raw.to_lowercase();
    let name = lowered.strip_prefix(':').unwrap_or(&lowered);
    let name = name.strip_suffix(':').unwrap_or(name);
    let fallback = || Part::Text(raw.into());
    if name.is_empty() || name.contains(':') {
        return fallback();
    }
    // 5kVY resolves standard names before workspace custom names. The general
    // Block Kit helper has a different precedence and cannot be used here.
    let (text, image) = if super_platinum_core::state::is_standard_emoji(name) {
        (super_platinum_core::state::emoji_glyph(name), None)
    } else {
        let Some(workspace) = s.core.workspaces.get(team) else {
            return fallback();
        };
        match super_platinum_core::state::custom_emoji_source(&workspace.custom_emoji, name) {
            Some(EmojiSource::Unicode(glyph)) => (glyph.into(), None),
            Some(EmojiSource::Url(url)) => (raw.into(), Some(s.media.register_emoji(name, url))),
            None => return fallback(),
        }
    };
    Part::Emoji {
        text,
        title: tooltip.then(|| raw.to_owned()),
        image,
    }
}

#[cfg(test)]
mod tests;
