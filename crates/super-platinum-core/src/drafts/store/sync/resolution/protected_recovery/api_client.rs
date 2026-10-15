//! Every method enqueues synchronously in the existing FIFO. No renderer I/O.
use super::*;
use crate::drafts::sync::{
    ConsumeIntent, ConsumeKind, DraftObservation, RejectedScheduleChoice, Snapshot,
};
#[derive(Clone)]
pub struct Client {
    pub(super) account: String,
    pub(super) route: crate::drafts::store::worker::protected_queue::Route,
}
impl Client {
    pub fn new(account: String) -> Self {
        Self {
            account,
            route: crate::drafts::store::worker::protected_queue::Route::Default,
        }
    }
    fn submit<T>(
        &self,
        action: impl FnOnce(std::sync::mpsc::Sender<Result<T, Error>>) -> commands::Action,
    ) -> Reply<T> {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.route.submit(commands::Command {
            account: self.account.clone(),
            action: action(sender),
        });
        Reply(receiver)
    }
    pub fn load(&self, mode: LoadMode) -> Reply<Loaded> {
        self.submit(|reply| commands::Action::Load { mode, reply })
    }
    pub fn review(&self, scope: Scope) -> Reply<Review> {
        self.submit(|reply| commands::Action::Review { scope, reply })
    }
    pub fn review_local(&self, scope: Scope) -> Reply<LocalInventory> {
        self.submit(|reply| commands::Action::ReviewLocal { scope, reply })
    }
    pub fn enroll_local(&self, scope: Scope, expected: LocalInventory) -> Reply<Review> {
        self.submit(|reply| commands::Action::EnrollLocal {
            scope,
            expected,
            reply,
        })
    }
    pub fn register_writer(&self, expected: Review) -> Reply<Writer> {
        self.submit(|reply| commands::Action::RegisterWriter { expected, reply })
    }
    pub fn receipt(&self, expected: Row, operation: Operation) -> Reply<Receipt> {
        self.submit(|reply| commands::Action::Receipt {
            expected,
            operation,
            reply,
        })
    }
    pub fn checkpoint(
        &self,
        writer: Writer,
        serial: u64,
        expected: Review,
        local: LocalDraft,
        direct: Option<ConsumeIntent>,
    ) -> Reply<WriteResult> {
        self.submit(|reply| commands::Action::Checkpoint {
            writer,
            serial,
            expected,
            local,
            direct,
            reply,
        })
    }
    pub fn preserve_version(
        &self,
        writer: Writer,
        serial: u64,
        local: LocalDraft,
    ) -> Reply<WriteResult> {
        self.submit(|reply| commands::Action::PreserveVersion {
            writer,
            serial,
            local,
            reply,
        })
    }
    pub fn clear(&self, expected: Review) -> Reply<Review> {
        self.submit(|reply| commands::Action::Clear { expected, reply })
    }
    pub fn settle_local(&self, expected: Review, choice: LocalChoice) -> Reply<Review> {
        self.submit(|reply| commands::Action::SettleLocal {
            expected,
            choice,
            reply,
        })
    }
    pub fn stream_state(&self, team: String) -> Reply<StreamState> {
        self.submit(|reply| commands::Action::StreamState { team, reply })
    }
    pub fn open_stream(&self, team: String, expected: StreamState) -> Reply<Stream> {
        self.submit(|reply| commands::Action::OpenStream {
            team,
            expected,
            reply,
        })
    }
    pub fn mark_gap(&self, stream: Stream, reason: GapReason) -> Reply<()> {
        stream.revoke();
        self.submit(|reply| commands::Action::MarkGap {
            stream,
            reason,
            reply,
        })
    }
    pub fn verify_local(&self, stream: Stream, expected: Review) -> Reply<Review> {
        self.submit(|reply| commands::Action::VerifyLocal {
            stream,
            expected,
            reply,
        })
    }
    pub fn consume(&self, stream: Stream, expected: Review, kind: ConsumeKind) -> Reply<Review> {
        self.submit(|reply| commands::Action::Consume {
            stream,
            expected,
            kind,
            reply,
        })
    }
    pub fn reviewed_schedule(
        &self,
        stream: Stream,
        expected: Review,
        reviewed: Snapshot,
        post_at: i64,
    ) -> Reply<Review> {
        self.submit(|reply| commands::Action::ReviewedSchedule {
            stream,
            expected,
            reviewed,
            post_at,
            reply,
        })
    }
    pub fn rejected_schedule(
        &self,
        stream: Stream,
        expected: Review,
        choice: RejectedScheduleChoice,
        now_secs: i64,
    ) -> Reply<Review> {
        self.submit(|reply| commands::Action::RejectedSchedule {
            stream,
            expected,
            choice,
            now_secs,
            reply,
        })
    }
    pub fn control(&self, expected: Review, action: Control) -> Reply<Review> {
        self.submit(|reply| commands::Action::Control {
            expected,
            action,
            reply,
        })
    }
    pub fn prepare(&self, stream: Stream, expected: Review) -> Reply<DispatchTicket> {
        self.submit(|reply| commands::Action::Prepare {
            stream,
            expected,
            reply,
        })
    }
    pub fn claim_dispatch(&self, ticket: DispatchTicket) -> Reply<ClaimedDispatch> {
        self.submit(|reply| commands::Action::ClaimDispatch { ticket, reply })
    }
    pub fn record(&self, receipt: Receipt, outcome: Outcome, known_until: u64) -> Reply<Review> {
        self.submit(|reply| commands::Action::Record {
            receipt,
            outcome,
            known_until,
            reply,
        })
    }
    pub fn recovery_state(&self, team: String) -> Reply<Option<RecoveryState>> {
        self.submit(|reply| commands::Action::RecoveryState { team, reply })
    }
    pub fn take_over(&self, team: String, expected: Option<RecoveryState>) -> Reply<Recovery> {
        self.submit(|reply| commands::Action::TakeOver {
            team,
            expected,
            reply,
        })
    }
    pub fn reserve_check(
        &self,
        recovery: Recovery,
        expected: Review,
        receipt: Receipt,
        now_ms: u64,
    ) -> Reply<CheckTicket> {
        self.submit(|reply| commands::Action::ReserveCheck {
            recovery,
            expected,
            receipt,
            now_ms,
            reply,
        })
    }
    pub fn claim_check(
        &self,
        recovery: Recovery,
        ticket: CheckTicket,
        now_ms: u64,
    ) -> Reply<CheckClaim> {
        self.submit(|reply| commands::Action::ClaimCheck {
            recovery,
            ticket,
            now_ms,
            reply,
        })
    }
    pub fn complete_check(
        &self,
        completion: CheckCompletion,
        result: CheckResult,
        known_until: u64,
    ) -> Reply<CheckReceipt> {
        self.submit(|reply| commands::Action::CompleteCheck {
            completion,
            result,
            known_until,
            reply,
        })
    }
    pub fn supersede_check(
        &self,
        recovery: Recovery,
        expected: Review,
        admission: LostCheck,
        now_ms: u64,
    ) -> Reply<u64> {
        self.submit(|reply| commands::Action::SupersedeCheck {
            recovery,
            expected,
            admission,
            now_ms,
            reply,
        })
    }
    pub fn accept_late_positive(&self, expected: Review, admission: LostCheck) -> Reply<Review> {
        self.submit(|reply| commands::Action::AcceptLatePositive {
            expected,
            admission,
            reply,
        })
    }
    pub fn reserve_baseline(
        &self,
        stream: Stream,
        expected: Review,
        now_ms: u64,
    ) -> Reply<BaselineTicket> {
        self.submit(|reply| commands::Action::ReserveBaseline {
            stream,
            expected,
            now_ms,
            reply,
        })
    }
    pub fn claim_baseline(&self, ticket: BaselineTicket, now_ms: u64) -> Reply<BaselineClaim> {
        self.submit(|reply| commands::Action::ClaimBaseline {
            ticket,
            now_ms,
            reply,
        })
    }
    pub fn retain_baseline(
        &self,
        completion: BaselineCompletion,
        response: BaselineResponse,
        known_until: u64,
    ) -> Reply<BaselineResult> {
        self.submit(|reply| commands::Action::RetainBaseline {
            completion,
            response,
            known_until,
            reply,
        })
    }
    pub fn finish_baseline(
        &self,
        evidence: BaselineEvidence,
        choice: RemoteChoice,
    ) -> Reply<Review> {
        self.submit(|reply| commands::Action::FinishBaseline {
            evidence,
            choice,
            reply,
        })
    }
    pub fn supersede_baseline(
        &self,
        recovery: Recovery,
        expected: Review,
        admission: LostBaseline,
        now_ms: u64,
    ) -> Reply<u64> {
        self.submit(|reply| commands::Action::SupersedeBaseline {
            recovery,
            expected,
            admission,
            now_ms,
            reply,
        })
    }
    pub fn reserve_observation(
        &self,
        stream: Stream,
        expected: Review,
        event: DraftObservation,
    ) -> Reply<ObservationTicket> {
        self.submit(|reply| commands::Action::ReserveObservation {
            stream,
            expected,
            event,
            reply,
        })
    }
    pub fn finish_observation(&self, ticket: ObservationTicket) -> Reply<ObservationResult> {
        self.submit(|reply| commands::Action::FinishObservation { ticket, reply })
    }
    pub fn release(&self, expected: Review) -> Reply<Release> {
        self.submit(|reply| commands::Action::Release { expected, reply })
    }
    pub fn start_local(&self, release: Release, writer: Writer) -> Reply<LocalWriterLease> {
        self.submit(|reply| commands::Action::StartLocal {
            release,
            writer,
            reply,
        })
    }
    pub fn renew_local(&self, expected: LocalInventory) -> Reply<LocalWriterLease> {
        self.submit(|reply| commands::Action::RenewLocal { expected, reply })
    }
    pub fn write_local(
        &self,
        lease: LocalWriterLease,
        predecessor_serial: u64,
        serial: u64,
        expected_local: LocalDraft,
        local: LocalDraft,
    ) -> Reply<LocalWriteResult> {
        self.submit(|reply| commands::Action::WriteLocal {
            lease,
            predecessor_serial,
            serial,
            expected_local,
            local,
            reply,
        })
    }
    pub fn keep_editing(&self, expected: Review, intent: ConsumeIntent) -> Reply<()> {
        self.submit(|reply| commands::Action::KeepEditing {
            expected,
            intent,
            reply,
        })
    }
}
