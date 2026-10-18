//! Account-retained actual baseline family. Explicit read/resume/repair only;
//! no timer, alternative storage client, synthesized evidence or choice.
use super::owned_route::{o, ui};
use super::*;
use crate::{
    drafts::network::{BaselineRead, Binding},
    state::ShellState,
};
use dioxus::prelude::{ReadableExt, Signal};

impl Evidence {
    pub(crate) fn source(&self) -> Arc<Source> {
        self.source.clone()
    }
    pub(crate) fn binding(&self) -> Binding {
        self.binding.clone()
    }
    pub(crate) fn remote(&self) -> &super_platinum_core::drafts::sync::RemoteDraft {
        self.evidence.remote()
    }
    pub(crate) fn review(&self) -> &durable::Review {
        self.evidence.review()
    }
}

pub(super) struct Read {
    owner: Owner,
    expected: durable::Review,
    binding: Binding,
    source: Arc<source::Source>,
    gate: tokio::sync::Mutex<()>,
    held: ownership::Tracked<State>,
}
#[derive(Default)]
struct State {
    reserved: Option<Result<o::BaselineReserved, o::AdmissionRefused<o::BaselineInput>>>,
    // Last actual no-request deadline is retained as attribution, while its
    // exact observed three-slot family is released through the closed receipt.
    cooldown: Option<o::BaselineCooldown>,
    claimed: Option<o::BaselineClaimed>,
    completion: Option<o::BaselineCompletion>,
    read: Option<Arc<BaselineRead>>,
    outcome: Option<o::BaselineOutcome>,
    acknowledgment: Option<ReadAcknowledgment>,
    acknowledged: bool,
}
enum ReadAcknowledgment {
    Clean(o::BaselineAcknowledgment),
    Repaired(o::BaselineRepairReceipt, o::BaselineRepairAcknowledgment),
}
impl ReadAcknowledgment {
    fn acknowledge(self) -> Result<(), (o::AdmissionError, Self)> {
        match self {
            Self::Clean(receipt) => receipt.acknowledge().map_err(|(e, r)| (e, Self::Clean(r))),
            Self::Repaired(observed, receipt) => receipt
                .acknowledge()
                .map_err(|(e, r)| (e, Self::Repaired(observed, r))),
        }
    }
}
impl Owner {
    pub(super) fn begin_owned_baseline(
        &self,
        shell: &ShellState,
        expected: durable::Review,
        binding: Binding,
        source: Arc<source::Source>,
        now: u64,
    ) -> Result<Arc<Read>, durable::Error> {
        let route = self
            .account
            .owned
            .as_ref()
            .ok_or(durable::Error::Unsupported)?;
        self.check(&expected)?;
        if !binding.current(shell) {
            return Err(durable::Error::Stale);
        }
        let stream = source.stream(shell)?;
        self.check_stream(&stream)?;
        let mut journal = self
            .account
            .journal
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        if let Some(read) = journal.owned_baselines.get(&self.scope.key) {
            if read.expected.same(&expected)
                && Arc::ptr_eq(&read.source, &source)
                && read.binding.current(shell)
            {
                return Ok(read.clone());
            }
            return Err(durable::Error::Pending);
        }
        if journal.busy_scope(self.scope()) || journal.occupied() >= retention::MAX_JOBS {
            return Err(durable::Error::Pending);
        }
        // The returned refusal/input also stays inside the same account slot.
        let reserved =
            route
                .operations
                .baseline(o::BaselineInput::new(stream, expected.clone(), now));
        let read = Arc::new(Read {
            owner: self.clone(),
            expected,
            binding,
            source,
            gate: tokio::sync::Mutex::new(()),
            held: ownership::Tracked::new(State {
                reserved: Some(reserved),
                ..Default::default()
            }),
        });
        journal
            .owned_baselines
            .insert(self.scope.key.clone(), read.clone());
        Ok(read)
    }
}
impl Read {
    fn current(&self, shell: &ShellState) -> bool {
        self.binding.current(shell) && self.source.current(shell)
    }
    pub(super) fn expected(&self) -> &durable::Review {
        &self.expected
    }
    pub(super) fn transport_read(&self) -> Option<Arc<BaselineRead>> {
        self.held.lock().ok()?.read.clone()
    }
    pub(super) async fn explicit_read(
        self: &Arc<Self>,
        state: Signal<ShellState>,
        now: fn() -> u64,
    ) -> Result<BaselineRun, durable::Error> {
        let completion = self
            .held
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .completion
            .clone();
        if let Some(completion) = completion {
            match completion.state().map_err(ui)? {
                o::BaselineState::Deferred(until) if now() < until => {
                    return Ok(BaselineRun::Deferred { until });
                }
                o::BaselineState::Deferred(_) | o::BaselineState::ClaimPending { .. } => {
                    return self.resume(state, now).await;
                }
                o::BaselineState::Failed(_) => return self.retry_storage().await,
                _ => {}
            }
        }
        self.run(state, now).await
    }
    /// Root-scope future retains this same read after the UI waiter is dropped.
    /// Resume and storage repair are separate explicit invocations.
    pub(super) async fn run(
        self: &Arc<Self>,
        shell: Signal<ShellState>,
        now: fn() -> u64,
    ) -> Result<BaselineRun, durable::Error> {
        let read = self.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        dioxus::core::spawn_forever(async move {
            let _ = send.send(read.run_inner(shell, now).await);
        });
        receive.await.map_err(|_| durable::Error::Storage)?
    }
    async fn run_inner(
        &self,
        shell: Signal<ShellState>,
        now: fn() -> u64,
    ) -> Result<BaselineRun, durable::Error> {
        let _gate = self.gate.lock().await;
        if let Some(value) = self.result()? {
            return Ok(value);
        }
        self.resume_reservation_after_cooldown(shell, now())?;
        let reserved = self
            .held
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .reserved
            .as_ref()
            .ok_or(durable::Error::Stale)?
            .as_ref()
            .map_err(|r| ui(r.reason))?
            .clone();
        reserved.submit().map_err(ui)?;
        let waiting = reserved.clone();
        let status = tokio::task::spawn_blocking(move || waiting.wait())
            .await
            .map_err(|_| durable::Error::Storage)?
            .map_err(ui)?;
        if let o::JobState::Failed(durable::Error::Cooldown(until)) = status {
            self.observe_cooldown(until)?;
            return Err(durable::Error::Cooldown(until));
        }
        ready(status)?;
        let claimed = {
            let mut held = self.held.lock().map_err(|_| durable::Error::Storage)?;
            if held.claimed.is_none() {
                held.claimed = Some(reserved.claim().map_err(ui)?);
            }
            held.claimed.as_ref().unwrap().clone()
        };
        claimed.submit().map_err(ui)?;
        let waiting = claimed.clone();
        ready(
            tokio::task::spawn_blocking(move || waiting.wait())
                .await
                .map_err(|_| durable::Error::Storage)?
                .map_err(ui)?,
        )?;
        let completion = {
            let mut held = self.held.lock().map_err(|_| durable::Error::Storage)?;
            if held.completion.is_none() {
                held.completion = Some(claimed.completion().map_err(ui)?);
            }
            held.completion.as_ref().unwrap().clone()
        };
        self.finish(shell, now, completion).await
    }
    /// Called once by an explicit read action, under the read gate. This is a
    /// new no-request reservation, not same-ticket deferred-claim resumption.
    fn resume_reservation_after_cooldown(
        &self,
        shell: Signal<ShellState>,
        now: u64,
    ) -> Result<(), durable::Error> {
        let current = shell.try_peek().map_err(|_| durable::Error::Stale)?;
        if !self.current(&current) {
            return Err(durable::Error::Gap);
        }
        let mut held = self.held.lock().map_err(|_| durable::Error::Storage)?;
        let Some(observed) = &held.cooldown else {
            return Ok(());
        };
        if held.reserved.is_some() {
            return Ok(());
        }
        if now < observed.until_ms() {
            return Err(durable::Error::Cooldown(observed.until_ms()));
        }
        if held.claimed.is_some() || held.completion.is_some() || held.read.is_some() {
            return Err(durable::Error::Pending);
        }
        let route = self
            .owner
            .account
            .owned
            .as_ref()
            .ok_or(durable::Error::Unsupported)?;
        // This explicit later action creates one fresh reservation only after
        // the prior failed family was observed and acknowledged. The executor
        // still checks this full original Review and actual current Stream.
        held.reserved = Some(route.operations.baseline(o::BaselineInput::new(
            self.source.stream(&current)?,
            self.expected.clone(),
            now,
        )));
        Ok(())
    }

    fn observe_cooldown(&self, until: u64) -> Result<(), durable::Error> {
        let mut held = self.held.lock().map_err(|_| durable::Error::Storage)?;
        if held.claimed.is_some() || held.completion.is_some() || held.read.is_some() {
            return Err(durable::Error::Stale);
        }
        // Caller holds this Read's gate across the actual wait. Retrieve the
        // retained reservation itself; opaque worker keys are not public IDs.
        let reserved = held
            .reserved
            .as_ref()
            .ok_or(durable::Error::Stale)?
            .as_ref()
            .map_err(|r| ui(r.reason))?;
        let (observed, acknowledgment) = reserved.observe_cooldown().map_err(ui)?;
        if observed.until_ms() != until {
            return Err(durable::Error::Stale);
        }
        // No request was issued. Keep the actual error/deadline attribution,
        // then release precisely the root and its two reserved placeholders.
        held.cooldown = Some(observed);
        acknowledgment.acknowledge().map_err(|(e, _)| ui(e))?;
        held.reserved = None;
        Ok(())
    }

    async fn finish(
        &self,
        shell: Signal<ShellState>,
        now: fn() -> u64,
        completion: o::BaselineCompletion,
    ) -> Result<BaselineRun, durable::Error> {
        match completion.state().map_err(ui)? {
            o::BaselineState::Deferred(until) => return Ok(BaselineRun::Deferred { until }),
            o::BaselineState::ClaimPending { .. } => return Err(durable::Error::Pending),
            o::BaselineState::Failed(error) => return Err(error),
            o::BaselineState::Disconnected => return Err(durable::Error::Storage),
            o::BaselineState::Ready => {
                #[cfg(feature = "protected-native-fixture")]
                if super::production::baseline_native::named() {
                    let current = shell.try_peek().map_err(|_| durable::Error::Stale)?;
                    if !self.current(&current) {
                        return Err(durable::Error::Gap);
                    }
                    let read =
                        super::production::baseline_native::read(&current, &completion, now())?;
                    self.held.lock().map_err(|_| durable::Error::Storage)?.read =
                        Some(Arc::new(read));
                } else {
                    self.transport_read_once(shell, now, &completion).await?;
                }
                #[cfg(not(feature = "protected-native-fixture"))]
                self.transport_read_once(shell, now, &completion).await?;
            }
            o::BaselineState::Requested | o::BaselineState::ResponseRetained => {}
        }
        self.retain_and_wait(completion).await
    }
    async fn transport_read_once(
        &self,
        shell: Signal<ShellState>,
        now: fn() -> u64,
        completion: &o::BaselineCompletion,
    ) -> Result<(), durable::Error> {
        let request = completion.take_request().map_err(ui)?;
        let read = Arc::new(if shell.try_peek().is_ok_and(|s| self.current(&s)) {
            self.binding
                .read_protected_baseline(request, now(), || {
                    shell.try_peek().is_ok_and(|s| self.current(&s))
                })
                .await
        } else {
            BaselineRead::not_requested(None)
        });
        self.held.lock().map_err(|_| durable::Error::Storage)?.read = Some(read);
        Ok(())
    }
    async fn retain_and_wait(
        &self,
        completion: o::BaselineCompletion,
    ) -> Result<BaselineRun, durable::Error> {
        let read = self
            .held
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .read
            .clone()
            .ok_or(durable::Error::Pending)?;
        if completion.retained_read().is_none() {
            completion
                .bind(o::BaselineRead {
                    response: read.response.clone(),
                    known_until: read.cooldown_until_ms.unwrap_or(0),
                })
                .map_err(|r| ui(r.reason))?;
        }
        completion.submit().map_err(ui)?;
        self.wait_result(completion).await
    }
    async fn wait_result(
        &self,
        completion: o::BaselineCompletion,
    ) -> Result<BaselineRun, durable::Error> {
        let waiting = completion.clone();
        let outcome = tokio::task::spawn_blocking(move || waiting.wait())
            .await
            .map_err(|_| durable::Error::Storage)?
            .map_err(ui)??;
        let mut held = self.held.lock().map_err(|_| durable::Error::Storage)?;
        held.outcome = Some(outcome);
        if let Ok((observed, ack)) = completion.observe_success() {
            if held.outcome.as_ref().is_some_and(|v| ack.matches(v)) {
                held.outcome = Some(observed);
                held.acknowledgment = Some(ReadAcknowledgment::Clean(ack));
            }
        } else if let Ok((observed, reports, ack)) = completion.observe_repaired_success() {
            if held.outcome.as_ref().is_some_and(|v| ack.matches(v)) {
                held.outcome = Some(observed);
                held.acknowledgment = Some(ReadAcknowledgment::Repaired(reports, ack));
            }
        }
        drop(held);
        self.result()?.ok_or(durable::Error::Pending)
    }
    fn result(&self) -> Result<Option<BaselineRun>, durable::Error> {
        let held = self.held.lock().map_err(|_| durable::Error::Storage)?;
        let Some(outcome) = &held.outcome else {
            return Ok(None);
        };
        Ok(Some(match outcome.evidence() {
            Some(evidence) => BaselineRun::Evidence(Evidence {
                evidence,
                binding: self.binding.clone(),
                source: self.source.clone(),
            }),
            None => BaselineRun::Nonpositive(held.read.clone().ok_or(durable::Error::Stale)?),
        }))
    }
    /// Called only after the actual result was retained by its host consumer.
    pub(super) fn acknowledge(&self) -> Result<(), durable::Error> {
        let mut journal = self
            .owner
            .account
            .journal
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        if !journal
            .owned_baselines
            .get(&self.owner.scope.key)
            .is_some_and(|r| std::ptr::eq(r.as_ref(), self))
        {
            return Err(durable::Error::Stale);
        }
        let mut held = self.held.lock().map_err(|_| durable::Error::Storage)?;
        let ack = held.acknowledgment.take().ok_or(durable::Error::Pending)?;
        match ack.acknowledge() {
            Ok(()) => {
                held.acknowledged = true;
                journal.owned_baselines.remove(&self.owner.scope.key);
                Ok(())
            }
            Err((error, ack)) => {
                held.acknowledgment = Some(ack);
                Err(ui(error))
            }
        }
    }
    /// Explicit same-ticket resume; never schedule a timer or a new reservation.
    pub(super) async fn resume(
        self: &Arc<Self>,
        shell: Signal<ShellState>,
        now: fn() -> u64,
    ) -> Result<BaselineRun, durable::Error> {
        let read = self.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        dioxus::core::spawn_forever(async move {
            let result = async {
                let _gate = read.gate.lock().await;
                let completion = read
                    .held
                    .lock()
                    .map_err(|_| durable::Error::Storage)?
                    .completion
                    .clone()
                    .ok_or(durable::Error::Pending)?;
                if !shell.try_peek().is_ok_and(|s| read.current(&s)) {
                    return Err(durable::Error::Gap);
                }
                let resume_at = match completion.state().map_err(ui)? {
                    o::BaselineState::ClaimPending { now_ms, .. } => now_ms,
                    o::BaselineState::Deferred(_) => now(),
                    _ => return Err(durable::Error::Stale),
                };
                completion.resume_deferred(resume_at).map_err(ui)?;
                let waiting = completion.clone();
                let until = tokio::task::spawn_blocking(move || waiting.wait_deferred())
                    .await
                    .map_err(|_| durable::Error::Storage)?
                    .map_err(ui)??;
                if let Some(until) = until {
                    return Ok(BaselineRun::Deferred { until });
                }
                read.finish(shell, now, completion).await
            }
            .await;
            let _ = send.send(result);
        });
        receive.await.map_err(|_| durable::Error::Storage)?
    }
    pub(super) async fn retry_storage(self: &Arc<Self>) -> Result<BaselineRun, durable::Error> {
        let read = self.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        dioxus::core::spawn_forever(async move {
            let result = async {
                let _gate = read.gate.lock().await;
                let completion = read
                    .held
                    .lock()
                    .map_err(|_| durable::Error::Storage)?
                    .completion
                    .clone()
                    .ok_or(durable::Error::Pending)?;
                completion.retry_storage().map_err(ui)?;
                read.wait_result(completion).await
            }
            .await;
            let _ = send.send(result);
        });
        receive.await.map_err(|_| durable::Error::Storage)?
    }
}
fn ready(state: o::JobState) -> Result<(), durable::Error> {
    match state {
        o::JobState::Ready => Ok(()),
        o::JobState::Failed(error) => Err(error),
        _ => Err(durable::Error::Pending),
    }
}

/// Exact human choice retained by Editor before final enqueue. A retry uses
/// this same typed job and evidence, never another RemoteChoice allocation.
pub(super) struct Finish {
    held: Arc<owned_route::Held<o::BaselineFinishInput, durable::Review>>,
    repaired: ownership::Tracked<Option<o::BaselineRepairReceipt>>,
    acknowledged: ownership::Tracked<Option<durable::Review>>,
}
impl Finish {
    pub fn new(
        route: &owned_route::Route,
        evidence: durable::BaselineEvidence,
        choice: super_platinum_core::drafts::store::sync::RemoteChoice,
    ) -> Arc<Self> {
        Arc::new(Self {
            held: owned_route::Held::new(
                route
                    .operations
                    .finish_baseline(o::BaselineFinishInput::new(evidence, choice)),
            ),
            repaired: ownership::Tracked::new(None),
            acknowledged: ownership::Tracked::new(None),
        })
    }
    pub fn submit(&self) -> Result<(), durable::Error> {
        self.held.submit().map_err(ui)
    }
    pub async fn wait(&self) -> Result<durable::Review, durable::Error> {
        self.held.wait().await.map_err(ui)?;
        let job = self.held.job().map_err(ui)?;
        let result = job.result().ok_or(durable::Error::Pending)??;
        if let Ok((actual, reports, ack)) = job.observe_repaired_success() {
            if !actual.same(&result) || !ack.matches(&result) {
                return Err(durable::Error::Stale);
            }
            *self.repaired.lock().map_err(|_| durable::Error::Storage)? = Some(reports);
        }
        Ok(result)
    }
    pub fn retry(&self) -> Result<(), durable::Error> {
        self.held.job().map_err(ui)?.retry_storage().map_err(ui)
    }
    pub fn acknowledge(&self, after: &durable::Review) -> Result<(), durable::Error> {
        let mut acknowledged = self
            .acknowledged
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        if let Some(actual) = acknowledged.as_ref() {
            return if actual.same(after) {
                Ok(())
            } else {
                Err(durable::Error::Stale)
            };
        }
        self.acknowledge_once(after)?;
        *acknowledged = Some(after.clone());
        Ok(())
    }
    fn acknowledge_once(&self, after: &durable::Review) -> Result<(), durable::Error> {
        let job = self.held.job().map_err(ui)?;
        let repaired = self.repaired.lock().map_err(|_| durable::Error::Storage)?;
        if let Some(observed) = repaired.as_ref() {
            let (actual, reports, receipt) = job.observe_repaired_success().map_err(ui)?;
            if !actual.same(after)
                || !receipt.matches(after)
                || reports.failed_submissions() != observed.failed_submissions()
                || reports.successful_submission() != observed.successful_submission()
            {
                return Err(durable::Error::Stale);
            }
            receipt.acknowledge().map_err(|(error, _)| ui(error))
        } else {
            let (actual, receipt) = job.observe_success().map_err(ui)?;
            if !actual.same(after) {
                return Err(durable::Error::Stale);
            }
            receipt.acknowledge().map_err(|(error, _)| ui(error))
        }
    }
}
