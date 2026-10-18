//! Retire only exact failed read attempts followed by their observed inventory.
use super::*;
impl Book {
    pub(in crate::drafts::store::worker) fn inventory_observed(
        &mut self,
        account: &str,
        reports: &[(u64, terminal::Report)],
        retire: bool,
    ) -> bool {
        if self.account != account
            || self.incomplete.is_some()
            || reports.is_empty()
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
            let expected = terminal::Report::new(if index + 1 == reports.len() {
                terminal::Execution::ReadObserved
            } else {
                terminal::Execution::StorageUnresolved
            });
            if *report != expected
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
