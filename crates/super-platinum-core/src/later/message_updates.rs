use super::{LaterState, Message, PreviewUpdate, SavedKey};
use crate::slack::events::MessageUpdate;

impl LaterState {
    /// Same owned saved-message path, with optional original body presence.
    pub fn update_original_message(
        &mut self,
        channel: &str,
        message: &Message,
        original: Option<&MessageUpdate>,
    ) {
        let Some(ts) = message.ts.as_ref() else {
            return;
        };
        let key = SavedKey::message(channel, ts);
        if !self.items.contains_key(&key) && !self.hydrated.contains_key(&key) {
            return;
        }
        if self
            .preview_updates
            .get(&key)
            .is_some_and(|update| update.message.is_none())
        {
            return;
        }
        let mut message = message.clone();
        crate::file_actions::visit_message(&mut message, &mut |file| {
            if let Some(id) = file.id.as_deref()
                && self.deleted_files.contains(id)
            {
                *file = crate::file_actions::tombstone(id);
            }
        });
        let revision = self.next_hydration_revision();
        let pending = self
            .preview_updates
            .entry(key.clone())
            .or_insert_with(|| PreviewUpdate {
                // merge_message intentionally never changes message identity.
                // Seed this private delta with its already-owned map key.
                message: Some(Message {
                    ts: Some(key.ts.clone()),
                    ..Default::default()
                }),
                ..Default::default()
            });
        pending.revision = revision;
        pending.record_fields(&message, revision);
        pending.body.record(channel, &message, original, revision);
        if let Some(update) = &mut pending.message {
            pending.body.merge_into(update, channel, message.clone(), 0);
        }
        if let Some(existing) = self.hydrated.get_mut(&key) {
            crate::state::merge_message_update(existing, channel, message, original);
        }
    }
}

#[cfg(test)]
mod tests;
