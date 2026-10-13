//! Local-only typing beside an immutable direct-send intent.
use super::*;

impl DraftSync {
    pub fn accepts_direct_local(&self, intent: &ConsumeIntent) -> bool {
        if intent.scope != self.scope
            || intent.kind != ConsumeKind::Send
            || intent.base.is_some()
            || self.acknowledged.is_some()
        {
            return false;
        }
        if let Some(current) = &self.consumption {
            current == intent
                && !matches!(self.workflow, Workflow::Conflict { .. })
                && self.pending().is_none_or(|operation| {
                    operation.kind == OperationKind::Send
                        && operation.base.is_none()
                        && operation.consume_id.as_deref() == Some(intent.id.as_str())
                })
        } else {
            // The same editor can finish a local checkpoint after its positive
            // receipt, while release is waiting on the checkpoint gate.
            self.workflow == Workflow::Idle
                && intent.local_revision.checked_add(1) == Some(self.acknowledged_local_revision)
        }
    }

    /// `desired` is only a supported projection of the separately paired local
    /// draft. None may mean unsupported content, so it must still advance the
    /// local revision to prevent restoration over newer mentions/attachments.
    pub fn checkpoint_direct_local(
        &mut self,
        intent: &ConsumeIntent,
        desired: Option<Snapshot>,
    ) -> Result<(), ConsumeError> {
        if !self.accepts_direct_local(intent) {
            return Err(ConsumeError::Busy);
        }
        self.local_revision = self
            .local_revision
            .checked_add(1)
            .ok_or(ConsumeError::RevisionExhausted)?;
        self.tombstone = desired.is_none();
        self.desired = desired;
        Ok(())
    }
}
