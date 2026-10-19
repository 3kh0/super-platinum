//! Original changed-message evidence updates only the current loaded reply.
use super::*;
use super_platinum_core::{
    slack::{
        events::{MessageUpdateKind, RtEvent},
        models::Message,
    },
    slack_lists::comments::{ReplyEdit, ReplyEditError},
};

const NOTICE: &str =
    "Newer comment updates arrived. Loaded comments are retained. Refresh to try again.";

pub(crate) fn apply(shell: &mut ShellState, team: &str, generation: u64, event: &RtEvent) -> bool {
    let RtEvent::MessageChanged {
        channel,
        message,
        original_update: Some(update),
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
        || channel != entry.target.channel_id()
        || update.kind() != MessageUpdateKind::Changed
        || !update.matches_identity(channel, message)
    {
        return false;
    }
    let (Some(page), Some(projection), Some(ts)) = (&entry.page, &entry.projection, &message.ts)
    else {
        return false;
    };
    if entry.target.root_ts() == Some(ts.as_str())
        || entry.deleted.contains(ts)
        || accepted::deleted(page, ts)
        || !entry.rows.iter().any(|row| row.ts == *ts)
    {
        return false;
    }
    let Some(known) = page
        .messages()
        .iter()
        .chain(&entry.accepted)
        .chain(&entry.live)
        .find(|m| m.ts.as_ref() == Some(ts))
    else {
        return false;
    };
    let edit = match ReplyEdit::project(&entry.target, known, update) {
        Ok(edit) => edit,
        Err(ReplyEditError::Refused) => return false,
        Err(ReplyEditError::Unsupported) => {
            // The row is known, but this bounded projector cannot safely apply it.
            // Retain every field and require an explicit canonical refresh.
            shell.slack_lists.comments.entry.as_mut().unwrap().error = Some(NOTICE.into());
            return false;
        }
    };
    let mut next_page = page.clone();
    let mut accepted = entry.accepted.clone();
    let mut live = entry.live.clone();
    let mut changed = next_page.apply_reply_edit(&edit);
    for rows in [&mut accepted, &mut live] {
        for row in rows.iter_mut().filter(|m| m.ts.as_ref() == Some(ts)) {
            let Some(next) = edit.apply(row) else {
                return false;
            };
            changed |= !same(row, &next);
            *row = next;
        }
    }
    let custody = if entry.live.iter().any(|m| m.ts.as_ref() == Some(ts)) {
        match entry.live_edits.get(ts) {
            Some(previous) => match previous.union(&edit) {
                Some(composed) => Some(composed),
                None => return false,
            },
            None => Some(edit),
        }
    } else {
        None
    };
    let rows = if changed {
        let Some(rows) = accepted::rows_with_live(
            shell,
            &entry.owner,
            &next_page,
            projection,
            &accepted,
            &live,
        ) else {
            return false;
        };
        Some(rows)
    } else {
        None
    };
    let entry = shell.slack_lists.comments.entry.as_mut().unwrap();
    // Equal-current writes still protect a live reply from an older differing ACK.
    // They do not replace the projected rows or retire a read.
    if let Some(custody) = custody {
        entry.live_edits.insert(ts.clone(), custody);
    }
    if let Some(rows) = rows {
        entry.page = Some(next_page);
        entry.accepted = accepted;
        entry.live = live;
        entry.rows = rows;
        retained(entry);
    }
    changed
}

fn same(a: &Message, b: &Message) -> bool {
    serde_json::to_value(a).ok() == serde_json::to_value(b).ok()
}
fn retained(entry: &mut Entry) {
    if entry.loading && entry.read_replaces_page {
        entry.read = next_id();
        entry.loading = false;
        entry.error = Some(NOTICE.into());
    }
}

#[cfg(test)]
mod tests;
