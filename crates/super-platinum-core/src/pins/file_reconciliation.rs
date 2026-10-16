//! File-only reconciliation at admitted pin snapshot boundaries.
use super::{PinUpdate, PinnedItem, PinsPage, PinsState};
use crate::{
    CoreAppState,
    file_actions::{FileDisplayRead, tombstone, visit_message},
    slack::models::{File, Message},
};

fn visit_item(item: &mut PinnedItem, update: &mut impl FnMut(&mut File)) {
    if let Some(file) = &mut item.file {
        update(file);
    }
    if let Some(message) = &mut item.message {
        visit_message(message, update);
    }
}

impl PinsState {
    /// Pending message files have the same accepted field order as visible copies.
    pub(crate) fn visit_files(&mut self, update: &mut impl FnMut(&mut File)) {
        for item in self.items.values_mut() {
            visit_item(item, update);
        }
        for message in self.message_updates.values_mut().filter_map(Option::as_mut) {
            visit_message(message, update);
        }
    }

    fn visit_file_copies(&mut self, key: &str, update: &mut impl FnMut(&mut File)) {
        if let Some(item) = self.items.get_mut(key) {
            visit_item(item, update);
        }
        if let Some(ts) = key.strip_prefix("message:")
            && let Some(Some(message)) = self.message_updates.get_mut(ts)
        {
            visit_message(message, update);
        }
    }
}

impl CoreAppState {
    /// The adapter must retain its transport/account checks before calling this.
    /// False means stale pins ownership/generation; an error refuses the page.
    pub fn apply_pin_page_since(
        &mut self,
        team: &str,
        generation: u64,
        mut page: PinsPage,
        read: FileDisplayRead,
    ) -> Result<bool, &'static str> {
        if !self.pins.matches_scope(self.account_epoch, team) || self.pins.generation != generation
        {
            return Ok(false);
        }
        let files = self.finish_file_display_read(team, read)?;
        if !files.is_empty() {
            for item in &mut page.items {
                visit_item(item, &mut |file| files.apply_file(file));
            }
        }
        // Preserve PinsState's existing message/reaction precedence. Earlier
        // pending file copies received later display patches through visit_files.
        let applied = self.pins.apply(generation, page);
        if applied {
            self.mask_pin_file_deletions(team, None);
        }
        Ok(applied)
    }

    /// Pin event timestamps keep their existing membership-only authority.
    pub fn apply_pin_event(&mut self, team: &str, update: &PinUpdate) -> bool {
        if !self.pins.matches_scope(self.account_epoch, team) || !self.pins.apply_event(update) {
            return false;
        }
        if update.pinned && self.pins.channel.as_deref() == Some(&update.channel) {
            if let Some(key) = update.item.key() {
                self.mask_pin_file_deletions(team, Some(&key));
            }
        }
        true
    }

    /// Local pin acknowledgments retain their existing admission/version guards.
    /// This masks known deletions; it does not give the captured display snapshot
    /// authority over later metadata or add a local-ack display freshness policy.
    pub fn set_pin_item(&mut self, team: &str, channel: &str, item: PinnedItem, pinned: bool) {
        if !self.pins.matches_scope(self.account_epoch, team) {
            return;
        }
        let key = item.key();
        self.pins.set(channel, item, pinned);
        if pinned && self.pins.channel.as_deref() == Some(channel) {
            if let Some(key) = key {
                self.mask_pin_file_deletions(team, Some(&key));
            }
        }
    }

    /// Mask both the visible copy and the pending copy after the normal merge.
    pub fn update_pinned_message(&mut self, team: &str, channel: &str, message: &Message) {
        if !self.pins.matches_scope(self.account_epoch, team)
            || self.pins.channel.as_deref() != Some(channel)
        {
            return;
        }
        self.pins.update_message(channel, message);
        if let Some(ts) = &message.ts {
            self.mask_pin_file_deletions(team, Some(&format!("message:{ts}")));
        }
    }

    fn mask_pin_file_deletions(&mut self, team: &str, key: Option<&str>) {
        let Some(workspace) = self.workspaces.get(team) else {
            return;
        };
        if workspace.deleted_files.is_empty() {
            return;
        }
        let mut mask = |file: &mut File| {
            if let Some(id) = file.id.as_deref()
                && workspace.deleted_files.contains(id)
            {
                *file = tombstone(id);
            }
        };
        if let Some(key) = key {
            self.pins.visit_file_copies(key, &mut mask);
        } else {
            self.pins.visit_files(&mut mask);
        }
    }
}

#[cfg(test)]
mod tests;
