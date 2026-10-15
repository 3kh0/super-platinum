//! Durable admission for explicit read-only checks. No dispatch or retry trigger.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct CheckDeadline {
    operation_id: String,
    until_ms: u64,
    // Formats 1–4 stored only the id. The store binds an active legacy deadline
    // to its complete retained operation before allowing a new-format write.
    #[serde(default)]
    operation: Option<Operation>,
}

impl CheckDeadline {
    pub(super) fn continuity_operation(&self) -> Option<&Operation> {
        self.operation.as_ref()
    }
    pub(super) fn protected_until(&self) -> u64 {
        self.until_ms
    }
}

/// A durable receipt for ONE admitted read, not permission to replay that read.
/// Persist `reserve_check` successfully before dispatch. A loaded ticket means
/// completion was not durably recorded: elapsed time cannot release it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckTicket {
    id: String,
    operation: Operation,
}
impl CheckTicket {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn operation(&self) -> &Operation {
        &self.operation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CheckBlock {
    #[error(
        "the immutable operation is not a supported uncertain send, update, or known-ID schedule"
    )]
    Ineligible,
    #[error("a previous check has no durable completion")]
    Pending,
    #[error("the next explicit check must wait until {0}")]
    Cooldown(u64),
}

// Operation-only validation also accepts a historical completed ticket; current
// consumption linkage is checked separately when admitting/completing a read.
impl Operation {
    /// B Id6n6810246 selects Update for a schedule with an existing server ID.
    /// A missing ID has no proven UUID lookup. This first slice is file-free and
    /// preserves exact destination metadata; it does not enroll or dispatch.
    pub fn known_schedule_check(&self) -> bool {
        if !matches!(self.kind, OperationKind::Schedule { post_at } if post_at > 0)
            || !nonnil_uuid(&self.id)
            || !self.consume_id.as_deref().is_some_and(nonnil_uuid)
        {
            return false;
        }
        let (Some(base), Some(snapshot)) = (&self.base, &self.snapshot) else {
            return false;
        };
        if base.scope != self.scope
            || base.client_msg_id != self.client_msg_id
            || !base.snapshot.file_ids.is_empty()
            || !snapshot.file_ids.is_empty()
            || snapshot.blocks.is_empty()
            || snapshot.destinations != base.snapshot.destinations
            || DraftSync::adopt_remote(&self.scope, base.clone()).is_err()
        {
            return false;
        }
        let [destination] = snapshot.destinations.as_slice() else {
            return false;
        };
        if destination.as_object().is_none_or(|fields| {
            fields
                .keys()
                .any(|key| !matches!(key.as_str(), "channel_id" | "thread_ts" | "broadcast"))
                || (self.scope.key.thread.is_none() && fields.contains_key("broadcast"))
        }) {
            return false;
        }
        let mut reviewed = base.clone();
        reviewed.snapshot = snapshot.clone();
        DraftSync::adopt_remote(&self.scope, reviewed).is_ok()
    }
}
fn supported_check(operation: &Operation) -> bool {
    matches!(operation.kind, OperationKind::Send | OperationKind::Update)
        || operation.known_schedule_check()
}

impl DraftSync {
    fn check_operation(&self, operation: &Operation) -> bool {
        matches!(&self.workflow, Workflow::Uncertain(current) if current == operation
        && supported_check(current)
        && (!matches!(current.kind, OperationKind::Schedule { .. })
            || (current.scope == self.scope
                && self.consumption.is_some()
                && self.consumption_valid()
                && self.acknowledged.as_ref().is_some_and(|acknowledged| {
                    acknowledged.scope == current.scope
                            && acknowledged.client_msg_id == current.client_msg_id
                            && current.base.as_ref().is_some_and(|base| base.id == acknowledged.id)
                }))))
    }
    fn check_deadline_ms(&self, operation: &Operation) -> u64 {
        self.check_deadline
            .as_ref()
            .filter(|d| {
                d.operation_id == operation.id
                    && d.operation.as_ref().is_none_or(|bound| bound == operation)
            })
            .map_or(0, |d| d.until_ms)
    }
    /// `None` also covers an unresolved check. Use `pending_check` to distinguish
    /// that state; no guessed timestamp represents an unknown server cooldown.
    pub fn check_retry_at_ms(&self, operation: &Operation) -> Option<u64> {
        (self.check_operation(operation) && self.pending_check(operation).is_none())
            .then(|| self.check_deadline_ms(operation))
    }
    pub fn pending_check(&self, operation: &Operation) -> Option<&CheckTicket> {
        self.check_ticket
            .as_ref()
            .filter(|ticket| &ticket.operation == operation)
    }
    /// Pure preparation only. The caller must persist the returned state under
    /// storage CAS before sending its one read request. Cancellation/lost persistence
    /// acknowledgement is recovered by loading, never by replaying this ticket.
    pub fn reserve_check(
        &mut self,
        operation: &Operation,
        now_ms: u64,
    ) -> Result<CheckTicket, CheckBlock> {
        if !self.recovery_state_valid() || !self.check_operation(operation) {
            return Err(CheckBlock::Ineligible);
        }
        if self.pending_check(operation).is_some() {
            return Err(CheckBlock::Pending);
        }
        let until = self.check_deadline_ms(operation);
        if now_ms < until {
            return Err(CheckBlock::Cooldown(until));
        }
        let ticket = CheckTicket {
            id: uuid::Uuid::new_v4().to_string(),
            operation: operation.clone(),
        };
        // Local spacing is independent of any server Retry-After. Pending has no
        // timeout, so this floor cannot authorize a replay after a failed save.
        self.defer_check(operation, now_ms.saturating_add(30_000));
        self.check_ticket = Some(ticket.clone());
        Ok(ticket)
    }
    /// Complete only the exact admitted read on its unchanged operation. Apply
    /// any positive readback outcome AFTER this method on the same state clone,
    /// then persist completion + outcome together in one storage CAS.
    pub fn finish_check(&mut self, ticket: &CheckTicket, until_ms: u64) -> bool {
        if self.check_ticket.as_ref() != Some(ticket) || !self.check_operation(&ticket.operation) {
            return false;
        }
        self.defer_check(&ticket.operation, until_ms);
        self.check_ticket = None;
        true
    }
    /// A deadline-only update never releases an unresolved ticket. This also
    /// retains the existing send-dispatch Retry-After persistence API.
    pub fn defer_check(&mut self, operation: &Operation, until_ms: u64) -> bool {
        if !self.check_operation(operation) {
            return false;
        }
        // A positively resolved predecessor may retain historical check data.
        // It cannot guard a different newly admitted immutable operation.
        if self
            .check_ticket
            .as_ref()
            .is_some_and(|ticket| ticket.operation.id != operation.id)
        {
            self.check_ticket = None;
        }
        self.check_deadline = Some(CheckDeadline {
            operation_id: operation.id.clone(),
            until_ms: self.check_deadline_ms(operation).max(until_ms),
            operation: Some(operation.clone()),
        });
        true
    }
    pub(crate) fn migrate_check_deadline(&mut self) {
        let Some(deadline) = &self.check_deadline else {
            return;
        };
        if deadline.operation.is_some() {
            return;
        }
        let operation = self
            .pending()
            .filter(|op| op.id == deadline.operation_id && supported_check(op))
            .cloned();
        if let Some(operation) = operation {
            self.check_deadline.as_mut().unwrap().operation = Some(operation);
        } else {
            // An old resolved operation has no remaining check to authorize.
            // Do not fabricate its lost immutable payload from a newer draft.
            self.check_deadline = None;
        }
    }
    pub(crate) fn recovery_state_valid(&self) -> bool {
        let valid = |op: &Operation| {
            op.scope == self.scope
                && nonnil_uuid(&op.id)
                && supported_check(op)
                && self
                    .pending()
                    .filter(|current| current.id == op.id)
                    .is_none_or(|current| current == op)
        };
        self.check_deadline.as_ref().is_none_or(|deadline| {
            deadline
                .operation
                .as_ref()
                .is_some_and(|op| op.id == deadline.operation_id && valid(op))
        }) && self.check_ticket.as_ref().is_none_or(|ticket| {
            nonnil_uuid(&ticket.id)
                && valid(&ticket.operation)
                && self
                    .check_deadline
                    .as_ref()
                    .is_some_and(|deadline| deadline.operation.as_ref() == Some(&ticket.operation))
        })
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod schedule_tests;
