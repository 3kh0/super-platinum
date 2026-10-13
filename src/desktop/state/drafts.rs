use super::{ComposerTarget, ShellState};
use super_platinum_core::drafts::DraftKey;

impl ShellState {
    pub(crate) fn composer_key(&self, target: ComposerTarget) -> Option<DraftKey> {
        Some(DraftKey {
            team: self.core.active_team.clone()?,
            channel: self.core.active_channel.clone()?,
            thread: match target {
                ComposerTarget::Channel => None,
                ComposerTarget::Thread => Some(self.thread_root.clone()?),
            },
        })
    }
    /// Bind the live fields to their destination before the next render. The
    /// outgoing fields are captured using their previous binding, even when
    /// navigation has already changed active_channel.
    pub(crate) fn sync_composer_drafts(&mut self) {
        let channel = self.composer_key(ComposerTarget::Channel);
        let thread = self.composer_key(ComposerTarget::Thread);
        let core = &mut self.core;
        // Offline fixtures may start with composed text before their first bind.
        if core.drafts.channel.is_none()
            && let Some(key) = &channel
            && !core.drafts.items.contains_key(key)
        {
            core.drafts
                .capture(key, &core.composer, &core.composer_attachments);
        }
        if core.drafts.thread.is_none()
            && let Some(key) = &thread
            && !core.drafts.items.contains_key(key)
        {
            core.drafts.capture(
                key,
                &core.thread_composer,
                &core.thread_composer_attachments,
            );
        }
        core.drafts.bind(
            channel,
            false,
            &mut core.composer,
            &mut core.composer_attachments,
        );
        core.drafts.bind(
            thread,
            true,
            &mut core.thread_composer,
            &mut core.thread_composer_attachments,
        );
        self.persist_draft_changes();
    }
    pub(crate) fn checkpoint_composers(&mut self) {
        self.sync_composer_drafts();
        let core = &mut self.core;
        if let Some(key) = core.drafts.channel.clone() {
            core.drafts
                .capture(&key, &core.composer, &core.composer_attachments);
        }
        if let Some(key) = core.drafts.thread.clone() {
            core.drafts.capture(
                &key,
                &core.thread_composer,
                &core.thread_composer_attachments,
            );
        }
        self.persist_draft_changes();
    }
    fn persist_draft_changes(&mut self) {
        let changes = self.core.drafts.take_changes();
        crate::drafts::integration::route(self, changes);
    }
}

impl ShellState {
    pub(crate) fn discard_local_draft(&mut self, key: &DraftKey) {
        self.checkpoint_composers();
        let target = if key.thread.is_some() {
            ComposerTarget::Thread
        } else {
            ComposerTarget::Channel
        };
        if self.composer_key(target).as_ref() == Some(key) {
            match target {
                ComposerTarget::Channel => {
                    self.core.composer = Default::default();
                    self.core.composer_attachments.clear();
                }
                ComposerTarget::Thread => {
                    self.core.thread_composer = Default::default();
                    self.core.thread_composer_attachments.clear();
                }
            }
        }
        self.core.drafts.capture(key, &Default::default(), &[]);
        self.checkpoint_composers();
    }
}
