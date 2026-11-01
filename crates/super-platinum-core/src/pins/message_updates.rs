use super::{Message, PinsState};
use crate::{slack::events::MessageUpdate, state::DeferredBody};

#[derive(Debug, Default)]
pub(super) struct PendingMessage {
    pub(super) message: Message,
    body: DeferredBody,
}

// Preserve read-only inspection of the existing private pending message in its
// original tests. File reconciliation mutates the explicit message field only.
impl std::ops::Deref for PendingMessage {
    type Target = Message;
    fn deref(&self) -> &Message {
        &self.message
    }
}

impl PendingMessage {
    fn for_ts(ts: &str) -> Self {
        // The legacy merge never changes ts. Keep this private delta bound to
        // its admitted map key; this does not certify any body's field presence.
        Self {
            message: Message {
                ts: Some(ts.into()),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn update(&mut self, channel: &str, message: &Message, original: Option<&MessageUpdate>) {
        // Pins begin clears this whole pending map. All changes in that one
        // request interval are newer than its snapshot; no extra clock needed.
        self.body.record(channel, message, original, 1);
        self.body
            .merge_into(&mut self.message, channel, message.clone(), 0);
    }
    pub(super) fn apply(&self, existing: &mut Message, channel: &str) {
        self.body
            .merge_into(existing, channel, self.message.clone(), 0);
    }
}

impl PinsState {
    pub fn update_original_message(
        &mut self,
        channel: &str,
        message: &Message,
        original: Option<&MessageUpdate>,
    ) {
        if self.channel.as_deref() != Some(channel) {
            return;
        }
        let Some(ts) = message.ts.as_ref() else {
            return;
        };
        if matches!(self.message_updates.get(ts), Some(None)) {
            return;
        }
        if self.loading || self.items.contains_key(&format!("message:{ts}")) {
            let update = self
                .message_updates
                .entry(ts.clone())
                .or_insert_with(|| Some(PendingMessage::for_ts(ts)));
            if let Some(existing) = update {
                existing.update(channel, message, original);
            }
        }
        if let Some(item) = self.items.get_mut(&format!("message:{ts}")) {
            let bound = item.channel.as_deref().is_none_or(|c| c == channel);
            if let Some(existing) = item.message.as_mut() {
                if bound {
                    crate::state::merge_message_update(
                        existing,
                        channel,
                        message.clone(),
                        original,
                    );
                } else {
                    crate::state::merge_message(existing, message.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
