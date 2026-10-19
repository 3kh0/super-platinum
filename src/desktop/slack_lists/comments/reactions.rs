//! Socket deltas update the visible reader without replacing its mounted editor.
use super::*;
use super_platinum_core::{slack::events::RtEvent, state::ChannelMessages};

pub(crate) fn apply(shell: &mut ShellState, team: &str, generation: u64, event: &RtEvent) -> bool {
    let (channel, ts, user, name, added) = match event {
        RtEvent::ReactionAdded {
            channel,
            ts,
            user,
            reaction,
        } => (channel, ts, user, reaction, true),
        RtEvent::ReactionRemoved {
            channel,
            ts,
            user,
            reaction,
        } => (channel, ts, user, reaction, false),
        _ => return false,
    };
    let Some(entry) = shell.slack_lists.comments.entry.as_ref() else {
        return false;
    };
    if entry.owner.scope.team != team || !entry.owner.active(shell)
        || shell.core.workspaces.get(team).is_none_or(|w| w.rt_generation != generation)
        || entry.target.channel_id() != channel
        || user.is_empty() || name.is_empty()
        // Use the actual observed projection, including hidden/deleted rows.
        || !entry.rows.iter().any(|row| row.ts == *ts)
    {
        return false;
    }
    let (Some(mut page), Some(projection)) = (entry.page.clone(), entry.projection.as_ref()) else {
        return false;
    };
    let mut tail = ChannelMessages {
        messages: entry.accepted.clone(),
        ..Default::default()
    };
    let mut live = ChannelMessages {
        messages: entry.live.clone(),
        ..Default::default()
    };
    let changed = page.apply_reaction(channel, ts, user, name, added);
    let tail_changed = tail.apply_reaction(ts, user, name, added);
    let live_changed = live.apply_reaction(ts, user, name, added);
    if (!changed && !tail_changed && !live_changed)
        || (same_reactions(entry.page.as_ref().unwrap().messages(), page.messages(), ts)
            && same_reactions(&entry.accepted, &tail.messages, ts)
            && same_reactions(&entry.live, &live.messages, ts))
    {
        return false;
    }
    let Some(rows) = accepted::rows_with_live(
        shell,
        &entry.owner,
        &page,
        projection,
        &tail.messages,
        &live.messages,
    ) else {
        return false;
    };
    let live_reacted = !same_reactions(&entry.live, &live.messages, ts);
    let entry = shell.slack_lists.comments.entry.as_mut().unwrap();
    entry.page = Some(page);
    entry.accepted = tail.messages;
    entry.live = live.messages;
    if live_reacted {
        entry.live_reactions.insert(ts.clone());
    }
    entry.rows = rows;
    // A response already in flight predates this live delta. Retire its original
    // read authority so request cancellation and every after-await guard reject it,
    // including the later reply-observation path. A fresh explicit read is allowed.
    if entry.loading && entry.read_replaces_page {
        entry.read = next_id();
        entry.loading = false;
        entry.error = Some(
            "Newer comment updates arrived. Loaded comments are retained. Refresh to try again."
                .into(),
        );
    }
    // No reveal marker, pagination, draft or editor generation changes.
    true
}

fn same_reactions(
    before: &[super_platinum_core::slack::models::Message],
    after: &[super_platinum_core::slack::models::Message],
    ts: &str,
) -> bool {
    match (
        before.iter().find(|m| m.ts.as_deref() == Some(ts)),
        after.iter().find(|m| m.ts.as_deref() == Some(ts)),
    ) {
        (Some(a), Some(b)) => a
            .reactions
            .iter()
            .map(|r| (&r.name, &r.users, r.count, &r.extra))
            .eq(b
                .reactions
                .iter()
                .map(|r| (&r.name, &r.users, r.count, &r.extra))),
        (None, None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests;
