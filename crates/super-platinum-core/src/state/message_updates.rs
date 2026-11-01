//! Presence-aware removal for original socket edits in already-owned messages.
//! The parser evidence is data only; callers still own the channel/bag/event.
use super::{ChannelMessages, SlackMessage, merge_message};
use crate::slack::events::{MessageUpdate, MessageUpdateKind};

pub(crate) fn original_edit_matches(
    existing: &SlackMessage,
    channel: &str,
    update: &SlackMessage,
    original: &MessageUpdate,
) -> bool {
    let Some(ts) = update.ts.as_deref().filter(|ts| !ts.is_empty()) else {
        return false;
    };
    original.kind() == MessageUpdateKind::Changed
        && !channel.is_empty()
        && original.matches_identity(channel, update)
        && update.channel.as_deref() == Some(channel)
        && existing.ts.as_deref() == Some(ts)
        && existing
            .channel
            .as_deref()
            .is_none_or(|stored| stored == channel)
        && original.raw().get("ts").and_then(serde_json::Value::as_str) == Some(ts)
        && original
            .raw()
            .get("channel")
            .is_none_or(|value| value.as_str() == Some(channel))
}

pub(crate) fn merge(
    existing: &mut SlackMessage,
    channel: &str,
    update: SlackMessage,
    original: Option<&MessageUpdate>,
) {
    let original =
        original.filter(|original| original_edit_matches(existing, channel, &update, original));
    let empty = |field| {
        original
            .and_then(|o| o.raw().get(field))
            .and_then(serde_json::Value::as_array)
            .is_some_and(Vec::is_empty)
    };
    // Neither typed defaults, omission, null, synthetic legacy events nor a
    // message_replied parent may certify removal. Keep all legacy merging.
    let blocks = update.blocks.is_empty() && empty("blocks");
    let attachments = update.attachments.is_empty() && empty("attachments");
    merge_message(existing, update);
    if blocks {
        existing.blocks.clear();
    }
    if attachments {
        existing.attachments.clear();
    }
}

impl ChannelMessages {
    /// Ordinary merge plus originally explicit body removals. The native caller
    /// must retain its original account/socket/channel ownership. This creates
    /// no additional insertion path, history coverage or permission receipt.
    pub fn merge_original_update(
        &mut self,
        channel: &str,
        update: SlackMessage,
        original: Option<&MessageUpdate>,
    ) -> bool {
        if let Some(index) = update.ts.as_deref().and_then(|ts| self.index_of(ts)) {
            merge(&mut self.messages[index], channel, update, original);
            false
        } else {
            self.merge_update(update)
        }
    }
}

mod deferred;
pub(crate) use deferred::DeferredBody;
mod preview;
pub(crate) use preview::PreviewUpdate;

#[cfg(test)]
mod tests;
