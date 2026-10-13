//! Explicit recovery controls; none of these dispatches a Slack request.
use super::*;

impl Owner {
    /// Apply a positively observed retirement after the caller has checked event
    /// ordering. Absence from a draft list must never be supplied as deletion.
    pub async fn observe_deleted(&self, expected: u64, remote_id: &str) -> Result<Stored, Fault> {
        let _guard = self.gate.lock().await;
        let view = self.current().await?;
        let mut stored = view.ready(expected)?;
        let previous = stored.state.desired().cloned();
        if !stored.state.observe_deleted(&self.scope, remote_id) {
            return Ok(stored);
        }
        let local = if previous.as_ref() != stored.state.desired() {
            match stored.state.desired() {
                None => LocalDraft::default(),
                Some(snapshot) => LocalDraft {
                    composer: codec::decode(&self.scope.key, snapshot)
                        .map_err(|_| Fault::Unsupported)?
                        .composer()
                        .clone(),
                    attachments: vec![],
                },
            }
        } else {
            view.local
        };
        self.save(Some(expected), local, stored.state).await
    }

    /// Only explicit user intent unlocks a definitely rejected consumption.
    /// The caller must request dispatch separately after this commit succeeds.
    pub async fn retry_consumption(&self, expected: u64, intent_id: &str) -> Result<Stored, Fault> {
        let _guard = self.gate.lock().await;
        let view = self.current().await?;
        let mut stored = view.ready(expected)?;
        if !stored.state.retry_consumption(&self.scope, intent_id) {
            return Err(Fault::Pending);
        }
        self.save(Some(expected), view.local, stored.state).await
    }

    /// Restore the frozen unsent draft atomically only when the reducer proves
    /// the detached composer has no newer edits and no uncertain request.
    pub async fn cancel_consumption(
        &self,
        expected: u64,
        intent_id: &str,
    ) -> Result<Stored, Fault> {
        let _guard = self.gate.lock().await;
        let view = self.current().await?;
        let mut stored = view.ready(expected)?;
        let snapshot = stored
            .state
            .cancel_consumption(&self.scope, intent_id)
            .map_err(|_| Fault::Pending)?;
        let local = LocalDraft {
            composer: codec::decode(&self.scope.key, &snapshot)
                .map_err(|_| Fault::Unsupported)?
                .composer()
                .clone(),
            attachments: vec![],
        };
        self.save(Some(expected), local, stored.state).await
    }
}
