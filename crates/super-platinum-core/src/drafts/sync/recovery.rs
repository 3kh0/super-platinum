//! Persisted spacing for explicit, read-only delivery checks. No retry trigger.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct CheckDeadline {
    operation_id: String,
    until_ms: u64,
}
impl DraftSync {
    pub fn check_retry_at_ms(&self, operation: &Operation) -> Option<u64> {
        if !matches!(&self.workflow, Workflow::Uncertain(current) if current == operation && current.kind == OperationKind::Send)
        {
            return None;
        }
        Some(
            self.check_deadline
                .as_ref()
                .filter(|d| d.operation_id == operation.id)
                .map_or(0, |d| d.until_ms),
        )
    }
    /// Requires the immutable original operation, including account and payload.
    /// Older callbacks cannot set a deadline on a replacement operation.
    pub fn defer_check(&mut self, operation: &Operation, until_ms: u64) -> bool {
        let Some(current) = self.check_retry_at_ms(operation) else {
            return false;
        };
        self.check_deadline = Some(CheckDeadline {
            operation_id: operation.id.clone(),
            until_ms: current.max(until_ms),
        });
        true
    }
}
