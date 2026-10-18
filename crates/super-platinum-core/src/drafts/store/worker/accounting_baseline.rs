//! Closed baseline history only. Generic success retirement still rejects Deferred.
use super::*;
impl Book {
    /// Called after exact retained-ticket/result proof under Pending. A prior
    /// Deferred claim is retired only as part of this completed baseline chain;
    /// it is never independently promoted to successful work.
    pub(in crate::drafts::store::worker) fn baseline_family(
        &mut self,
        reports: &[(u64, terminal::Report)],
        retire: bool,
    ) -> bool {
        let ids: BTreeSet<_> = reports.iter().map(|(id, _)| *id).collect();
        if reports.len() < 3
            || ids.len() != reports.len()
            || self.incomplete.is_some()
            || self
                .requests
                .values()
                .chain(self.updates.values())
                .any(|id| ids.contains(id))
        {
            return false;
        }
        for (index, (id, report)) in reports.iter().enumerate() {
            // Actual Request::Protected accounting deliberately uses the broad
            // "protected" tag. Exact named family slots and typed result links
            // are proved by observe_baseline_family before entering this helper.
            let family = "protected";
            let expected = if index + 1 == reports.len() {
                terminal::Report::new(terminal::Execution::StorageAccepted)
            } else if index != 0 && index + 2 < reports.len() {
                terminal::Report::new(terminal::Execution::Deferred)
            } else {
                terminal::Report::new(terminal::Execution::StorageAccepted).pending(true)
            };
            if *report != expected
                || self.records.get(id).is_none_or(|r| {
                    r.family != family
                        || r.stage != Stage::FinishedClassified
                        || r.terminal != Some(*report)
                })
            {
                return false;
            }
        }
        if retire {
            for id in ids {
                self.records.remove(&id);
            }
        }
        true
    }
}

impl Book {
    // Called only after the typed exact family/result proof under Pending.
    fn baseline_observed_records(
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
        for (id, report) in reports {
            if self.records.get(id).is_none_or(|r| {
                r.family != "protected"
                    || r.key.is_some()
                    || r.origin != Origin::Submission
                    || r.stage != Stage::FinishedClassified
                    || r.terminal != Some(*report)
            }) {
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
    pub(in crate::drafts::store::worker) fn baseline_cooldown(
        &mut self,
        id: u64,
        retire: bool,
    ) -> bool {
        self.baseline_observed_records(
            &[(id, terminal::Report::new(terminal::Execution::Deferred))],
            retire,
        )
    }
    pub(in crate::drafts::store::worker) fn baseline_finish_repair(
        &mut self,
        reports: &[(u64, terminal::Report)],
        retire: bool,
    ) -> bool {
        if reports.len() < 2
            || reports.iter().enumerate().any(|(i, (_, r))| {
                *r != terminal::Report::new(if i + 1 == reports.len() {
                    terminal::Execution::StorageAccepted
                } else {
                    terminal::Execution::StorageUnresolved
                })
            })
        {
            return false;
        }
        self.baseline_observed_records(reports, retire)
    }
    pub(in crate::drafts::store::worker) fn baseline_repaired_family(
        &mut self,
        reports: &[(u64, terminal::Report)],
        issuance_count: usize,
        retire: bool,
    ) -> bool {
        if issuance_count < 2 || reports.len() < issuance_count + 2 {
            return false;
        }
        for (i, (_, r)) in reports.iter().enumerate() {
            let expected = if i + 1 == reports.len() {
                terminal::Report::new(terminal::Execution::StorageAccepted)
            } else if i >= issuance_count {
                terminal::Report::new(terminal::Execution::StorageUnresolved)
            } else if i != 0 && i + 1 < issuance_count {
                terminal::Report::new(terminal::Execution::Deferred)
            } else {
                terminal::Report::new(terminal::Execution::StorageAccepted).pending(true)
            };
            if *r != expected {
                return false;
            }
        }
        self.baseline_observed_records(reports, retire)
    }
}
