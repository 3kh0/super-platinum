//! Proposed additional cfg(test) child of drafts::sync. Collision-only helpers.
use super::*;
impl DraftSync {
    pub(crate) fn protected_retained_check(&self) -> Option<&CheckTicket> {
        self.check_ticket.as_ref()
    }
    pub(crate) fn protected_retained_deadline(&self) -> u64 {
        self.check_deadline
            .as_ref()
            .map_or(0, |d| d.protected_until())
    }
    pub(crate) fn protected_claims(&self, server: Option<&str>, client: Option<&str>) -> bool {
        let remote = |r: &RemoteDraft| {
            server == Some(r.id.as_str()) || client == Some(r.client_msg_id.as_str())
        };
        client == Some(self.client_msg_id())
            || self.acknowledged.as_ref().is_some_and(&remote)
            || self.pending().is_some_and(|op| {
                client == Some(op.client_msg_id.as_str()) || op.base.as_ref().is_some_and(&remote)
            })
            || self.consumption.as_ref().is_some_and(|i| {
                client == Some(i.client_msg_id.as_str()) || i.base.as_ref().is_some_and(&remote)
            })
            || server.is_some_and(|id| self.retired_ids.iter().any(|x| x == id))
            || self
                .observed_events
                .values()
                .chain(self.blocked_observation.iter())
                .any(|e| {
                    server == Some(e.draft_id.as_str()) || client == Some(e.client_msg_id.as_str())
                })
            || self.continuity_history_claims(server.unwrap_or(""), client)
    }
    pub(crate) fn protected_current_identity_has_history(&self) -> bool {
        let id = self.client_msg_id();
        self.acknowledged.is_some()
            || self.pending().is_some()
            || self.consumption.is_some()
            || self
                .observed_events
                .values()
                .chain(self.blocked_observation.iter())
                .any(|e| e.client_msg_id == id)
            || self.continuity_history_claims("", Some(id))
    }
}
