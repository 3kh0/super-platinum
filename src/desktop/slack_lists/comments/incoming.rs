//! Socket arrivals supplement the acquired page without claiming send settlement.
use super::*;
use super_platinum_core::slack::events::RtEvent;

const MAX_LIVE: usize = 128;

pub(crate) fn apply(shell: &mut ShellState, team: &str, generation: u64, event: &RtEvent) -> bool {
    let RtEvent::Message(message) = event else {
        return false;
    };
    let Some(entry) = shell.slack_lists.comments.entry.as_ref() else {
        return false;
    };
    if entry.owner.scope.team != team
        || !entry.owner.active(shell)
        || shell
            .core
            .workspaces
            .get(team)
            .is_none_or(|w| w.rt_generation != generation)
    {
        return false;
    }
    let (Some(page), Some(projection), Some(ts)) = (&entry.page, &entry.projection, &message.ts)
    else {
        return false;
    };
    if !page.accepts_live_reply(message)
        || entry.deleted.contains(ts)
        || page
            .messages()
            .iter()
            .chain(&entry.accepted)
            .chain(&entry.live)
            .any(|m| m.ts.as_ref() == Some(ts))
        || projection
            .visible(
                std::slice::from_ref(message),
                entry.target.root_ts().unwrap_or(""),
            )
            .is_empty()
    {
        return false;
    }
    if entry.live.len() >= MAX_LIVE {
        // Keep every currently visible reply. A later explicit canonical read can
        // retire this bounded overlay; do not silently evict rows or start HTTP.
        let entry = shell.slack_lists.comments.entry.as_mut().unwrap();
        if entry.loading && entry.read_replaces_page {
            entry.read = next_id();
            entry.loading = false;
        }
        entry.error = Some("More comments arrived. Refresh to load the latest comments.".into());
        return false;
    }
    let mut live = entry.live.clone();
    live.push(message.clone());
    let Some(rows) = accepted::rows_with_live(
        shell,
        &entry.owner,
        page,
        projection,
        &entry.accepted,
        &live,
    ) else {
        return false;
    };
    let entry = shell.slack_lists.comments.entry.as_mut().unwrap();
    entry.live = live;
    entry.rows = rows;
    if entry.loading && entry.read_replaces_page {
        entry.read = next_id();
        entry.loading = false;
        entry.error = Some(
            "Newer comment updates arrived. Loaded comments are retained. Refresh to try again."
                .into(),
        );
    }
    // No reveal, navigation, journal, editor or local send authority changes.
    true
}

#[cfg(test)]
mod tests;
