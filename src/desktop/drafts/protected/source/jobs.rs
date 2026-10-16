//! Retained pre-Source work; no complete account, drain or Current certificate.
use super::super::ownership::{self, Tracked};
use super::*;
use std::sync::Weak;
use tokio::sync::oneshot;
mod gap;
mod reply;
use reply::ReplyJob;
const MAX_JOBS: usize = 64;

#[derive(Default)]
pub(in super::super) struct Book {
    next: u64,
    entries: std::collections::BTreeMap<u64, Arc<Job>>,
}
pub(crate) struct Request {
    pub state: dioxus::prelude::Signal<ShellState>,
    pub account: Arc<Account>,
    pub socket: Arc<observation::Source>,
    pub frame_generation: u64,
    pub team: String,
}
pub(crate) struct Refused {
    pub request: Request,
    pub error: durable::Error,
}
pub(crate) struct Opening {
    job: Arc<Job>,
    receive: oneshot::Receiver<Result<Arc<Source>, durable::Error>>,
}
pub(in super::super) struct Job {
    account: Weak<Account>,
    socket: Arc<observation::Source>,
    frame_generation: u64,
    team: String,
    state: Tracked<State>,
    gap: Arc<gap::Gap>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Reserved,
    Initializing,
    Reading,
    Opening,
    Delivering,
    Delivered,
    Abandoned,
    Failed,
}
struct State {
    phase: Phase,
    abandoned: bool,
    read: Option<Arc<ReplyJob<durable::StreamState>>>,
    open: Option<Arc<ReplyJob<durable::Stream>>>,
    issued: Option<durable::Stream>,
    source: Option<(Weak<Source>, inventory::Handle)>,
    error: Option<durable::Error>,
}
impl Request {
    pub(crate) fn begin(self) -> Result<Opening, Refused> {
        let runtime = match tokio::runtime::Handle::try_current() {
            Ok(runtime) => runtime,
            Err(_) => {
                return Err(Refused {
                    request: self,
                    error: durable::Error::Storage,
                });
            }
        };
        let job = match Job::reserve(
            &self.account,
            self.socket.clone(),
            self.frame_generation,
            &self.team,
            runtime.clone(),
        ) {
            Ok(job) => job,
            Err(error) => {
                return Err(Refused {
                    request: self,
                    error,
                });
            }
        };
        let (send, receive) = oneshot::channel();
        let guard = RunGuard {
            job: job.clone(),
            finished: false,
        };
        // Construct the guard before spawning: even an unpolled task cannot
        // disappear from ownership inventory as though it never existed.
        dioxus::core::spawn_forever(async move {
            let mut guard = guard;
            let result = guard.job.run(self.state, self.account, &runtime).await;
            if let Err(error) = &result {
                let mut state = guard.job.state.lock().unwrap();
                state.error = Some(*error);
                state.phase = Phase::Failed;
            }
            // Actual strong Source belongs only to this task or the external
            // receiver, never Account's Job. There is no await during transfer.
            match send.send(result) {
                Ok(()) => guard.job.state.lock().unwrap().phase = Phase::Delivered,
                Err(result) => {
                    guard.job.abandon();
                    drop(result);
                }
            }
            guard.finished = true;
        });
        Ok(Opening { job, receive })
    }
}
impl Opening {
    pub(crate) async fn receive(self) -> Result<Arc<Source>, durable::Error> {
        self.receive.await.map_err(|_| durable::Error::Storage)?
    }
}
struct RunGuard {
    job: Arc<Job>,
    finished: bool,
}
impl Drop for RunGuard {
    fn drop(&mut self) {
        if !self.finished {
            self.job.abandon();
        }
    }
}
impl Job {
    fn reserve(
        account: &Arc<Account>,
        socket: Arc<observation::Source>,
        frame_generation: u64,
        team: &str,
        runtime: tokio::runtime::Handle,
    ) -> Result<Arc<Self>, durable::Error> {
        let mut book = account
            .source_jobs
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        if book.entries.len() >= MAX_JOBS || book.next == u64::MAX || team.len() > 4096 {
            return Err(durable::Error::Pending);
        }
        let job = Arc::new(Self {
            account: Arc::downgrade(account),
            socket,
            frame_generation,
            team: team.to_owned(),
            state: Tracked::new(State {
                phase: Phase::Reserved,
                abandoned: false,
                read: None,
                open: None,
                issued: None,
                source: None,
                error: None,
            }),
            gap: gap::Gap::new(account, runtime),
        });
        book.next += 1;
        let id = book.next;
        book.entries.insert(id, job.clone());
        Ok(job)
    }
    fn current(&self, state: &dioxus::prelude::Signal<ShellState>, account: &Arc<Account>) -> bool {
        use dioxus::prelude::ReadableExt;
        self.account
            .upgrade()
            .is_some_and(|owner| Arc::ptr_eq(&owner, account))
            && !account.team_blocked(&self.team)
            && state.try_peek().is_ok_and(|shell| {
                shell.core.active_account.as_deref() == Some(account.id())
                    && self.socket.matches_owner(account.id(), &self.team)
                    && self.socket.current(&shell, self.frame_generation)
            })
    }
    async fn run(
        self: &Arc<Self>,
        state: dioxus::prelude::Signal<ShellState>,
        account: Arc<Account>,
        runtime: &tokio::runtime::Handle,
    ) -> Result<Arc<Source>, durable::Error> {
        use dioxus::prelude::ReadableExt;
        self.state.lock().unwrap().phase = Phase::Initializing;
        account.initialize().await?;
        if !self.current(&state, &account) {
            return Err(durable::Error::Gap);
        }
        let read = ReplyJob::new(account.client()?.stream_state(self.team.clone()));
        {
            let mut held = self.state.lock().unwrap();
            held.read = Some(read.clone());
            held.phase = Phase::Reading;
        }
        read.start(runtime, |_| ())?;
        let expected = read.ready().await?;
        if !self.current(&state, &account) {
            return Err(durable::Error::Gap);
        }
        let open = ReplyJob::new(account.client()?.open_stream(self.team.clone(), expected));
        {
            let mut held = self.state.lock().unwrap();
            held.open = Some(open.clone());
            held.phase = Phase::Opening;
        }
        let weak = Arc::downgrade(self);
        // Account already crosses the existing mark_gap Tokio boundary. Keep
        // it alive until the actual late Reply is recorded/cleaned up; no
        // Signal, renderer future or !Sync execution Evidence enters this task.
        let retained_account = account.clone();
        open.start(runtime, move |result| {
            if let (Some(job), Ok(stream)) = (weak.upgrade(), result) {
                job.issued(stream);
            }
            drop(retained_account);
        })?;
        let stream = open.ready().await?;
        let shell = match state.try_peek() {
            Ok(shell) => shell,
            Err(_) => {
                self.retire(stream, durable::GapReason::SourceReplaced);
                return Err(durable::Error::Gap);
            }
        };
        let source = Source::captured_with_job(
            account,
            self.socket.clone(),
            self.frame_generation,
            stream,
            &shell,
            Some(self.clone()),
        )?;
        {
            let mut held = self.state.lock().unwrap();
            held.source = Some((Arc::downgrade(&source), source.ownership()));
            held.phase = Phase::Delivering;
        }
        Ok(source)
    }
    fn issued(self: &Arc<Self>, stream: durable::Stream) {
        let abandoned = {
            let mut state = self.state.lock().unwrap();
            state.issued = Some(stream.clone());
            state.abandoned
        };
        if abandoned {
            self.retire(stream, durable::GapReason::InterruptedAdmission);
        }
    }
    fn abandon(self: &Arc<Self>) {
        let stream = {
            let mut state = self.state.lock().unwrap();
            state.abandoned = true;
            state.phase = Phase::Abandoned;
            state.issued.clone()
        };
        if let Some(stream) = stream {
            self.retire(stream, durable::GapReason::InterruptedAdmission);
        }
    }
    pub(super) fn retire(self: &Arc<Self>, stream: durable::Stream, reason: durable::GapReason) {
        self.gap.retire(stream, reason);
    }
    pub(super) fn retry_gap(self: &Arc<Self>) -> Result<(), durable::Error> {
        self.gap.submit()
    }
}

#[derive(Clone)]
pub(crate) struct Handle {
    account: Weak<Account>,
    id: u64,
    job: Arc<Job>,
}
impl Account {
    pub(in super::super) fn retained_gap(
        self: &Arc<Self>,
        team: &str,
        generation: u64,
    ) -> Result<Option<Handle>, durable::Error> {
        Ok(self
            .retained_source_openings()?
            .into_iter()
            .find(|h| h.job.gap.generation(team) == Some(generation)))
    }
    pub(crate) fn retained_source_openings(
        self: &Arc<Self>,
    ) -> Result<Vec<Handle>, durable::Error> {
        Ok(self
            .source_jobs
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .entries
            .iter()
            .map(|(id, job)| Handle {
                account: Arc::downgrade(self),
                id: *id,
                job: job.clone(),
            })
            .collect())
    }
    pub(crate) fn acknowledge_source_opening(
        self: &Arc<Self>,
        handle: &Handle,
    ) -> Result<(), durable::Error> {
        if !handle
            .account
            .upgrade()
            .is_some_and(|a| Arc::ptr_eq(&a, self))
        {
            return Err(durable::Error::Stale);
        }
        let mut book = self
            .source_jobs
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        if !book
            .entries
            .get(&handle.id)
            .is_some_and(|j| Arc::ptr_eq(j, &handle.job))
        {
            return Err(durable::Error::Stale);
        }
        if !handle.job.reclaimable()? {
            return Err(durable::Error::Pending);
        }
        book.entries.remove(&handle.id);
        Ok(())
    }
}
impl Handle {
    /// Historical successful delivery cannot recreate a retired Source.
    pub(crate) fn source(&self) -> Result<Arc<Source>, durable::Error> {
        let state = self.job.state.lock().map_err(|_| durable::Error::Storage)?;
        if let Some(error) = state.error {
            return Err(error);
        }
        let source = state
            .source
            .as_ref()
            .ok_or(durable::Error::Pending)?
            .0
            .upgrade()
            .ok_or(durable::Error::Gap)?;
        source.live().then_some(source).ok_or(durable::Error::Gap)
    }
    pub(crate) fn retry_gap(&self) -> Result<(), durable::Error> {
        self.job.retry_gap()
    }
}
impl Job {
    fn reclaimable(&self) -> Result<bool, durable::Error> {
        let state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if !matches!(
            state.phase,
            Phase::Delivered | Phase::Abandoned | Phase::Failed
        ) || state
            .source
            .as_ref()
            .is_some_and(|(source, _)| source.upgrade().is_some())
        {
            return Ok(false);
        }
        if state
            .read
            .as_ref()
            .is_some_and(|r| !matches!(r.result(), Ok(Some(_))))
            || state
                .open
                .as_ref()
                .is_some_and(|r| !matches!(r.result(), Ok(Some(_))))
        {
            return Ok(false);
        }
        if state.issued.is_some() {
            return self.gap.terminal();
        }
        Ok(true)
    }
}

/// Sampled ownership only. Neither empty membership nor successful validation
/// attests unrelated direct captures, initialization, legacy gap calls or drain.
pub(crate) struct Snapshot {
    account: Weak<Account>,
    book: ownership::Stamp,
    jobs: Vec<(Arc<Job>, JobSnapshot)>,
}
struct JobSnapshot {
    state: ownership::Stamp,
    read: Option<(Arc<ReplyJob<durable::StreamState>>, ownership::Stamp)>,
    open: Option<(Arc<ReplyJob<durable::Stream>>, ownership::Stamp)>,
    gap: gap::Snapshot,
    source: Option<(Weak<Source>, bool)>,
}
impl Account {
    pub(crate) fn source_jobs_inventory(self: &Arc<Self>) -> Result<Snapshot, ownership::Error> {
        let (book, jobs) = self
            .source_jobs
            .observe(|b| b.entries.values().cloned().collect::<Vec<_>>())?;
        let jobs = jobs
            .into_iter()
            .map(|job| Ok((job.clone(), job.snapshot()?)))
            .collect::<Result<_, ownership::Error>>()?;
        let snapshot = Snapshot {
            account: Arc::downgrade(self),
            book,
            jobs,
        };
        snapshot.validate(self)?;
        Ok(snapshot)
    }
}
impl Job {
    fn snapshot(&self) -> Result<JobSnapshot, ownership::Error> {
        let (state, (read, open, source)) = self.state.observe(|s| {
            (
                s.read.clone(),
                s.open.clone(),
                s.source.as_ref().map(|(v, _)| v.clone()),
            )
        })?;
        let read = read.map(|v| Ok((v.clone(), v.stamp()?))).transpose()?;
        let open = open.map(|v| Ok((v.clone(), v.stamp()?))).transpose()?;
        let source = source.map(|v| {
            let live = v.upgrade().is_some();
            (v, live)
        });
        let snapshot = JobSnapshot {
            state,
            read,
            open,
            source,
            gap: self.gap.snapshot()?,
        };
        self.validate(&snapshot)?;
        Ok(snapshot)
    }
    fn validate(&self, snapshot: &JobSnapshot) -> Result<(), ownership::Error> {
        self.state.validate(&snapshot.state)?;
        if let Some((v, s)) = &snapshot.read {
            v.validate(s)?;
        }
        if let Some((v, s)) = &snapshot.open {
            v.validate(s)?;
        }
        self.gap.validate(&snapshot.gap)?;
        if snapshot
            .source
            .as_ref()
            .is_some_and(|(v, live)| v.upgrade().is_some() != *live)
        {
            return Err(ownership::Error::Changed);
        }
        self.state.validate(&snapshot.state)
    }
}
impl Snapshot {
    pub(crate) fn validate(&self, account: &Arc<Account>) -> Result<(), ownership::Error> {
        if !self
            .account
            .upgrade()
            .is_some_and(|v| Arc::ptr_eq(&v, account))
        {
            return Err(ownership::Error::Replaced);
        }
        account.source_jobs.validate(&self.book)?;
        for (job, snapshot) in &self.jobs {
            job.validate(snapshot)?;
        }
        account.source_jobs.validate(&self.book)
    }
}

#[cfg(test)]
mod tests;
