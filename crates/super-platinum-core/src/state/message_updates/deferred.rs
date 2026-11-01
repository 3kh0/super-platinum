//! At most two original field removals, replayed within an existing owner's bag.
use super::original_edit_matches;
use crate::slack::{events::MessageUpdate, models::Message};

#[derive(Debug, Clone)]
struct Removal {
    channel: String,
    ts: String,
    revision: u64,
}

impl Removal {
    fn matches(&self, message: &Message, channel: &str, requested: u64) -> bool {
        self.revision > requested
            && self.channel == channel
            && message.ts.as_deref() == Some(self.ts.as_str())
            && message.channel.as_deref().is_none_or(|c| c == channel)
    }
}

/// Presence data only. Existing membership/account/generation/deletion checks
/// still admit the event and its later snapshot. No event log or read authority.
#[derive(Debug, Clone, Default)]
pub(crate) struct DeferredBody {
    blocks: Option<Removal>,
    attachments: Option<Removal>,
}

impl DeferredBody {
    pub(crate) fn record(
        &mut self,
        channel: &str,
        message: &Message,
        original: Option<&MessageUpdate>,
        revision: u64,
    ) {
        // Here the message is only the original incoming identity. merge_into
        // separately checks the stored preimage before doing the legacy merge.
        let original = original.filter(|o| original_edit_matches(message, channel, message, o));
        for (slot, populated, field) in [
            (&mut self.blocks, !message.blocks.is_empty(), "blocks"),
            (
                &mut self.attachments,
                !message.attachments.is_empty(),
                "attachments",
            ),
        ] {
            if populated {
                // A newer real value replaces an earlier removal. An omitted,
                // null or synthetic empty vector must neither renew nor undo it.
                *slot = None;
            } else if original
                .and_then(|o| o.raw().get(field))
                .and_then(serde_json::Value::as_array)
                .is_some_and(Vec::is_empty)
            {
                *slot = Some(Removal {
                    channel: channel.into(),
                    ts: message.ts.clone().expect("original nonempty identity"),
                    revision,
                });
            }
        }
    }

    pub(crate) fn merge_into(
        &self,
        existing: &mut Message,
        channel: &str,
        update: Message,
        requested_revision: u64,
    ) {
        // Check BEFORE merge_message can change the stored channel or timestamp.
        // Also bind the aggregate's identity: no reuse with another message.
        let identity =
            update.ts == existing.ts && update.channel.as_deref().is_none_or(|c| c == channel);
        let blocks = identity
            && self
                .blocks
                .as_ref()
                .is_some_and(|r| r.matches(existing, channel, requested_revision));
        let attachments = identity
            && self
                .attachments
                .as_ref()
                .is_some_and(|r| r.matches(existing, channel, requested_revision));
        crate::state::merge_message(existing, update);
        if blocks {
            existing.blocks.clear();
        }
        if attachments {
            existing.attachments.clear();
        }
    }
}
