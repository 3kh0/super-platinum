//! Immutable pre-folded completion candidates. Construction belongs off the UI thread.
//! Snapshot contains only search metadata, never workspace histories or media.
use super::{Broadcast, Kind, LIMIT, Suggestion, Target, Trigger};
use crate::state::Workspace;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
struct User {
    key: String,
    id: String,
    deleted: bool,
    name: Option<String>,
    real: Option<String>,
    display: Option<String>,
    profile_real: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Channel {
    key: String,
    id: String,
    name: Option<String>,
    archived: bool,
    im: bool,
    mpim: bool,
    private: bool,
    group: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Group {
    key: String,
    id: String,
    handle: String,
    name: String,
    deleted: u64,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    users: Vec<User>,
    channels: Vec<Channel>,
    groups: Vec<Group>,
}
impl Snapshot {
    /// Copy only fields read by suggestions. Caller separately captures account,
    /// workspace/mount owner before moving this into a worker.
    pub fn capture(workspace: &Workspace) -> Self {
        Self {
            users: workspace
                .users
                .iter()
                .map(|(key, u)| User {
                    key: key.clone(),
                    id: u.id.clone(),
                    deleted: u.deleted,
                    name: u.name.clone(),
                    real: u.real_name.clone(),
                    display: u.profile.as_ref().and_then(|p| p.display_name.clone()),
                    profile_real: u.profile.as_ref().and_then(|p| p.real_name.clone()),
                })
                .collect(),
            channels: workspace
                .channels
                .iter()
                .map(|(key, c)| Channel {
                    key: key.clone(),
                    id: c.id.clone(),
                    name: c.name.clone(),
                    archived: c.is_archived,
                    im: c.is_im,
                    mpim: c.is_mpim,
                    private: c.is_private,
                    group: c.is_group,
                })
                .collect(),
            groups: workspace
                .usergroups
                .iter()
                .map(|(key, g)| Group {
                    key: key.clone(),
                    id: g.id.clone(),
                    handle: g.handle.clone(),
                    name: g.name.clone(),
                    deleted: g.date_delete,
                })
                .collect(),
        }
    }

    /// Exact allocation-free check before EVERY reuse and after a background
    /// build. Ordering is included: equal legacy rank tuples can preserve map
    /// iteration order for inconsistent duplicate IDs. A rehash may therefore
    /// conservatively require rebuilding even when logical fields are unchanged.
    pub fn matches(&self, ws: &Workspace) -> bool {
        ws.users.len() == self.users.len()
            && ws.channels.len() == self.channels.len()
            && ws.usergroups.len() == self.groups.len()
            && self
                .users
                .iter()
                .zip(ws.users.iter())
                .all(|(old, (key, u))| {
                    old.key == *key
                        && old.id == u.id
                        && old.deleted == u.deleted
                        && old.name == u.name
                        && old.real == u.real_name
                        && old.display.as_ref()
                            == u.profile.as_ref().and_then(|p| p.display_name.as_ref())
                        && old.profile_real.as_ref()
                            == u.profile.as_ref().and_then(|p| p.real_name.as_ref())
                })
            && self
                .channels
                .iter()
                .zip(ws.channels.iter())
                .all(|(old, (key, c))| {
                    old.key == *key
                        && old.id == c.id
                        && old.name == c.name
                        && old.archived == c.is_archived
                        && old.im == c.is_im
                        && old.mpim == c.is_mpim
                        && old.private == c.is_private
                        && old.group == c.is_group
                })
            && self
                .groups
                .iter()
                .zip(ws.usergroups.iter())
                .all(|(old, (key, g))| {
                    old.key == *key
                        && old.id == g.id
                        && old.handle == g.handle
                        && old.name == g.name
                        && old.deleted == g.date_delete
                })
    }

    /// Owned string capacities only; excludes Vec storage and allocator overhead.
    pub fn string_capacity_bytes(&self) -> usize {
        self.users
            .iter()
            .map(|u| {
                u.key.capacity()
                    + u.id.capacity()
                    + [&u.name, &u.real, &u.display, &u.profile_real]
                        .into_iter()
                        .flatten()
                        .map(String::capacity)
                        .sum::<usize>()
            })
            .sum::<usize>()
            + self
                .channels
                .iter()
                .map(|c| {
                    c.key.capacity() + c.id.capacity() + c.name.as_ref().map_or(0, String::capacity)
                })
                .sum::<usize>()
            + self
                .groups
                .iter()
                .map(|g| {
                    g.key.capacity() + g.id.capacity() + g.handle.capacity() + g.name.capacity()
                })
                .sum::<usize>()
    }
}

/// Correctness-safe cache value without mutation hooks. All reuse must call the
/// checked methods while holding the canonical state read; no await in between.
#[derive(Debug)]
pub struct CheckedIndex {
    snapshot: Snapshot,
    index: Index,
}
impl CheckedIndex {
    pub fn build(snapshot: Snapshot) -> Self {
        let index = Index::build(&snapshot);
        Self { snapshot, index }
    }
    pub fn matches(&self, ws: &Workspace) -> bool {
        self.snapshot.matches(ws)
    }
    pub fn suggestions(&self, ws: &Workspace, trigger: &Trigger) -> Option<Vec<Suggestion>> {
        self.matches(ws).then(|| self.index.suggestions(trigger))
    }
    /// A copied old menu row is never upgraded to current authority. Revalidate
    /// the whole directory and exact current bounded row, including its label.
    pub fn admits(&self, ws: &Workspace, trigger: &Trigger, choice: &Suggestion) -> bool {
        self.suggestions(ws, trigger)
            .is_some_and(|choices| choices.contains(choice))
    }
    /// Caller must also admit its renderer/account/mount owner. Trigger::insert
    /// enforces current trigger/caret and same kind; the renderer must separately
    /// check its full text/selection snapshot, including text outside the trigger.
    pub fn insert_if_current(
        &self,
        ws: &Workspace,
        composer: &mut crate::ComposerState,
        trigger: &Trigger,
        choice: &Suggestion,
    ) -> bool {
        self.admits(ws, trigger, choice) && trigger.insert(composer, choice)
    }
    pub fn string_capacity_bytes(&self) -> usize {
        self.snapshot.string_capacity_bytes() + self.index.string_capacity_bytes()
    }
    pub fn index_string_capacity_bytes(&self) -> usize {
        self.index.string_capacity_bytes()
    }
    pub fn snapshot_string_capacity_bytes(&self) -> usize {
        self.snapshot.string_capacity_bytes()
    }
    /// For isolated benchmark attribution only. Runtime callers use suggestions.
    pub fn unchecked_index(&self) -> &Index {
        &self.index
    }
}

#[derive(Debug)]
struct Entry {
    choice: Suggestion,
    names: Vec<String>,
    category: u8,
    label: String,
    token: String,
}
impl Entry {
    fn make(
        choice: Suggestion,
        names: impl IntoIterator<Item = String>,
        category: u8,
    ) -> Option<Self> {
        let token = choice.target.token()?;
        Some(Self {
            label: choice.label.to_lowercase(),
            names: names
                .into_iter()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_lowercase())
                .collect(),
            choice,
            token,
            category,
        })
    }
}

#[derive(Debug)]
pub struct Index {
    people: Vec<Entry>,
    channels: Vec<Entry>,
}
impl Index {
    pub fn build(snapshot: &Snapshot) -> Self {
        let mut people = Vec::new();
        let mut channels = Vec::new();
        // The original algorithm resolves display_name by u.id, not iteration
        // key. Preserve that behavior even for inconsistent imported metadata.
        let users: HashMap<_, _> = snapshot.users.iter().map(|u| (u.key.as_str(), u)).collect();
        for user in snapshot.users.iter().filter(|u| !u.deleted) {
            let display = users
                .get(user.id.as_str())
                .and_then(|source| {
                    [
                        source.display.as_ref(),
                        source.profile_real.as_ref(),
                        source.real.as_ref(),
                        source.name.as_ref(),
                    ]
                    .into_iter()
                    .flatten()
                    .find(|s| !s.trim().is_empty())
                    .map(|s| s.trim().to_owned())
                })
                .unwrap_or_else(|| user.id.clone());
            let handle = user.name.clone().unwrap_or_default();
            let real = user
                .real
                .clone()
                .or_else(|| user.profile_real.clone())
                .unwrap_or_default();
            let detail = if handle.is_empty() {
                real.clone()
            } else {
                format!("@{handle}")
            };
            if let Some(entry) = Entry::make(
                Suggestion {
                    label: format!("@{display}"),
                    detail,
                    target: Target::User(user.id.clone()),
                },
                [display, handle, real],
                0,
            ) {
                people.push(entry);
            }
        }
        for group in snapshot
            .groups
            .iter()
            .filter(|g| g.deleted == 0 && !g.handle.is_empty())
        {
            if let Some(entry) = Entry::make(
                Suggestion {
                    label: format!("@{}", group.handle),
                    detail: group.name.clone(),
                    target: Target::Group(group.id.clone()),
                },
                [group.handle.clone(), group.name.clone()],
                1,
            ) {
                people.push(entry);
            }
        }
        for (name, broadcast, detail) in [
            (
                "here",
                Broadcast::Here,
                "Notify people online in this conversation",
            ),
            (
                "channel",
                Broadcast::Channel,
                "Notify everyone in this conversation",
            ),
            (
                "everyone",
                Broadcast::Everyone,
                "Notify everyone in the workspace",
            ),
        ] {
            people.push(
                Entry::make(
                    Suggestion {
                        label: format!("@{name}"),
                        detail: detail.into(),
                        target: Target::Broadcast(broadcast),
                    },
                    [name.into()],
                    2,
                )
                .unwrap(),
            );
        }
        for channel in snapshot
            .channels
            .iter()
            .filter(|c| !c.archived && !c.im && !c.mpim)
        {
            let Some(name) = channel.name.as_ref().filter(|s| !s.is_empty()) else {
                continue;
            };
            if let Some(entry) = Entry::make(
                Suggestion {
                    label: format!("#{name}"),
                    detail: if channel.private || channel.group {
                        "Private channel"
                    } else {
                        "Channel"
                    }
                    .into(),
                    target: Target::Channel(channel.id.clone()),
                },
                [name.clone()],
                0,
            ) {
                channels.push(entry);
            }
        }
        Self { people, channels }
    }

    pub fn suggestions(&self, trigger: &Trigger) -> Vec<Suggestion> {
        let entries = match trigger.kind {
            Kind::Person => &self.people,
            Kind::Channel => &self.channels,
        };
        let query = trigger.query.to_lowercase();
        type Rank<'a> = (bool, bool, u8, &'a str, &'a str);
        let mut best: Vec<(Rank<'_>, &Entry)> = Vec::with_capacity(LIMIT + 1);
        for entry in entries {
            if !query.is_empty() && !entry.names.iter().any(|s| s.contains(&query)) {
                continue;
            }
            let rank = (
                !entry.names.iter().any(|s| s == &query),
                !entry.names.iter().any(|s| s.starts_with(&query)),
                entry.category,
                entry.label.as_str(),
                entry.token.as_str(),
            );
            let index = best.partition_point(|(old, _)| old < &rank);
            if index < LIMIT {
                best.insert(index, (rank, entry));
                best.truncate(LIMIT);
            }
        }
        best.into_iter()
            .map(|(_, entry)| entry.choice.clone())
            .collect()
    }

    /// Allocated UTF-8 string capacities only, not allocator overhead/RSS.
    pub fn string_capacity_bytes(&self) -> usize {
        self.people
            .iter()
            .chain(&self.channels)
            .map(|e| {
                e.choice.label.capacity()
                    + e.choice.detail.capacity()
                    + e.names.iter().map(String::capacity).sum::<usize>()
                    + e.label.capacity()
                    + e.token.capacity()
                    + match &e.choice.target {
                        Target::User(id) | Target::Channel(id) | Target::Group(id) => id.capacity(),
                        Target::Broadcast(_) => 0,
                    }
            })
            .sum()
    }
}

#[cfg(test)]
mod tests;

pub mod cache;
