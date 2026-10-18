//! Exact repaired archive diagnostics; no generic failed-record eviction.
use super::*;
impl Book {
    pub(in crate::drafts::store::worker) fn archive_repair(
        &mut self,
        key: &DraftKey,
        reports: &[(u64, terminal::Report)],
        retire: bool,
    ) -> bool {
        if reports.len() < 2
            || self.incomplete.is_some()
            || reports.windows(2).any(|p| p[0].0 >= p[1].0)
            || self
                .requests
                .values()
                .chain(self.updates.values())
                .any(|id| reports.iter().any(|(observed, _)| observed == id))
        {
            return false;
        }
        for (index, (id, report)) in reports.iter().enumerate() {
            let expected = terminal::Report::new(if index + 1 == reports.len() {
                terminal::Execution::StorageAccepted
            } else {
                terminal::Execution::StorageUnresolved
            });
            if *report != expected
                || self.records.get(id).is_none_or(|r| {
                    r.family != "protected-slash"
                        || r.key.as_ref() != Some(key)
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
