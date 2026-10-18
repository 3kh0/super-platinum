//! Retire exact hosted storage attempts only after their accepted cleanup was observed.
use super::*;
impl Book {
    pub(in crate::drafts::store::worker) fn hosted_draft_saved(
        &mut self,
        scope: &Scope,
        reports: &[(u64, terminal::Report)],
        retire: bool,
    ) -> bool {
        if self.account != scope.account_id
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
                terminal::Execution::StorageAccepted
            } else {
                terminal::Execution::StorageUnresolved
            });
            if *report != expected
                || self.records.get(id).is_none_or(|r| {
                    r.family != "protected-hosted"
                        || r.key.as_ref() != Some(&scope.key)
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
    pub(in crate::drafts::store::worker) fn hosted_settlement(
        &mut self,
        scope: &Scope,
        claim: u64,
        returned: &[(u64, terminal::Report)],
        cleanup: &[(u64, terminal::Report)],
        retire: bool,
    ) -> bool {
        if self.account != scope.account_id
            || self.incomplete.is_some()
            || returned.is_empty()
            || cleanup.is_empty()
        {
            return false;
        }
        let accepted = terminal::Report::new(terminal::Execution::StorageAccepted);
        let failed = terminal::Report::new(terminal::Execution::StorageUnresolved);
        for attempts in [returned, cleanup] {
            if attempts.last().is_none_or(|(_, r)| *r != accepted)
                || attempts[..attempts.len() - 1]
                    .iter()
                    .any(|(_, r)| *r != failed)
            {
                return false;
            }
        }
        let reports: Vec<_> = std::iter::once((claim, accepted))
            .chain(returned.iter().copied())
            .chain(cleanup.iter().copied())
            .collect();
        if reports.windows(2).any(|p| p[0].0 >= p[1].0)
            || self
                .requests
                .values()
                .chain(self.updates.values())
                .any(|id| reports.iter().any(|(seen, _)| seen == id))
        {
            return false;
        }
        if reports.iter().any(|(id, report)| {
            self.records.get(id).is_none_or(|r| {
                r.family != "protected-hosted"
                    || r.key.as_ref() != Some(&scope.key)
                    || r.origin != Origin::Submission
                    || r.stage != Stage::FinishedClassified
                    || r.terminal != Some(*report)
            })
        }) {
            return false;
        }
        if retire {
            for (id, _) in reports {
                self.records.remove(&id);
            }
        }
        true
    }
}
