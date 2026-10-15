//! Local-only typing beside an exact frozen terminal consumption.
use super::*;

/// Durable provenance for the short interval between positive consumption and
/// release. Revisions alone cannot identify the intent that rotated the UUID.
/// This is local-write authority, never remote-write or replay authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct LocalCompletion {
    intent: ConsumeIntent,
    successor_client_msg_id: String,
}

impl LocalCompletion {
    pub(super) fn continuity_intent(&self) -> &ConsumeIntent {
        &self.intent
    }
}

impl DraftSync {
    /// Called only by finish_consumption, after its positive receipt has rotated
    /// client_msg_id. The marker is persisted in the same paired transaction.
    pub(super) fn retain_local_completion(&mut self, intent: ConsumeIntent) {
        self.local_completion = Some(LocalCompletion {
            intent,
            successor_client_msg_id: self.client_msg_id.clone(),
        });
        // Older loadable rows may have a base accepted by the historical
        // receipt validator but not today's full adoption rules. Never make a
        // positively confirmed receipt unpersistable by adding optional local
        // authority: omit that authority and retain ordinary completion.
        if !self.local_completion_valid() {
            self.local_completion = None;
        }
    }

    /// Structural validation only: a marker can remain inert after a remote
    /// conflict. Actual eligibility checks the full current state below.
    pub(super) fn local_completion_valid(&self) -> bool {
        self.local_completion.as_ref().is_none_or(|completion| {
            let intent = &completion.intent;
            intent.scope == self.scope
                && nonnil_uuid(&intent.id)
                && nonnil_uuid(&intent.client_msg_id)
                && nonnil_uuid(&completion.successor_client_msg_id)
                && completion.successor_client_msg_id != intent.client_msg_id
                && intent.local_revision.checked_add(1).is_some()
                && !matches!(intent.kind, ConsumeKind::Schedule { post_at } if post_at <= 0)
                && intent.base.as_ref().is_none_or(|base| {
                    base.scope == intent.scope
                        && base.client_msg_id == intent.client_msg_id
                        && Self::adopt_remote(&intent.scope, base.clone()).is_ok()
                })
        })
    }

    /// Capture only authority already proven by a durable active intent or its
    /// exact completion marker. Older rows lacking the marker still load, but
    /// cannot infer completed-intent authority from a revision number.
    pub fn local_preservation_intent(&self) -> Option<&ConsumeIntent> {
        self.consumption
            .as_ref()
            .or_else(|| self.local_completion.as_ref().map(|done| &done.intent))
            .filter(|intent| self.accepts_direct_local(intent))
    }

    // Keep the existing method name so current direct-Send callers remain on
    // the same guarded path. This does not enable Send/Schedule eligibility.
    pub fn accepts_direct_local(&self, intent: &ConsumeIntent) -> bool {
        if intent.scope != self.scope
            || self.blocked_observation.is_some()
            || matches!(self.workflow, Workflow::Conflict { .. })
            || !self.consumption_valid()
        {
            return false;
        }
        if let Some(current) = &self.consumption {
            if current != intent
                || self.client_msg_id != intent.client_msg_id
                || self.acknowledged != intent.base
            {
                return false;
            }
            let kind = match intent.kind {
                ConsumeKind::Send => OperationKind::Send,
                ConsumeKind::Schedule { post_at } => OperationKind::Schedule { post_at },
            };
            return match &self.workflow {
                Workflow::Idle => true,
                Workflow::InFlight(op) | Workflow::Uncertain(op) | Workflow::Rejected(op) => {
                    nonnil_uuid(&op.id)
                        && op.scope == intent.scope
                        && op.kind == kind
                        && op.client_msg_id == intent.client_msg_id
                        && op.consume_id.as_deref() == Some(intent.id.as_str())
                        && op.local_revision == intent.local_revision
                        && op.snapshot.as_ref() == Some(&intent.snapshot)
                        && op.base == intent.base
                }
                Workflow::Conflict { .. } => false,
            };
        }
        self.workflow == Workflow::Idle
            && self.acknowledged.is_none()
            && intent.local_revision.checked_add(1) == Some(self.acknowledged_local_revision)
            && self.local_revision >= self.acknowledged_local_revision
            && self.local_completion.as_ref().is_some_and(|done| {
                &done.intent == intent && done.successor_client_msg_id == self.client_msg_id
            })
    }

    /// None can mean unsupported local content, not deletion. The complete
    /// LocalDraft must be persisted alongside this state by the owner. Advance
    /// revision even for unsupported input or clearing, blocking stale restore.
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

#[cfg(test)]
mod tests;
