//! Renderer-neutral completion triggers and bounded Slack reference candidates.
//! The renderer displays labels; only explicit selection inserts a wire token.
use crate::{ComposerState, state::Workspace};

pub mod index;
mod person_scan;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Person,
    Channel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trigger {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
    pub query: String,
}

impl Trigger {
    /// UTF-8 byte offsets, matching ComposerState. A selection is not a query.
    pub fn at(text: &str, anchor: usize, focus: usize) -> Option<Self> {
        if anchor != focus || !text.is_char_boundary(focus) {
            return None;
        }
        let before = text.get(..focus)?;
        let (start, marker) = before.char_indices().rev().find(|(_, ch)| {
            matches!(ch, '@' | '#')
                || ch.is_whitespace()
                || matches!(
                    ch,
                    '<' | '>' | '`' | '/' | ':' | '(' | ')' | '[' | ']' | '{' | '}'
                )
        })?;
        let kind = match marker {
            '@' => Kind::Person,
            '#' => Kind::Channel,
            _ => return None,
        };
        if start > 0
            && !before[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_whitespace() || matches!(c, '(' | '[' | '{'))
        {
            return None;
        }
        if in_code(&text[..start]) {
            return None;
        }
        let query = &text[start + 1..focus];
        if query.chars().any(|c| {
            c.is_control()
                || (c.is_ascii() && !c.is_ascii_alphanumeric() && !matches!(c, '.' | '_' | '-'))
        }) {
            return None;
        }
        Some(Self {
            kind,
            start,
            end: focus,
            query: query.into(),
        })
    }
    /// Reject stale query/caret snapshots before applying a chosen reference.
    pub fn insert(&self, composer: &mut ComposerState, choice: &Suggestion) -> bool {
        if Self::at(
            &composer.text,
            composer.selection.start,
            composer.selection.end,
        )
        .as_ref()
            != Some(self)
        {
            return false;
        }
        let Some(token) = choice.target.token() else {
            return false;
        };
        let matches_kind = matches!(
            (&self.kind, &choice.target),
            (Kind::Channel, Target::Channel(_))
                | (
                    Kind::Person,
                    Target::User(_) | Target::Group(_) | Target::Broadcast(_)
                )
        );
        if !matches_kind {
            return false;
        }
        let spacer = if composer.text[self.end..].chars().next().is_some_and(|c| {
            c.is_whitespace() || matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | ')' | ']' | '}')
        }) {
            ""
        } else {
            " "
        };
        composer.set_selection(self.start, self.end);
        composer.replace_selection(&format!("{token}{spacer}"));
        true
    }
}

fn in_code(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut delimiter = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 1;
            if i < bytes.len() {
                i += 1;
            }
            continue;
        }
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i] == b'`' {
            i += 1;
        }
        let length = i - start;
        if delimiter == 0 {
            delimiter = length;
        } else if length == delimiter {
            delimiter = 0;
        }
    }
    delimiter != 0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Broadcast {
    Here,
    Channel,
    Everyone,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Target {
    User(String),
    Channel(String),
    Group(String),
    Broadcast(Broadcast),
}
impl Target {
    pub fn token(&self) -> Option<String> {
        let (prefix, id) = match self {
            Self::User(id) => ("@", id.as_str()),
            Self::Channel(id) => ("#", id.as_str()),
            Self::Group(id) => ("!subteam^", id.as_str()),
            Self::Broadcast(b) => {
                return Some(format!(
                    "<!{}>",
                    match b {
                        Broadcast::Here => "here",
                        Broadcast::Channel => "channel",
                        Broadcast::Everyone => "everyone",
                    }
                ));
            }
        };
        (!id.is_empty() && id.bytes().all(|c| c.is_ascii_alphanumeric()))
            .then(|| format!("<{prefix}{id}>"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Suggestion {
    pub label: String,
    pub detail: String,
    pub target: Target,
}
pub const LIMIT: usize = 8;

/// Rank exact/prefix before substring; keep only visible candidates while
/// scanning, rather than sorting or cloning a large workspace directory.
pub fn suggestions(workspace: &Workspace, trigger: &Trigger) -> Vec<Suggestion> {
    if trigger.kind == Kind::Person {
        return person_scan::suggestions(workspace, trigger);
    }
    type Rank = (bool, bool, u8, String, String);
    let query = trigger.query.to_lowercase();
    let mut best: Vec<(Rank, Suggestion)> = Vec::with_capacity(LIMIT + 1);
    let mut add = |label: String, detail: String, target: Target, names: &[&str], category: u8| {
        if target.token().is_none() {
            return;
        }
        let folded: Vec<_> = names
            .iter()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase())
            .collect();
        if !query.is_empty() && !folded.iter().any(|s| s.contains(&query)) {
            return;
        }
        let rank = (
            !folded.iter().any(|s| s == &query),
            !folded.iter().any(|s| s.starts_with(&query)),
            category,
            label.to_lowercase(),
            target.token().unwrap(),
        );
        let index = best.partition_point(|(old, _)| old < &rank);
        if index < LIMIT {
            best.insert(
                index,
                (
                    rank,
                    Suggestion {
                        label,
                        detail,
                        target,
                    },
                ),
            );
            best.truncate(LIMIT);
        }
    };
    match trigger.kind {
        Kind::Person => unreachable!("Person scan dispatched above"),
        Kind::Channel => {
            for channel in workspace
                .channels
                .values()
                .filter(|c| !c.is_archived && !c.is_im && !c.is_mpim)
            {
                let Some(name) = channel.name.as_deref().filter(|s| !s.is_empty()) else {
                    continue;
                };
                let detail = if channel.is_private || channel.is_group {
                    "Private channel"
                } else {
                    "Channel"
                };
                add(
                    format!("#{name}"),
                    detail.into(),
                    Target::Channel(channel.id.clone()),
                    &[name],
                    0,
                );
            }
        }
    }
    best.into_iter().map(|(_, item)| item).collect()
}

#[cfg(test)]
mod tests;
