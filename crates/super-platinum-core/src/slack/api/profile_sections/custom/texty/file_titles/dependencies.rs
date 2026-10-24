//! Separate source dependencies; channel/group hits never become user-bypass evidence.
use super::{
    EmojiText, Unavailable,
    blocks::{self, Leaf},
};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TitleDependencies {
    pub users: BTreeSet<String>,
    pub channels: BTreeSet<String>,
    pub groups: BTreeSet<String>,
}

/// Syntax dependencies only. This neither admits a source row nor schedules a
/// request; callers retain each independently owned observation until mount.
pub fn dependencies_with_emoji_context(
    effective: &Value,
    emoji: Option<EmojiText>,
) -> Result<TitleDependencies, Unavailable> {
    scan(effective, emoji, true, false)
}

/// Explicit group-aware syntax scan. No row/source authority or request is
/// created. Legacy scanners continue to refuse group-containing titles.
pub fn dependencies_with_groups(
    effective: &Value,
    emoji: Option<EmojiText>,
) -> Result<TitleDependencies, Unavailable> {
    scan(effective, emoji, true, true)
}

pub(super) fn scan(
    effective: &Value,
    emoji: Option<EmojiText>,
    channels: bool,
    groups: bool,
) -> Result<TitleDependencies, Unavailable> {
    let mut ids = TitleDependencies::default();
    let items = match effective.get("title_blocks") {
        None | Some(Value::Null) => return Ok(ids),
        Some(Value::Array(v)) => v,
        _ => return Err(Unavailable::UnsupportedTitle),
    };
    for block in items {
        for section in blocks::elements(block, "rich_text")? {
            for leaf in blocks::elements(section, "rich_text_section")? {
                match blocks::classify(leaf, emoji)? {
                    Leaf::User(id) => {
                        ids.users.insert(id.to_owned());
                    }
                    Leaf::Channel(id) if channels => {
                        ids.channels.insert(id.to_owned());
                    }
                    Leaf::Channel(_) => return Err(Unavailable::UnsupportedTitle),
                    Leaf::UserGroup(id) if groups => {
                        ids.groups.insert(id.to_owned());
                    }
                    Leaf::UserGroup(_) => return Err(Unavailable::UnsupportedTitle),
                    Leaf::Text(_) | Leaf::Emoji(_) => (),
                }
            }
        }
    }
    Ok(ids)
}
