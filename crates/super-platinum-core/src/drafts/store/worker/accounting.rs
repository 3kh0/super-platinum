//! Bounded diagnostic metadata only. No drain/readiness/admission authority.
use super::*;

const MAX_RECORDS: usize = 64;
const MAX_COMPONENT: usize = 256;
type Key = (String, DraftKey);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stage {
    Queued,
    Detached,
    FinishedUnclassified,
    FinishedClassified,
    Superseded,
    AbandonedUnknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Incomplete {
    Capacity,
    Counter,
    Metadata,
    ForeignAccount,
    Tracking,
    Abandoned,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Origin {
    Submission,
    CanonicalInput,
    CanonicalRetry,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Record {
    pub id: u64,
    pub family: &'static str,
    pub key: Option<DraftKey>,
    pub origin: Origin,
    pub stage: Stage,
    pub terminal: Option<terminal::Report>,
}
#[derive(Clone, Debug)]
pub(super) struct Snapshot {
    pub account: String,
    pub worker: String,
    pub sequence: u64,
    pub incomplete: Option<Incomplete>,
    pub records: Vec<Record>,
    pub pending_requests: usize,
    pub pending_updates: usize,
    pub failed: bool,
    pub blocked: bool,
}
pub(super) struct Identity<'a> {
    pub account: &'a str,
    pub family: &'static str,
    pub key: Option<&'a DraftKey>,
}
pub(super) struct Book {
    account: String,
    worker: String,
    sequence: u64,
    incomplete: Option<Incomplete>,
    records: BTreeMap<u64, Record>,
    requests: BTreeMap<usize, u64>,
    updates: BTreeMap<Key, u64>,
}
#[derive(Default)]
pub(super) struct Batch {
    requests: BTreeMap<usize, u64>,
    updates: BTreeMap<Key, u64>,
}
impl Book {
    /// Called only while wiring the actual A owner/worker, not a public factory.
    pub(super) fn owned(account: &str, worker: &str) -> Self {
        Self {
            account: account.into(),
            worker: worker.into(),
            sequence: 0,
            incomplete: None,
            records: BTreeMap::new(),
            requests: BTreeMap::new(),
            updates: BTreeMap::new(),
        }
    }
    fn incomplete(&mut self, reason: Incomplete) {
        self.incomplete.get_or_insert(reason);
    }
    fn allocate(&mut self, identity: Identity<'_>, origin: Origin) -> Option<u64> {
        if self.incomplete.is_some() {
            return None;
        }
        if identity.account != self.account {
            self.incomplete(Incomplete::ForeignAccount);
            return None;
        }
        let bounded = identity.account.len() <= MAX_COMPONENT
            && identity.key.is_none_or(|key| {
                key.team.len() <= MAX_COMPONENT
                    && key.channel.len() <= MAX_COMPONENT
                    && key
                        .thread
                        .as_ref()
                        .is_none_or(|value| value.len() <= MAX_COMPONENT)
            });
        if !bounded {
            self.incomplete(Incomplete::Metadata);
            return None;
        }
        if self.records.len() >= MAX_RECORDS {
            self.incomplete(Incomplete::Capacity);
            return None;
        }
        let Some(id) = self.sequence.checked_add(1) else {
            self.incomplete(Incomplete::Counter);
            return None;
        };
        self.sequence = id;
        self.records.insert(
            id,
            Record {
                id,
                family: identity.family,
                key: identity.key.cloned(),
                origin,
                stage: Stage::Queued,
                terminal: None,
            },
        );
        Some(id)
    }
    pub(super) fn admission_room(&self, count: usize) -> bool {
        self.incomplete.is_none()
            && self
                .records
                .len()
                .checked_add(count)
                .is_some_and(|n| n <= MAX_RECORDS)
            && u64::try_from(count)
                .ok()
                .and_then(|n| self.sequence.checked_add(n))
                .is_some()
    }
    // Additional handoff veto only, not an account drain/current certificate.
    pub(super) fn slash_inflight(&self) -> bool {
        self.incomplete.is_some()
            || self
                .records
                .values()
                .any(|r| !matches!(r.stage, Stage::FinishedClassified | Stage::Superseded))
    }
    pub(super) fn capacity_blocked(&self, count: usize) -> bool {
        self.incomplete.is_none()
            && self
                .records
                .len()
                .checked_add(count)
                .is_some_and(|n| n > MAX_RECORDS)
            && u64::try_from(count)
                .ok()
                .and_then(|n| self.sequence.checked_add(n))
                .is_some()
    }
    /// Only an exact consumer-observed terminal can retire diagnostic metadata.
    /// Durable SQLite lineage, idempotency, and command records are untouched.
    pub(super) fn acknowledge_success(&mut self, id: u64, report: terminal::Report) -> bool {
        let eligible = self.incomplete.is_none()
            && self.records.get(&id).is_some_and(|record| {
                record.stage == Stage::FinishedClassified
                    && record.terminal == Some(report)
                    && matches!(
                        report.execution,
                        terminal::Execution::ReadObserved | terminal::Execution::StorageAccepted
                    )
                    && report.inventory == terminal::Inventory::NotAssessed
            })
            && !self.requests.values().any(|value| *value == id)
            && !self.updates.values().any(|value| *value == id);
        if eligible {
            self.records.remove(&id);
        }
        eligible
    }
    /// Closed completed-family callers validate holder/link/result custody first.
    /// Validate the entire batch before removing any accounting record.
    pub(super) fn acknowledge_family(&mut self, reports: &[(u64, terminal::Report)]) -> bool {
        let ids: BTreeSet<_> = reports.iter().map(|(id, _)| *id).collect();
        if reports.is_empty()
            || ids.len() != reports.len()
            || self.incomplete.is_some()
            || self
                .requests
                .values()
                .chain(self.updates.values())
                .any(|id| ids.contains(id))
            || reports.iter().any(|(id, report)| {
                !matches!(
                    report.execution,
                    terminal::Execution::StorageAccepted | terminal::Execution::ReadObserved
                ) || !matches!(
                    report.inventory,
                    terminal::Inventory::NotAssessed | terminal::Inventory::OperationPending
                ) || self.records.get(id).is_none_or(|record| {
                    record.stage != Stage::FinishedClassified || record.terminal != Some(*report)
                })
            })
        {
            return false;
        }
        for id in ids {
            self.records.remove(&id);
        }
        true
    }
    pub(super) fn terminal(&self, id: u64) -> Option<terminal::Report> {
        self.records.get(&id).and_then(|record| record.terminal)
    }
    // Exact historical execution only, never a whole-account drain result.
    pub(super) fn waiting(&self, id: u64) -> bool {
        self.incomplete.is_none()
            && self
                .records
                .get(&id)
                .is_some_and(|record| matches!(record.stage, Stage::Queued | Stage::Detached))
    }
    pub(super) fn work_ids(&self) -> Vec<u64> {
        self.records.keys().copied().collect()
    }
    pub(super) fn request(&mut self, index: usize, identity: Identity<'_>) -> Option<u64> {
        if let Some(id) = self.allocate(identity, Origin::Submission) {
            if self.requests.insert(index, id).is_some() {
                self.incomplete(Incomplete::Tracking);
            }
            Some(id)
        } else {
            None
        }
    }
    pub(super) fn update(&mut self, account: &str, key: &DraftKey, retry: bool) {
        // No draft payload is received, cloned, serialized or hashed here.
        // Check metadata bounds before cloning the actual canonical key.
        let identity = Identity {
            account,
            key: Some(key),
            family: "canonical",
        };
        let id = self.allocate(
            identity,
            if retry {
                Origin::CanonicalRetry
            } else {
                Origin::CanonicalInput
            },
        );
        // Once incomplete, keep known supersession truthful when this bounded
        // key is still tracked. Never clone an oversized/untrusted key.
        if account.len() > MAX_COMPONENT
            || key.team.len() > MAX_COMPONENT
            || key.channel.len() > MAX_COMPONENT
            || key
                .thread
                .as_ref()
                .is_some_and(|value| value.len() > MAX_COMPONENT)
        {
            return;
        }
        let lookup = (account.to_owned(), key.clone());
        if let Some(old) = self.updates.remove(&lookup) {
            self.mark(old, Stage::Superseded);
        }
        if let Some(id) = id {
            self.updates.insert(lookup, id);
        }
    }
    fn mark(&mut self, id: u64, stage: Stage) {
        if let Some(record) = self.records.get_mut(&id) {
            record.stage = stage;
        } else {
            self.incomplete(Incomplete::Tracking);
        }
    }
    pub(super) fn detach(&mut self, requests: usize, updates: usize) -> Batch {
        if self.incomplete.is_none()
            && (self.requests.len() != requests || self.updates.len() != updates)
        {
            self.incomplete(Incomplete::Tracking);
        }
        let batch = Batch {
            requests: std::mem::take(&mut self.requests),
            updates: std::mem::take(&mut self.updates),
        };
        for id in batch.requests.values().chain(batch.updates.values()) {
            self.mark(*id, Stage::Detached);
        }
        batch
    }
    pub(super) fn abandon(&mut self) {
        for record in self.records.values_mut() {
            if matches!(record.stage, Stage::Queued | Stage::Detached) {
                record.stage = Stage::AbandonedUnknown;
                self.incomplete.get_or_insert(Incomplete::Abandoned);
            }
        }
        self.requests.clear();
        self.updates.clear();
    }
    pub(super) fn snapshot(&self, pending: &Pending) -> Snapshot {
        Snapshot {
            account: self.account.clone(),
            worker: self.worker.clone(),
            sequence: self.sequence,
            incomplete: self.incomplete,
            records: self.records.values().cloned().collect(),
            pending_requests: pending.requests.len(),
            pending_updates: pending.updates.len(),
            failed: pending.failed.contains(&self.account),
            blocked: pending.blocked.contains(&self.account),
        }
    }
}
pub(super) struct Executing<'a> {
    worker: &'a Worker,
    batch: Batch,
}
impl<'a> Executing<'a> {
    pub(super) fn new(worker: &'a Worker, batch: Batch) -> Self {
        Self { worker, batch }
    }
    pub(super) fn finish_request(&mut self, index: usize, report: terminal::Report) {
        if let Some(id) = self.batch.requests.remove(&index) {
            self.finish([(id, report)]);
        }
    }
    pub(super) fn update_account(&self, account: &str) -> Option<String> {
        self.batch
            .updates
            .keys()
            .any(|(owner, _)| owner == account)
            .then(|| account.to_owned())
    }
    pub(super) fn update_reports(
        &self,
        account: &str,
        result: &Result<BTreeSet<DraftKey>, AppError>,
    ) -> Vec<(u64, terminal::Report)> {
        self.batch
            .updates
            .iter()
            .filter(|((owner, _), _)| owner == account)
            .map(|((_, key), id)| {
                (
                    *id,
                    terminal::app(result, |rejected| {
                        if rejected.contains(key) {
                            terminal::Report::preserved()
                        } else {
                            terminal::accepted(rejected)
                        }
                    }),
                )
            })
            .collect()
    }
    pub(super) fn finish_updates(&mut self, account: &str, reports: Vec<(u64, terminal::Report)>) {
        let keys: Vec<_> = self
            .batch
            .updates
            .keys()
            .filter(|(owner, _)| owner == account)
            .cloned()
            .collect();
        let ids: Vec<_> = keys
            .into_iter()
            .filter_map(|key| self.batch.updates.remove(&key))
            .collect();
        if ids.len() != reports.len() {
            if let Ok(mut pending) = self.worker.pending.lock() {
                if let Some(book) = &mut pending.accounting {
                    book.incomplete(Incomplete::Tracking);
                    for id in ids {
                        book.mark(id, Stage::AbandonedUnknown);
                    }
                }
            }
            self.worker.accounted.notify_all();
            return;
        }
        self.finish(reports);
    }
    fn finish(&self, reports: impl IntoIterator<Item = (u64, terminal::Report)>) {
        if let Ok(mut pending) = self.worker.pending.lock() {
            if let Some(book) = &mut pending.accounting {
                for (id, report) in reports {
                    book.mark(
                        id,
                        if report.execution == terminal::Execution::Unsupported {
                            Stage::FinishedUnclassified
                        } else {
                            Stage::FinishedClassified
                        },
                    );
                    if let Some(record) = book.records.get_mut(&id) {
                        record.terminal = Some(report);
                    }
                }
            }
        }
        self.worker.accounted.notify_all();
    }
}
impl Drop for Executing<'_> {
    fn drop(&mut self) {
        if self.batch.requests.is_empty() && self.batch.updates.is_empty() {
            return;
        }
        if let Ok(mut pending) = self.worker.pending.lock() {
            if let Some(book) = &mut pending.accounting {
                book.incomplete(Incomplete::Abandoned);
                for id in self
                    .batch
                    .requests
                    .values()
                    .chain(self.batch.updates.values())
                {
                    book.mark(*id, Stage::AbandonedUnknown);
                }
            }
        }
        self.worker.accounted.notify_all();
    }
}

#[cfg(test)]
#[path = "accounting_tests.rs"]
mod tests;

#[path = "accounting_archive.rs"]
mod archive;
#[path = "accounting_baseline.rs"]
mod baseline;
#[path = "accounting_writer.rs"]
mod writer;
