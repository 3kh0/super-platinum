//! Pure explicit remote cleanup reducer. This never authorizes network execution.
use super::*;
impl DraftSync {
    pub(crate) fn reserve_slash_cleanup(
        &mut self,
        scope: &Scope,
        base: &RemoteDraft,
        id: &str,
    ) -> Option<Operation> {
        if scope != &self.scope
            || self.acknowledged.as_ref() != Some(base)
            || self.workflow != Workflow::Idle
            || self.consumption.is_some()
            || self.blocked_observation.is_some()
            || self.check_ticket.is_some()
            || self.check_deadline.is_some()
            || self.retired_ids.contains(&base.id)
            || uuid::Uuid::parse_str(id).is_err()
            || uuid::Uuid::parse_str(id).ok()?.is_nil()
            || !base.snapshot.file_ids.is_empty()
            || base.snapshot.destinations.len() != 1
        {
            return None;
        }
        let operation = Operation {
            id: id.into(),
            scope: self.scope.clone(),
            client_msg_id: self.client_msg_id.clone(),
            local_revision: self.local_revision,
            kind: OperationKind::Delete,
            snapshot: None,
            base: Some(base.clone()),
            consume_id: None,
        };
        self.local_completion = None;
        self.workflow = Workflow::InFlight(operation.clone());
        Some(operation)
    }
    pub(crate) fn matches_slash_cleanup_result(&self, op: &Operation, after: &Self) -> bool {
        if self.pending() != Some(op)
            || self.acknowledged.as_ref() != op.base.as_ref()
            || !matches!(self.workflow, Workflow::InFlight(_))
            || op.kind != OperationKind::Delete
            || after.client_msg_id == self.client_msg_id
            || uuid::Uuid::parse_str(&after.client_msg_id).is_err()
            || uuid::Uuid::parse_str(&after.client_msg_id).is_ok_and(|u| u.is_nil())
        {
            return false;
        }
        let Some(base) = &op.base else { return false };
        let mut expected = self.clone();
        if !expected.confirm_deleted(&op.scope, &op.id, &base.id) {
            return false;
        }
        expected.client_msg_id = after.client_msg_id.clone();
        expected == *after
    }
    /// Fresh accepted writer input may advance after a known Delete receipt;
    /// every remote/lifecycle/identity field must remain that exact successor.
    pub(crate) fn matches_slash_cleanup_followup(&self, settled: &Self) -> bool {
        if self.scope != settled.scope
            || settled.acknowledged.is_some()
            || settled.pending().is_some()
            || settled.consumption.is_some()
            || settled.workflow != Workflow::Idle
        {
            return false;
        }
        if self == settled {
            return true;
        }
        if self.local_revision <= settled.local_revision || self.tombstone != self.desired.is_none()
        {
            return false;
        }
        let mut normalized = self.clone();
        normalized.desired = settled.desired.clone();
        normalized.local_revision = settled.local_revision;
        normalized.tombstone = settled.tombstone;
        normalized == *settled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleanup_followup_accepts_only_real_edit_shape_including_aba() {
        let scope = Scope {
            account_id: uuid::Uuid::new_v4().to_string(),
            key: crate::drafts::DraftKey {
                team: "T1".into(),
                channel: "C1".into(),
                thread: None,
            },
        };
        let settled = DraftSync::new(scope.clone());
        assert!(settled.matches_slash_cleanup_followup(&settled));
        let mut changed = settled.clone();
        changed.tombstone = !changed.tombstone;
        assert!(!changed.matches_slash_cleanup_followup(&settled));
        let mut composer = crate::ComposerState::default();
        composer.text = "newer".into();
        let snapshot = crate::drafts::codec::encode_composer(&scope.key, &composer).unwrap();
        let mut changed = settled.clone();
        changed.desired = Some(snapshot.clone());
        assert!(!changed.matches_slash_cleanup_followup(&settled));
        let mut changed = settled.clone();
        changed.edit(&scope, Some(snapshot));
        changed.edit(&scope, None);
        assert!(changed.matches_slash_cleanup_followup(&settled));
        changed.client_msg_id = uuid::Uuid::new_v4().to_string();
        assert!(!changed.matches_slash_cleanup_followup(&settled));
    }
}
