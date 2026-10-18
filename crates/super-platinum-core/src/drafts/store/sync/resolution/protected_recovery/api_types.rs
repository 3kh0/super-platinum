//! Opaque ownership, inventory and process capability types for the FIFO facade.
use super::*;
use crate::drafts::store::sync::{BlockedLocal, LocalAlternate};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Storage,
    Stale,
    Pending,
    Cooldown(u64),
    Gap,
    ReviewRequired,
    Unsupported,
    WrongFormat,
}
pub struct Reply<T>(pub(super) std::sync::mpsc::Receiver<Result<T, Error>>);
impl<T> Reply<T> {
    /// Must be called off the renderer thread. Dropping a waiter is not cancellation.
    pub fn wait(self) -> Result<T, Error> {
        self.0.recv().map_err(|_| Error::Storage)?
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Basis {
    Unverified,
    Local,
    Remote,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lineage(pub(super) Ownership);
impl Lineage {
    pub fn enrollment(&self) -> &str {
        &self.0.enrollment_id
    }
    pub fn verified_epoch(&self) -> u64 {
        self.0.verified_epoch
    }
    pub fn basis(&self) -> Basis {
        match self.0.basis {
            super::super::Basis::Unverified => Basis::Unverified,
            super::super::Basis::Local => Basis::Local,
            super::super::Basis::Remote => Basis::Remote,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Row {
    pub(super) inner: ProtectedRow,
    lineage: Lineage,
}
impl Row {
    pub(super) fn new(inner: ProtectedRow) -> Self {
        Self {
            lineage: Lineage(inner.ownership.clone()),
            inner,
        }
    }
    pub fn scope(&self) -> &Scope {
        self.inner.stored.state.scope()
    }
    pub fn state(&self) -> &DraftSync {
        &self.inner.stored.state
    }
    pub fn revision(&self) -> u64 {
        self.inner.stored.revision
    }
    pub fn lineage(&self) -> &Lineage {
        &self.lineage
    }
    pub fn same(&self, other: &Self) -> bool {
        self.inner.matches(&other.inner)
    }
    pub fn same_enrollment(&self, other: &Self) -> bool {
        self.scope() == other.scope() && self.lineage.enrollment() == other.lineage.enrollment()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WriterOrigin {
    Enrollment(String),
    LocalGeneration(String),
    Legacy,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionOrigin {
    pub(super) writer: String,
    pub(super) origin: WriterOrigin,
}
impl VersionOrigin {
    pub fn writer_id(&self) -> &str {
        &self.writer
    }
    pub fn origin(&self) -> &WriterOrigin {
        &self.origin
    }
}

#[derive(Clone, Debug)]
pub struct Review {
    pub(super) inner: ProtectedReview,
    pub(super) row: Row,
    pub(super) clock: u64,
    pub(super) generation: Option<String>,
    pub(super) origins: Vec<VersionOrigin>,
    pub(super) uncommitted: Option<LocalDraft>,
}
impl Review {
    pub fn row(&self) -> &Row {
        &self.row
    }
    pub fn scope(&self) -> &Scope {
        &self.inner.review.scope
    }
    pub fn local(&self) -> &LocalDraft {
        &self.inner.review.local
    }
    pub fn blocked(&self) -> Option<&BlockedLocal> {
        self.inner.review.blocked.as_ref()
    }
    pub fn alternates(&self) -> &[LocalAlternate] {
        &self.inner.review.alternates
    }
    pub fn origins(&self) -> &[VersionOrigin] {
        &self.origins
    }
    pub fn uncommitted(&self) -> Option<&LocalDraft> {
        self.uncommitted.as_ref()
    }
    pub fn same(&self, other: &Self) -> bool {
        self.inner.matches(&other.inner)
            && self.clock == other.clock
            && self.generation == other.generation
            && self.origins == other.origins
            && same_optional_local(self.uncommitted(), other.uncommitted())
    }
}
fn same_optional_local(a: Option<&LocalDraft>, b: Option<&LocalDraft>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => same_local(a, b),
        _ => false,
    }
}

#[derive(Clone, Debug)]
pub struct LocalInventory {
    pub(super) review: LocalReview,
    pub(super) row: Option<Row>,
    pub(super) origins: Vec<VersionOrigin>,
    pub(super) clock: u64,
    pub(super) generation: Option<String>,
    pub(super) uncommitted: Option<LocalDraft>,
}
impl LocalInventory {
    pub fn scope(&self) -> &Scope {
        &self.review.scope
    }
    pub fn local(&self) -> &LocalDraft {
        &self.review.local
    }
    pub fn blocked(&self) -> Option<&BlockedLocal> {
        self.review.blocked.as_ref()
    }
    pub fn alternates(&self) -> &[LocalAlternate] {
        &self.review.alternates
    }
    pub fn origins(&self) -> &[VersionOrigin] {
        &self.origins
    }
    pub fn row(&self) -> Option<&Row> {
        self.row.as_ref()
    }
    pub fn clock(&self) -> u64 {
        self.clock
    }
    pub fn uncommitted(&self) -> Option<&LocalDraft> {
        self.uncommitted.as_ref()
    }
    pub fn same(&self, other: &Self) -> bool {
        self.clock == other.clock
            && self.generation == other.generation
            && self.review.matches(&other.review)
            && self.origins == other.origins
            && match (&self.row, &other.row) {
                (Some(a), Some(b)) => a.same(b),
                (None, None) => true,
                _ => false,
            }
            && same_optional_local(self.uncommitted(), other.uncommitted())
    }
}

pub struct Loaded {
    pub(super) hosted: hosted_files::Inventory,
    pub(super) managed: BTreeMap<DraftKey, Review>,
    pub(super) locals: BTreeMap<DraftKey, LocalInventory>,
    pub(super) uncommitted: BTreeMap<DraftKey, LocalDraft>,
    pub(super) recovery: Vec<RecoverySummary>,
}
impl Loaded {
    pub fn hosted(&self) -> &hosted_files::Inventory {
        &self.hosted
    }
    pub fn managed(&self) -> &BTreeMap<DraftKey, Review> {
        &self.managed
    }
    pub fn locals(&self) -> &BTreeMap<DraftKey, LocalInventory> {
        &self.locals
    }
    pub fn uncommitted(&self) -> &BTreeMap<DraftKey, LocalDraft> {
        &self.uncommitted
    }
    pub fn recovery(&self) -> &[RecoverySummary] {
        &self.recovery
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadMode {
    Current,
    AfterRestart,
}

#[derive(Clone)]
pub struct Writer {
    pub(super) scope: Scope,
    pub(super) enrollment: String,
    pub(super) writer: LocalWriter,
}
impl Writer {
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn enrollment(&self) -> &str {
        &self.enrollment
    }
    pub fn id(&self) -> &str {
        self.writer.id()
    }
    pub fn initial_serial(&self) -> u64 {
        0
    }
}
pub enum WriteResult {
    Applied(Review),
    Preserved(LocalInventory),
    Retired,
}
pub enum LocalWriteResult {
    Applied(LocalInventory),
    Preserved(LocalInventory),
    Retired,
}

#[derive(Clone)]
pub struct Stream(pub(super) LiveLease);
impl Stream {
    pub fn account(&self) -> &str {
        &self.0.account
    }
    pub fn team(&self) -> &str {
        &self.0.team
    }
    pub fn epoch(&self) -> u64 {
        self.0.epoch
    }
    pub fn is_live(&self) -> bool {
        self.0.is_live()
    }
    pub fn revoke(&self) {
        // Immediate and shared: no wait for the disk-held admission gate.
        let _ = self.0.revoke();
    }
}
#[derive(Clone, Debug)]
pub struct StreamState {
    pub(super) account: String,
    pub(super) team: String,
    pub(super) inner: Team,
}
impl StreamState {
    pub fn account(&self) -> &str {
        &self.account
    }
    pub fn team(&self) -> &str {
        &self.team
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GapReason {
    Disconnect,
    SourceReplaced,
    QueueOverflow,
    InterruptedAdmission,
    StorageFailure,
    ExplicitRecovery,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Active(crate::drafts::sync::RemoteDraft),
    Deleted {
        draft_id: String,
    },
    Sent {
        client_msg_id: String,
        draft_id: Option<String>,
        ts: String,
    },
    Scheduled {
        remote: crate::drafts::sync::RemoteDraft,
        post_at: i64,
    },
    Rejected,
    Uncertain,
}
pub enum Control {
    Clear,
    RetryConsumption(String),
    ReturnConsumption(String),
}
#[derive(Clone)]
pub struct Receipt(pub(super) facade::ReceiptTarget);
impl Receipt {
    pub fn scope(&self) -> &Scope {
        &self.0.scope
    }
    pub fn enrollment(&self) -> &str {
        &self.0.enrollment
    }
    pub fn operation(&self) -> &Operation {
        &self.0.operation
    }
}
pub struct DispatchTicket {
    pub(super) stream: Stream,
    pub(super) receipt: Receipt,
}
pub struct ClaimedDispatch(pub(super) Receipt);
impl ClaimedDispatch {
    // Receipt already permits storage-only completion. Borrowing it does not
    // consume or reconstruct the issued dispatch permission.
    pub(crate) fn receipt(&self) -> &Receipt {
        &self.0
    }
    pub fn operation(&self) -> &Operation {
        self.0.operation()
    }
    pub fn into_parts(self) -> (Operation, Receipt) {
        (self.0.operation().clone(), self.0)
    }
}

#[derive(Clone)]
pub struct Recovery {
    pub(super) account: String,
    pub(super) team: String,
    pub(super) inner: Arc<recovery::Permit>,
}
impl Recovery {
    pub fn is_live(&self) -> bool {
        self.inner.is_live()
    }
}
#[derive(Clone, Debug)]
pub struct RecoveryState {
    pub(super) account: String,
    pub(super) team: String,
    pub(super) inner: recovery::RecoveryOwner,
}
impl RecoveryState {
    pub fn account(&self) -> &str {
        &self.account
    }
    pub fn team(&self) -> &str {
        &self.team
    }
}
pub struct CheckTicket {
    pub(super) inner: recovery::Ticket,
    pub(super) receipt: Receipt,
}
#[derive(Clone)]
pub struct CheckCompletion {
    pub(super) inner: recovery::Completion,
    pub(super) receipt: Receipt,
}
impl CheckCompletion {
    pub fn scope(&self) -> &Scope {
        self.receipt.scope()
    }
    pub fn enrollment(&self) -> &str {
        self.receipt.enrollment()
    }
    pub fn operation(&self) -> &Operation {
        self.receipt.operation()
    }
}
pub enum CheckClaim {
    Ready {
        operation: Operation,
        completion: CheckCompletion,
    },
    Deferred {
        ticket: CheckTicket,
        until: u64,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KnownReadFailure {
    Missing,
    Unsupported,
    ApiError,
    RateLimited,
    Unavailable,
    NotRequested,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckResult {
    Positive(Outcome),
    Nonpositive(KnownReadFailure),
}
pub struct CheckReceipt {
    pub(super) review: Option<Review>,
    pub(super) positive_accepted: bool,
}
impl CheckReceipt {
    pub fn review(&self) -> Option<&Review> {
        self.review.as_ref()
    }
    pub fn positive_accepted(&self) -> bool {
        self.positive_accepted
    }
}
#[derive(Clone, Debug)]
pub struct LostCheck(pub(super) recovery::Attempt);
#[derive(Clone, Debug)]
pub struct LostBaseline {
    pub(super) scope: Scope,
    pub(super) id: String,
    pub(super) record: ReadRecord,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryPhase {
    Pending,
    Completed,
    SupersededUnknown,
    Historical,
}
#[derive(Clone, Debug)]
pub struct RecoverySummary {
    pub(super) scope: Scope,
    pub(super) enrollment: String,
    pub(super) operation: Option<Operation>,
    pub(super) phase: RecoveryPhase,
    pub(super) deadline: u64,
    pub(super) check: Option<LostCheck>,
    pub(super) baseline: Option<LostBaseline>,
}
impl RecoverySummary {
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn enrollment(&self) -> &str {
        &self.enrollment
    }
    pub fn operation(&self) -> Option<&Operation> {
        self.operation.as_ref()
    }
    pub fn phase(&self) -> RecoveryPhase {
        self.phase
    }
    pub fn deadline(&self) -> u64 {
        self.deadline
    }
    pub fn lost_check(&self) -> Option<&LostCheck> {
        self.check.as_ref()
    }
    pub fn lost_baseline(&self) -> Option<&LostBaseline> {
        self.baseline.as_ref()
    }
}

pub struct BaselineTicket {
    pub(super) review: Review,
    pub(super) inner: ReadTicket,
}
pub struct BaselineRequest {
    pub(super) scope: Scope,
    pub(super) draft_id: String,
}
impl BaselineRequest {
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn draft_id(&self) -> &str {
        &self.draft_id
    }
}
#[derive(Clone)]
pub struct BaselineCompletion {
    pub(super) review: Review,
    pub(super) inner: Arc<ReadTicket>,
}
impl BaselineCompletion {
    pub fn scope(&self) -> &Scope {
        &self.inner.admission.expected.review.scope
    }
    pub fn enrollment(&self) -> &str {
        &self.inner.admission.pending.enrollment_id
    }
    pub fn admission_id(&self) -> &str {
        &self.inner.admission.pending.id
    }
}
pub enum BaselineClaim {
    Ready {
        request: BaselineRequest,
        completion: BaselineCompletion,
    },
    Deferred {
        ticket: BaselineTicket,
        until: u64,
    },
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum BaselineResponse {
    Positive {
        raw: serde_json::Value,
        user: String,
    },
    Nonpositive(KnownReadFailure),
}
pub enum BaselineResult {
    Retained(BaselineEvidence),
    Nonpositive,
}
#[derive(Clone)]
pub struct BaselineEvidence {
    pub(super) completion: BaselineCompletion,
    pub(super) review: Review,
    pub(super) remote: crate::drafts::sync::RemoteDraft,
}
impl BaselineEvidence {
    pub fn scope(&self) -> &Scope {
        self.completion.scope()
    }
    pub fn enrollment(&self) -> &str {
        self.completion.enrollment()
    }
    pub fn admission_id(&self) -> &str {
        self.completion.admission_id()
    }
    pub fn review(&self) -> &Review {
        &self.review
    }
    pub fn remote(&self) -> &crate::drafts::sync::RemoteDraft {
        &self.remote
    }
}

pub struct ObservationTicket {
    pub(super) inner: Admission,
    pub(super) review: Review,
}
pub struct ObservationResult {
    pub(super) before: Review,
    pub(super) after: Review,
}
impl ObservationResult {
    pub fn before(&self) -> &Review {
        &self.before
    }
    pub fn after(&self) -> &Review {
        &self.after
    }
}

#[derive(Clone)]
pub struct Release(pub(super) archive::Release);
#[derive(Clone)]
pub struct LocalWriterLease {
    pub(super) scope: Scope,
    pub(super) writer: LocalWriter,
    pub(super) generation: String,
    pub(super) initial_serial: u64,
}
impl LocalWriterLease {
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn writer_id(&self) -> &str {
        self.writer.id()
    }
    pub fn generation(&self) -> &str {
        &self.generation
    }
    pub fn initial_serial(&self) -> u64 {
        self.initial_serial
    }
}
