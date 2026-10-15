// Frozen actual legacy suggestions body; do not optimize this oracle.
use crate::{
    composer_completion::{Broadcast, Kind, LIMIT, Suggestion, Target, Trigger},
    state::Workspace,
};

pub fn suggestions(workspace: &Workspace, trigger: &Trigger) -> Vec<Suggestion> {
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
        Kind::Person => {
            for user in workspace.users.values().filter(|u| !u.deleted) {
                let name = workspace.display_name(&user.id);
                let handle = user.name.as_deref().unwrap_or_default();
                let real = user
                    .real_name
                    .as_deref()
                    .or_else(|| user.profile.as_ref().and_then(|p| p.real_name.as_deref()))
                    .unwrap_or_default();
                add(
                    format!("@{name}"),
                    if handle.is_empty() {
                        real.into()
                    } else {
                        format!("@{handle}")
                    },
                    Target::User(user.id.clone()),
                    &[&name, handle, real],
                    0,
                );
            }
            for group in workspace.usergroups.values().filter(|g| g.date_delete == 0) {
                let handle = group.handle.as_str();
                let name = group.name.as_str();
                if !handle.is_empty() {
                    add(
                        format!("@{handle}"),
                        name.into(),
                        Target::Group(group.id.clone()),
                        &[handle, name],
                        1,
                    );
                }
            }
            // Explicit selections only; plain typed text is never upgraded to
            // a broadcast merely because it happens to spell @channel.
            for (name, kind, detail) in [
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
                add(
                    format!("@{name}"),
                    detail.into(),
                    Target::Broadcast(kind),
                    &[name],
                    2,
                );
            }
        }
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
