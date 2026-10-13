//! Only a proven direct-send editor may preserve arbitrary newer local content.
use super::*;
use super_platinum_core::drafts::sync::ConsumeIntent;

pub(super) fn direct_intent(stored: &Stored) -> Option<ConsumeIntent> {
    stored
        .state
        .consumption()
        .filter(|intent| stored.state.accepts_direct_local(intent))
        .cloned()
}

impl Owner {
    pub(super) async fn checkpoint_direct_local(
        &self,
        expected: u64,
        local: LocalDraft,
        intent: &ConsumeIntent,
    ) -> Result<Stored, Fault> {
        let _guard = self.gate.lock().await;
        let mut stored = self.current().await?.ready(expected)?;
        let projection = match local_snapshot(&self.scope.key, &local) {
            Ok(snapshot) => snapshot,
            Err(Fault::Unsupported) => None,
            Err(error) => return Err(error),
        };
        stored
            .state
            .checkpoint_direct_local(intent, projection)
            .map_err(|_| Fault::Pending)?;
        self.save(Some(expected), local, stored.state).await
    }
}
