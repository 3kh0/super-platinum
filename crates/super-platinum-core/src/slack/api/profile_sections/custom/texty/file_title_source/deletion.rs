//! xsCq -> /xid -> HcWC -> kbkl direct replacement, not 4OY3/files.info.
use super::*;
use crate::file_actions::FileDeletionEvent;
use serde_json::json;

/// Immediate detached preimage only: repeated deletions never retain a chain
/// of previous Rows or previous DeletionEvidence. The source owner separately
/// binds this data transition to its account/team and accepted event ingress.
pub struct DeletionEvidence {
    event: FileDeletionEvent,
    preimage_source: Value,
    preimage_receipt: Arc<Value>,
    preimage_reference: Option<Value>,
}
impl DeletionEvidence {
    pub fn event(&self) -> &FileDeletionEvent {
        &self.event
    }
    pub fn preimage_source(&self) -> &Value {
        &self.preimage_source
    }
    pub fn preimage_receipt(&self) -> &Value {
        &self.preimage_receipt
    }
    pub fn preimage_reference(&self) -> Option<&Value> {
        self.preimage_reference.as_ref()
    }
}

/// Exact source snapshot, including absent IDs. A later source publication may
/// not turn a previously absent event ID into a known-row deletion permission.
pub struct DeletionPlan {
    event: FileDeletionEvent,
    collection: Collection,
}
impl Store {
    pub fn prepare_deletion(&self, event: &FileDeletionEvent) -> DeletionPlan {
        DeletionPlan {
            event: event.clone(),
            collection: self.candidates(),
        }
    }
    /// Atomically assign complete tombstones for existing valid-ID rows. The
    /// original predicate is source presence, not normal/candidate eligibility:
    /// already deleted, unknown and unsupported rows remain known preimages.
    /// No mutation occurs on a retired source or capacity failure.
    pub fn apply_deletion(
        &mut self,
        plan: DeletionPlan,
    ) -> Result<Vec<ReferenceObservation>, Error> {
        if !plan.collection.current(self) {
            return Err(unavailable());
        }
        let mut rows = self.rows.clone();
        let mut changed = Vec::new();
        let receipt = Arc::new(plan.event.raw().clone());
        let event_bytes = plan.event.raw().to_string().len();
        for id in plan.event.ids() {
            let Some(previous) = self.rows.get(id) else {
                continue;
            };
            let source = json!({"id":id,"mode":"tombstone","is_deleted":true});
            let evidence = Arc::new(DeletionEvidence {
                event: plan.event.clone(),
                preimage_source: previous.source.clone(),
                preimage_receipt: previous.receipt.clone(),
                preimage_reference: previous.reference.clone(),
            });
            let category = category::project(source.as_object().unwrap(), None);
            // Count all retained values conservatively even when Arc-shared.
            let bytes = source.to_string().len() * 2
                + category.as_ref().map_or(0, |v| v.to_string().len())
                + event_bytes * 2
                + evidence.preimage_source.to_string().len()
                + evidence.preimage_receipt.to_string().len()
                + evidence
                    .preimage_reference
                    .as_ref()
                    .map_or(0, |v| v.to_string().len());
            rows.insert(
                id.clone(),
                Arc::new(Row {
                    receipt: receipt.clone(),
                    source: source.clone(),
                    // Keep the legacy normal-title/candidate contract closed.
                    effective: None,
                    candidate: None,
                    category,
                    reference: Some(source),
                    deletion: Some(evidence),
                    channel_map_order: None,
                    bytes,
                }),
            );
            changed.push(id.clone());
        }
        if rows.len() > MAX_ROWS || rows.values().map(|r| r.bytes).sum::<usize>() > MAX_TOTAL_BYTES
        {
            return Err(malformed());
        }
        self.rows = rows;
        // Existing source order remains unchanged; absent IDs were not added.
        changed.iter().map(|id| self.reference(id)).collect()
    }
}

#[cfg(test)]
mod tests;
