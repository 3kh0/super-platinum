//! Exact applied writer history; failures remain failures, never clean success.
use super::*;
impl Book {
    pub(in crate::drafts::store::worker) fn writer_settlement(
        &mut self,
        reports: &[(u64, terminal::Report)],
        retire: bool,
    ) -> bool {
        if reports.is_empty()
            || self.incomplete.is_some()
            || reports.windows(2).any(|p| p[0].0 >= p[1].0)
            || self
                .requests
                .values()
                .chain(self.updates.values())
                .any(|id| reports.iter().any(|(seen, _)| seen == id))
        {
            return false;
        }
        for (index, (id, report)) in reports.iter().enumerate() {
            let allowed = if index + 1 == reports.len() {
                report.execution == terminal::Execution::StorageAccepted
                    && matches!(
                        report.inventory,
                        terminal::Inventory::NotAssessed | terminal::Inventory::OperationPending
                    )
            } else {
                *report == terminal::Report::new(terminal::Execution::StorageUnresolved)
            };
            if !allowed
                || self.records.get(id).is_none_or(|r| {
                    r.family != "protected"
                        || r.key.is_some()
                        || r.origin != Origin::Submission
                        || r.stage != Stage::FinishedClassified
                        || r.terminal != Some(*report)
                })
            {
                return false;
            }
        }
        if retire {
            for (id, _) in reports {
                self.records.remove(id);
            }
        }
        true
    }
}
