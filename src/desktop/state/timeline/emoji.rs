//! Event-driven selected emoji presentation. No history reads or draft mutation.
use super::ShellState;

impl ShellState {
    /// Both the sampled shell projection and its committed original DOM must
    /// match. Same row IDs do not make an older glyph/image height authoritative.
    pub(crate) fn channel_projection_receipt_is_current(
        &self,
        sampled: u64,
        dom: Option<&str>,
    ) -> bool {
        self.channel_projection_generation == sampled
            && dom.and_then(|value| value.parse::<u64>().ok()) == Some(sampled)
    }

    /// Called only by accepted assignment publication / socket retirement.
    /// Reacquire the guarded observation at each projection. Missing successor
    /// facts remove the old selected rendering immediately, without IO.
    pub(crate) fn emoji_standard_changed(&mut self, team: &str) {
        if self.core.active_team.as_deref() != Some(team) {
            return;
        }
        if self.channel_window_is_canonical() {
            self.reproject_channel_window();
        }
        let canonical_thread = self
            .core
            .active_channel
            .as_ref()
            .zip(self.thread_root.as_ref())
            .is_some_and(|(channel, root)| {
                self.core
                    .threads
                    .contains_key(&(team.to_owned(), channel.clone(), root.clone()))
            });
        if canonical_thread {
            // sync retains visit/window policy and advances the existing
            // content generation checked by asynchronous thread measurements.
            self.refresh_thread_from_core();
            crate::thread_unread::annotate(self);
        }
    }
}

#[cfg(test)]
mod tests;
