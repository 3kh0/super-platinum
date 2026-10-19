//! Remove known replies without replacing the mounted composer or trusting older reads.
use super::*;
use super_platinum_core::slack::events::RtEvent;

pub(crate) fn apply(shell: &mut ShellState, team: &str, generation: u64, event: &RtEvent) -> bool {
    let RtEvent::MessageDeleted {
        channel,
        deleted_ts: ts,
    } = event
    else {
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
        || entry.target.channel_id() != channel
        || ts.is_empty()
        || entry.target.root_ts() == Some(ts.as_str())
        || !entry.rows.iter().any(|row| row.ts == *ts)
    {
        return false;
    }
    let (Some(mut page), Some(projection)) = (entry.page.clone(), entry.projection.as_ref()) else {
        return false;
    };
    let mut accepted = entry.accepted.clone();
    let mut live = entry.live.clone();
    let changed = page.remove_reply(channel, ts);
    accepted.retain(|message| message.ts.as_deref() != Some(ts.as_str()));
    live.retain(|message| message.ts.as_deref() != Some(ts.as_str()));
    if !changed && accepted.len() == entry.accepted.len() && live.len() == entry.live.len() {
        return false;
    }
    let Some(rows) =
        accepted::rows_with_live(shell, &entry.owner, &page, projection, &accepted, &live)
    else {
        return false;
    };
    let entry = shell.slack_lists.comments.entry.as_mut().unwrap();
    entry.page = Some(page);
    entry.accepted = accepted;
    entry.live = live;
    entry.live_reactions.remove(ts);
    entry.live_edits.remove(ts);
    entry.rows = rows;
    entry.deleted.insert(ts.clone());
    if entry
        .reveal
        .as_ref()
        .is_some_and(|(_, target)| target == ts)
    {
        entry.reveal = None;
    }
    // Only page-publishing reads are older than this delta. Send preflight may
    // continue, and any later accepted publication checks the visit tombstones.
    if entry.loading && entry.read_replaces_page {
        entry.read = next_id();
        entry.loading = false;
        entry.error = Some(
            "Newer comment updates arrived. Loaded comments are retained. Refresh to try again."
                .into(),
        );
    }
    true
}

#[cfg(test)]
mod tests;
