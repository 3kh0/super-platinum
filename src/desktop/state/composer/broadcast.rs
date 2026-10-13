//! The choice belongs to one draft destination and one mounted field scope.
use super::*;

impl ShellState {
    pub(crate) fn thread_broadcast_label(&self) -> String {
        let channel = self
            .core
            .active_team
            .as_ref()
            .and_then(|team| self.core.workspaces.get(team))
            .and_then(|workspace| {
                self.core
                    .active_channel
                    .as_ref()
                    .and_then(|id| workspace.channels.get(id))
            });
        match channel {
            Some(channel) if channel.is_mpim => "Also send to the group".into(),
            Some(channel) if channel.is_im => "Also send as direct message".into(),
            Some(channel) => format!(
                "Also send to {}",
                channel.name.as_deref().unwrap_or("channel")
            ),
            None => "Also send to channel".into(),
        }
    }
    pub(crate) fn thread_broadcast_disabled(&self) -> bool {
        self.composer_unavailable().is_some()
            || self
                .composer_key(ComposerTarget::Thread)
                .is_none_or(|key| self.draft_writes.sending.contains(&key))
            || self
                .core
                .thread_composer_attachments
                .iter()
                .any(|attachment| attachment.uploading)
    }
    pub(crate) fn set_thread_broadcast(
        &mut self,
        scope: &str,
        generation: u64,
        checked: bool,
    ) -> bool {
        if self.thread_generation != generation
            || self.thread_broadcast_disabled()
            || crate::view::composer::field_scope(self, ComposerTarget::Thread) != scope
        {
            return false;
        }
        self.core.thread_composer.reply_broadcast = checked;
        self.checkpoint_composers();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queued_toggle_cannot_change_a_new_thread_or_account() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.core.active_account = None;
        shell.thread_root = Some("100.000001".into());
        shell.sync_composer_drafts();
        let scope = crate::view::composer::field_scope(&shell, ComposerTarget::Thread);
        let generation = shell.thread_generation;
        assert!(shell.set_thread_broadcast(&scope, generation, true));
        let key = shell.composer_key(ComposerTarget::Thread).unwrap();
        assert!(shell.core.drafts.items[&key].composer.reply_broadcast);
        shell.thread_root = Some("100.000002".into());
        shell.sync_composer_drafts();
        assert!(!shell.set_thread_broadcast(&scope, generation, true));
        assert!(!shell.core.thread_composer.reply_broadcast);
        shell.thread_root = key.thread.clone();
        shell.sync_composer_drafts();
        assert!(shell.core.thread_composer.reply_broadcast);
        shell.thread_generation += 1;
        assert!(!shell.set_thread_broadcast(&scope, generation, false));
        shell.core.account_epoch += 1;
        assert!(!shell.set_thread_broadcast(&scope, generation, false));
    }
}
