//! Typed commands only. No public SQL closures or raw state replacements.
use super::*;
use crate::drafts::sync::{
    ConsumeIntent, ConsumeKind, DraftObservation, RejectedScheduleChoice, Snapshot,
};
pub(crate) struct Command {
    pub(crate) account: String,
    pub(crate) action: Action,
}
pub(crate) enum Action {
    #[cfg(test)]
    PausePrepared {
        entered: std::sync::mpsc::Sender<()>,
        resume: std::sync::mpsc::Receiver<()>,
        reply: std::sync::mpsc::Sender<Result<(), Error>>,
    },
    #[cfg(any(test, feature = "protected-test-support"))]
    ArmFault {
        point: faults::FaultPoint,
        reply: std::sync::mpsc::Sender<Result<(), Error>>,
    },
    // Retained inventory read only; no caller-selected initialization mode.
    #[cfg(any(test, feature = "protected-test-support"))]
    Inventory {
        reply: std::sync::mpsc::Sender<Result<std::sync::Arc<Loaded>, Error>>,
    },
    Load {
        mode: LoadMode,
        reply: std::sync::mpsc::Sender<Result<Loaded, Error>>,
    },
    Review {
        scope: Scope,
        reply: std::sync::mpsc::Sender<Result<Review, Error>>,
    },
    ReviewLocal {
        scope: Scope,
        reply: std::sync::mpsc::Sender<Result<LocalInventory, Error>>,
    },
    EnrollLocal {
        scope: Scope,
        expected: LocalInventory,
        reply: std::sync::mpsc::Sender<Result<Review, Error>>,
    },
    RegisterWriter {
        expected: Review,
        reply: std::sync::mpsc::Sender<Result<Writer, Error>>,
    },
    Receipt {
        expected: Row,
        operation: Operation,
        reply: std::sync::mpsc::Sender<Result<Receipt, Error>>,
    },
    Checkpoint {
        writer: Writer,
        serial: u64,
        expected: Review,
        local: LocalDraft,
        direct: Option<ConsumeIntent>,
        reply: std::sync::mpsc::Sender<Result<WriteResult, Error>>,
    },
    PreserveVersion {
        writer: Writer,
        serial: u64,
        local: LocalDraft,
        reply: std::sync::mpsc::Sender<Result<WriteResult, Error>>,
    },
    Clear {
        expected: Review,
        reply: std::sync::mpsc::Sender<Result<Review, Error>>,
    },
    SettleLocal {
        expected: Review,
        choice: LocalChoice,
        reply: std::sync::mpsc::Sender<Result<Review, Error>>,
    },
    StreamState {
        team: String,
        reply: std::sync::mpsc::Sender<Result<StreamState, Error>>,
    },
    OpenStream {
        team: String,
        expected: StreamState,
        reply: std::sync::mpsc::Sender<Result<Stream, Error>>,
    },
    MarkGap {
        stream: Stream,
        reason: GapReason,
        reply: std::sync::mpsc::Sender<Result<(), Error>>,
    },
    VerifyLocal {
        stream: Stream,
        expected: Review,
        reply: std::sync::mpsc::Sender<Result<Review, Error>>,
    },
    Consume {
        stream: Stream,
        expected: Review,
        kind: ConsumeKind,
        reply: std::sync::mpsc::Sender<Result<Review, Error>>,
    },
    ReviewedSchedule {
        stream: Stream,
        expected: Review,
        reviewed: Snapshot,
        post_at: i64,
        reply: std::sync::mpsc::Sender<Result<Review, Error>>,
    },
    RejectedSchedule {
        stream: Stream,
        expected: Review,
        choice: RejectedScheduleChoice,
        now_secs: i64,
        reply: std::sync::mpsc::Sender<Result<Review, Error>>,
    },
    Control {
        expected: Review,
        action: Control,
        reply: std::sync::mpsc::Sender<Result<Review, Error>>,
    },
    Prepare {
        stream: Stream,
        expected: Review,
        reply: std::sync::mpsc::Sender<Result<DispatchTicket, Error>>,
    },
    #[cfg(any(test, feature = "protected-test-support"))]
    PrepareConsumedSend {
        stream: Stream,
        consumed: Review,
        reply: std::sync::mpsc::Sender<Result<DispatchTicket, Error>>,
    },
    ClaimDispatch {
        ticket: DispatchTicket,
        reply: std::sync::mpsc::Sender<Result<ClaimedDispatch, Error>>,
    },
    Record {
        receipt: Receipt,
        outcome: Outcome,
        known_until: u64,
        reply: std::sync::mpsc::Sender<Result<Review, Error>>,
    },
    RecoveryState {
        team: String,
        reply: std::sync::mpsc::Sender<Result<Option<RecoveryState>, Error>>,
    },
    TakeOver {
        team: String,
        expected: Option<RecoveryState>,
        reply: std::sync::mpsc::Sender<Result<Recovery, Error>>,
    },
    ReserveCheck {
        recovery: Recovery,
        expected: Review,
        receipt: Receipt,
        now_ms: u64,
        reply: std::sync::mpsc::Sender<Result<CheckTicket, Error>>,
    },
    ClaimCheck {
        recovery: Recovery,
        ticket: CheckTicket,
        now_ms: u64,
        reply: std::sync::mpsc::Sender<Result<CheckClaim, Error>>,
    },
    CompleteCheck {
        completion: CheckCompletion,
        result: CheckResult,
        known_until: u64,
        reply: std::sync::mpsc::Sender<Result<CheckReceipt, Error>>,
    },
    SupersedeCheck {
        recovery: Recovery,
        expected: Review,
        admission: LostCheck,
        now_ms: u64,
        reply: std::sync::mpsc::Sender<Result<u64, Error>>,
    },
    AcceptLatePositive {
        expected: Review,
        admission: LostCheck,
        reply: std::sync::mpsc::Sender<Result<Review, Error>>,
    },
    ReserveBaseline {
        stream: Stream,
        expected: Review,
        now_ms: u64,
        reply: std::sync::mpsc::Sender<Result<BaselineTicket, Error>>,
    },
    ClaimBaseline {
        ticket: BaselineTicket,
        now_ms: u64,
        reply: std::sync::mpsc::Sender<Result<BaselineClaim, Error>>,
    },
    RetainBaseline {
        completion: BaselineCompletion,
        response: BaselineResponse,
        known_until: u64,
        reply: std::sync::mpsc::Sender<Result<BaselineResult, Error>>,
    },
    FinishBaseline {
        evidence: BaselineEvidence,
        choice: RemoteChoice,
        reply: std::sync::mpsc::Sender<Result<Review, Error>>,
    },
    SupersedeBaseline {
        recovery: Recovery,
        expected: Review,
        admission: LostBaseline,
        now_ms: u64,
        reply: std::sync::mpsc::Sender<Result<u64, Error>>,
    },
    ReserveObservation {
        stream: Stream,
        expected: Review,
        event: DraftObservation,
        reply: std::sync::mpsc::Sender<Result<ObservationTicket, Error>>,
    },
    #[cfg(any(test, feature = "protected-test-support"))]
    FinishRetainedObservation {
        ticket: std::sync::Arc<ObservationTicket>,
        reply: std::sync::mpsc::Sender<Result<ObservationResult, Error>>,
    },
    FinishObservation {
        ticket: ObservationTicket,
        reply: std::sync::mpsc::Sender<Result<ObservationResult, Error>>,
    },
    Release {
        expected: Review,
        reply: std::sync::mpsc::Sender<Result<Release, Error>>,
    },
    #[cfg(any(test, feature = "protected-test-support"))]
    StartRetainedLocal {
        release: std::sync::Arc<Release>,
        writer: Writer,
        reply: std::sync::mpsc::Sender<Result<LocalWriterLease, Error>>,
    },
    StartLocal {
        release: Release,
        writer: Writer,
        reply: std::sync::mpsc::Sender<Result<LocalWriterLease, Error>>,
    },
    RenewLocal {
        expected: LocalInventory,
        reply: std::sync::mpsc::Sender<Result<LocalWriterLease, Error>>,
    },
    WriteLocal {
        lease: LocalWriterLease,
        predecessor_serial: u64,
        serial: u64,
        expected_local: LocalDraft,
        local: LocalDraft,
        reply: std::sync::mpsc::Sender<Result<LocalWriteResult, Error>>,
    },
    KeepEditing {
        expected: Review,
        intent: ConsumeIntent,
        reply: std::sync::mpsc::Sender<Result<(), Error>>,
    },
    #[cfg(any(test, feature = "protected-test-support"))]
    Initialize {
        reply: std::sync::mpsc::Sender<Result<(), Error>>,
    },
}
#[path = "api_execute.rs"]
mod execute;
