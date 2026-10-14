//! Restore only a definitely rejected send; keep typing during disk I/O separate.
use super::*;
use super_platinum_core::drafts::sync::ConsumeIntent;
impl Editor {
    pub fn waiting_for_return(&self) -> bool {
        self.state.lock().unwrap().return_handoff.is_some()
    }

    /// A later explicit local-edit action must revalidate the durable restored
    /// row before re-arming a handoff interrupted by navigation or unmount.
    pub async fn review_return(&self, expected: u64) -> Result<(u64, View), Fault> {
        let _gate = self.gate.lock().await;
        let (serial, view) = {
            let state = self.state.lock().unwrap();
            let (serial, revision) = state.return_handoff.ok_or(Fault::Stale)?;
            if revision != expected
                || state.serial != serial
                || state.running
                || state.settling
                || state.projection.is_some()
                || state.pending.is_some()
                || state.error != Some(Fault::AlternateEdits)
            {
                return Err(Fault::Stale);
            }
            (serial, state.base.clone())
        };
        let review = self
            .owner
            .account
            .backend
            .read_local_review(self.owner.scope.clone())
            .await?;
        if review.stored.as_ref().map(|row| row.revision) != Some(expected)
            || !same_local(&review.local, &view.local)
            || review.blocked.is_some()
            || !review.alternates.is_empty()
        {
            return Err(Fault::Stale);
        }
        Ok((serial, view))
    }

    /// Only the UI handoff may resume ordinary edits after it has projected the
    /// restored content. Until then, even input after the disk receipt belongs
    /// to a separate version of the previously empty composer.
    pub fn finish_return(&self, serial: u64) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.return_handoff.map(|(id, _)| id) != Some(serial)
            || state.serial != serial
            || state.running
            || state.settling
            || state.projection.is_some()
            || state.pending.is_some()
            || state.error != Some(Fault::AlternateEdits)
        {
            return false;
        }
        state.error = None;
        state.return_handoff = None;
        true
    }
    pub async fn return_rejected(
        self: &Arc<Self>,
        expected: u64,
        intent: ConsumeIntent,
        serial: u64,
    ) -> Result<View, Fault> {
        let editor = self.clone();
        self.runtime
            .spawn(async move { editor.return_rejected_inner(expected, intent, serial).await })
            .await
            .map_err(|_| Fault::Storage)?
    }
    async fn return_rejected_inner(
        &self,
        expected: u64,
        intent: ConsumeIntent,
        serial: u64,
    ) -> Result<View, Fault> {
        let _gate = self.gate.lock().await;
        {
            let mut state = self.state.lock().unwrap();
            if state.serial != serial
                || state.running
                || state.settling
                || state.projection.is_some()
                || state.pending.is_some()
                || !state.local_writes.is_empty()
            {
                return Err(Fault::Stale);
            }
            state.settling = true;
        }
        let result = self.owner.return_rejected(expected, &intent).await;
        {
            let mut state = self.state.lock().unwrap();
            state.settling = false;
            if let Ok(view) = &result {
                state.base = view.clone();
                state.return_handoff = view.stored.as_ref().map(|row| (serial, row.revision));
                state.direct_local = None;
                // A later keystroke belongs to another version, not the restored frozen
                // message. Park it so the waiting runner saves an alternate durably.
                state.error = Some(Fault::AlternateEdits);
            }
        }
        self.changed.notify_waiters();
        result
    }
}
#[cfg(test)]
#[path = "checkpoint_return_tests.rs"]
mod tests;
