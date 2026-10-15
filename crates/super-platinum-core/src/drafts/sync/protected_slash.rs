//! Local input settlement for an acknowledged command. No remote lifecycle proof.
use super::*;
impl DraftSync {
    pub(crate) fn clear_acknowledged_slash_input(&self) -> Option<Self> {
        if self.workflow != Workflow::Idle
            || self.pending().is_some()
            || self.consumption.is_some()
            || self.blocked_observation.is_some()
            || self.desired.is_none()
            || self.local_revision.checked_add(1).is_none()
        {
            return None;
        }
        let mut next = self.clone();
        next.local_revision += 1;
        next.desired = None;
        next.tombstone = true;
        // acknowledged/base/client_msg_id/retired IDs/check evidence are unchanged.
        // The protected binding still blocks begin_operation(Delete) admission.
        Some(next)
    }
}
