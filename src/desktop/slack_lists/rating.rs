//! Bounded Rating display. A readable fallback never grants edit admission.
use super::*;
use crate::{media::MediaRegistry, model::RichNode};
use super_platinum_core::{
    slack_lists::{CellPresentation, Column, Record},
    state::{EmojiSource, Workspace},
};

pub(crate) fn glyph(
    workspace: Option<&Workspace>,
    media: &MediaRegistry,
    emoji: &str,
    team: Option<&str>,
) -> RichNode {
    let name = emoji.trim_matches(':');
    let source = workspace
        .filter(|w| team.is_none_or(|t| t == w.team_id))
        .and_then(|w| super_platinum_core::state::custom_emoji_source(&w.custom_emoji, name));
    match source {
        Some(EmojiSource::Url(url)) => RichNode::EmojiImage {
            id: media.register_emoji(name, url),
            name: name.into(),
        },
        Some(EmojiSource::Unicode(glyph)) => RichNode::Emoji {
            name: name.into(),
            glyph: glyph.into(),
        },
        None => RichNode::Emoji {
            name: name.into(),
            glyph: if super_platinum_core::state::is_standard_emoji(name) {
                super_platinum_core::state::emoji_glyph(name)
            } else {
                emoji.into()
            },
        },
    }
}
pub(super) fn project(
    workspace: Option<&Workspace>,
    media: &MediaRegistry,
    column: &Column,
    record: &Record,
) -> Option<(String, CellContent)> {
    if column.kind != "rating" {
        return None;
    }
    let options = column.raw.get("options");
    let max = match options.and_then(|v| v.get("max")) {
        None => 5,
        Some(v) => u8::try_from(v.as_u64()?)
            .ok()
            .filter(|v| (1..=5).contains(v))?,
    };
    let value = match column.presentation(record) {
        CellPresentation::Empty => 0,
        CellPresentation::Number(n) => u8::try_from(n.as_u64()?).ok().filter(|n| *n <= max)?,
        _ => return None,
    };
    let emoji = options
        .and_then(|v| v.get("emoji"))
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .unwrap_or(":star:");
    if emoji.len() > 256 || emoji.chars().any(char::is_control) {
        return None;
    }
    let team = options
        .and_then(|v| v.get("emoji_team_id"))
        .and_then(Value::as_str);
    let text = if value == 0 {
        String::new()
    } else {
        value.to_string()
    };
    Some((
        text,
        CellContent::Rating {
            value,
            max,
            glyph: glyph(workspace, media, emoji, team),
        },
    ))
}

#[cfg(test)]
mod tests;
