//! Positively settled replies supplement the current page until read back by the server.
use super::*;
use super_platinum_core::slack::models::Message;
use super_platinum_core::slack_lists::comments::ReplyRecord;

const MAX_ACCEPTED: usize = 128;

pub(super) fn deleted(page: &Page, ts: &str) -> bool {
    page.raw()["deleted"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|v| v.as_str() == Some(ts)))
}

/// A subsequent canonical row (including a hidden/tombstone row) or explicit deletion wins.
pub(super) fn remaining(page: &Page, accepted: &[Message]) -> Vec<Message> {
    accepted
        .iter()
        .filter(|message| {
            message.thread_ts.as_deref() == page.target().root_ts()
                && message.ts.as_deref().is_some_and(|ts| {
                    !deleted(page, ts)
                        && !page
                            .messages()
                            .iter()
                            .any(|row| row.ts.as_deref() == Some(ts))
                })
        })
        .cloned()
        .collect()
}

pub(super) fn rows(
    shell: &ShellState,
    owner: &Owner,
    page: &Page,
    projection: &Projection,
    accepted: &[Message],
) -> Option<Arc<Vec<crate::model::MessageVm>>> {
    let live = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .filter(|entry| entry.owner.serial == owner.serial && &entry.target == page.target())
        .map_or(&[][..], |entry| entry.live.as_slice());
    rows_with_live(shell, owner, page, projection, accepted, live)
}

pub(super) fn rows_with_live(
    shell: &ShellState,
    owner: &Owner,
    page: &Page,
    projection: &Projection,
    accepted: &[Message],
    live: &[Message],
) -> Option<Arc<Vec<crate::model::MessageVm>>> {
    let workspace = shell.core.workspaces.get(&owner.scope.team)?;
    let mut messages = page.messages().to_vec();
    let tail = remaining(page, accepted);
    let socket = remaining(page, live)
        .into_iter()
        .filter(|m| !tail.iter().any(|a| a.ts == m.ts))
        .collect::<Vec<_>>();
    if !tail.is_empty() || !socket.is_empty() {
        messages.extend(tail);
        messages.extend(socket);
        messages
            .sort_by(|a, b| super_platinum_core::state::cmp_ts(a.ts.as_deref(), b.ts.as_deref()));
    }
    let live_deleted = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .filter(|entry| entry.owner.serial == owner.serial && &entry.target == page.target());
    messages.retain(|m| {
        !m.ts.as_deref().is_some_and(|ts| {
            deleted(page, ts) || live_deleted.is_some_and(|entry| entry.deleted.contains(ts))
        })
    });
    Some(Arc::new(
        projection
            .visible(&messages, page.target().root_ts().unwrap_or(""))
            .into_iter()
            .map(|m| crate::message_vm::message_vm_with_pending(workspace, m, &shell.media, false))
            .collect(),
    ))
}

/// This guard deliberately retains the original visible visit across a disk await.
pub(super) fn publish(
    shell: &mut ShellState,
    owner: &Owner,
    record: &ReplyRecord,
    raw: &Value,
) -> bool {
    if !owner.active(shell)
        || record.workspace() != owner.scope.team
        || record.user() != owner.scope.stable_owner().2
    {
        return false;
    }
    let Some(entry) = shell.slack_lists.comments.entry.as_ref() else {
        return false;
    };
    if &entry.target != record.target() {
        return false;
    }
    let (Some(page), Some(projection)) = (&entry.page, &entry.projection) else {
        return false;
    };
    // The journal has already confirmed this terminal. Reuse the domain matcher rather
    // than projecting an arbitrary raw message when a retained terminal is retried.
    let Ok(observed) = Page::parse(
        record.target(),
        serde_json::json!({"ok":true,"messages":[raw]}),
    ) else {
        return false;
    };
    let Ok(Some(mut message)) = record.observed(&observed) else {
        return false;
    };
    let Some(ts) = message.ts.clone() else {
        return false;
    };
    if deleted(page, &ts) || entry.deleted.contains(&ts) {
        return false;
    }
    // Validate the original ACK above before applying later projected writes.
    // Its raw receipt and the durable journal remain the original evidence.
    if let Some(edit) = entry.live_edits.get(&ts) {
        let Some(edited) = edit.apply(&message) else {
            return false;
        };
        message = edited;
    }
    if entry.live_reactions.contains(&ts)
        && let Some(current) = entry.live.iter().find(|m| m.ts.as_ref() == Some(&ts))
    {
        message.reactions = current.reactions.clone();
    }
    let mut accepted = remaining(page, &entry.accepted);
    if !page.messages().iter().any(|m| m.ts.as_deref() == Some(&ts))
        && !accepted.iter().any(|m| m.ts.as_deref() == Some(&ts))
    {
        accepted.push(message);
        if accepted.len() > MAX_ACCEPTED {
            accepted.drain(..accepted.len() - MAX_ACCEPTED);
        }
    }
    let live = entry
        .live
        .iter()
        .filter(|m| m.ts.as_deref() != Some(&ts))
        .cloned()
        .collect::<Vec<_>>();
    let Some(rows) = rows_with_live(shell, owner, page, projection, &accepted, &live) else {
        return false;
    };
    let entry = shell.slack_lists.comments.entry.as_mut().unwrap();
    entry.accepted = accepted;
    entry.live = live;
    entry.live_reactions.remove(&ts);
    entry.live_edits.remove(&ts);
    entry.rows = rows;
    if entry.rows.iter().any(|row| row.ts == ts)
        && entry.reveal.as_ref().is_none_or(|(_, old)| old != &ts)
    {
        entry.reveal = Some((next_id(), ts));
    }
    true
}

#[cfg(test)]
mod tests;
