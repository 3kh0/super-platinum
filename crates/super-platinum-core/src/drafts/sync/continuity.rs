//! DISCONNECTED candidate child of drafts::sync. No activation caller.
use super::*;

impl DraftSync {
    /// Collision-only evidence from retained immutable operations/completions.
    /// This never authorizes routing to the row that supplies the history.
    pub(crate) fn continuity_history_claims(
        &self,
        draft_id: &str,
        client_id: Option<&str>,
    ) -> bool {
        let remote_claims = |base: &RemoteDraft| {
            base.id == draft_id || client_id == Some(base.client_msg_id.as_str())
        };
        let operation_claims = |op: &Operation| {
            client_id == Some(op.client_msg_id.as_str())
                || op.base.as_ref().is_some_and(&remote_claims)
        };
        self.local_completion.as_ref().is_some_and(|done| {
            let intent = done.continuity_intent();
            client_id == Some(intent.client_msg_id.as_str())
                || intent.base.as_ref().is_some_and(&remote_claims)
        }) || self
            .check_ticket
            .as_ref()
            .is_some_and(|ticket| operation_claims(ticket.operation()))
            || self
                .check_deadline
                .as_ref()
                .and_then(|deadline| deadline.continuity_operation())
                .is_some_and(operation_claims)
    }

    /// A protected owner cannot be released without an archive while any late
    /// frame/receipt can still name an identity this row claims. The current
    /// client's UUID alone, without remote/history authority, is not a claim.
    pub(crate) fn has_remote_identity_history(&self) -> bool {
        self.acknowledged.is_some()
            || self.workflow != Workflow::Idle
            || self.consumption.is_some()
            || self.local_completion.is_some()
            || self.check_ticket.is_some()
            || self.check_deadline.is_some()
            || !self.retired_ids.is_empty()
            || !self.observed_events.is_empty()
            || self.blocked_observation.is_some()
    }
}
