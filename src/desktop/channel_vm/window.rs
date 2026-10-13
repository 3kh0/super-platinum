//! Full-history geometry with rich projection limited to the selected window.
use crate::{media::MediaRegistry, model::MessageVm};
use std::ops::Range;
use super_platinum_core::{slack::models::Message, state::Workspace};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChannelRow {
    pub id: String,
    pub ts: String,
    pub pending: bool,
    pub source_index: usize,
    pub compact: bool,
    pub date_label: Option<String>,
    pub show_unread_divider: bool,
}

/// Must be rebuilt with the current source before projecting: source positions
/// and predecessor/unread annotations belong to this exact history snapshot.
pub(crate) fn channel_rows(
    workspace: &Workspace,
    channel_id: &str,
    divider_at: Option<&str>,
) -> Vec<ChannelRow> {
    use super_platinum_core::state::{
        date_key_for_ts, format_ts_date_label, is_channel_timeline_visible, same_message_group,
        ts_key,
    };
    let Some(bag) = workspace.messages.get(channel_id) else {
        return Vec::new();
    };
    let last_key = divider_at.or(bag.last_read.as_deref()).map(ts_key);
    let mut unread_found = false;
    let mut last_date = None;
    let mut previous: Option<&Message> = None;
    bag.messages
        .iter()
        .enumerate()
        .filter_map(|(source_index, message)| {
            if !is_channel_timeline_visible(message) {
                return None;
            }
            let id = crate::message_vm::message_id(message);
            let ts = id.clone();
            let mut date_label = None;
            if !ts.is_empty()
                && let Some(date) = date_key_for_ts(&ts)
            {
                if last_date.as_ref() != Some(&date) {
                    date_label = Some(format_ts_date_label(&ts));
                    last_date = Some(date);
                }
            }
            let show_unread_divider =
                !unread_found && !ts.is_empty() && last_key.is_some_and(|key| ts_key(&ts) > key);
            unread_found |= show_unread_divider;
            let compact = !show_unread_divider
                && date_label.is_none()
                && previous.is_some_and(|previous| same_message_group(previous, message));
            previous = Some(message);
            Some(ChannelRow {
                id,
                ts,
                pending: message.ts.as_deref().is_some_and(|ts| bag.is_pending(ts)),
                source_index,
                compact,
                date_label,
                show_unread_divider,
            })
        })
        .collect()
}

/// `range` uses absolute visible-history indices. Out-of-range bounds are
/// clamped. Any stale source position rejects the whole window, preserving the
/// caller's absolute start + offset indexing instead of compacting missing rows.
pub(crate) fn project_channel_window(
    workspace: &Workspace,
    channel_id: &str,
    media: &MediaRegistry,
    rows: &[ChannelRow],
    range: Range<usize>,
) -> Vec<MessageVm> {
    let Some(bag) = workspace.messages.get(channel_id) else {
        return Vec::new();
    };
    let start = range.start.min(rows.len());
    let end = range.end.min(rows.len()).max(start);
    rows[start..end]
        .iter()
        .map(|row| {
            let message = bag.messages.get(row.source_index)?;
            if !super_platinum_core::state::is_channel_timeline_visible(message)
                || crate::message_vm::message_id(message) != row.id
            {
                return None;
            }
            let mut vm =
                crate::message_vm::message_vm_with_pending(workspace, message, media, row.pending);
            vm.broadcast_parent_preview = vm.broadcast_root.as_deref().and_then(|root| {
                crate::message_vm::broadcast::preview(workspace, message, root, channel_id, media)
            });
            vm.compact = row.compact;
            vm.date_label = row.date_label.clone();
            vm.show_unread_divider = row.show_unread_divider;
            Some(vm)
        })
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
