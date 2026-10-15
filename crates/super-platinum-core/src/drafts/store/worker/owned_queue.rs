//! Inactive fixture route: one real Worker, owner lock, and retained migration jobs.
//! No Default opener, legacy admission, Current-mode permit, or drain certificate.
use super::super::protected::{Error, commands::Command as ProtectedCommand, upgrade};
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[path = "owned_initialization.rs"]
pub(in crate::drafts::store) mod initialization;

const MAX_JOBS: usize = 16;
type Outcome = Result<Arc<upgrade::Reply>, Error>;
struct JobState {
    receiver: Option<mpsc::Receiver<Result<upgrade::Reply, AppError>>>,
    outcome: Option<Outcome>,
}
pub(in crate::drafts::store) struct Job {
    commit: bool,
    state: Mutex<JobState>,
}
impl Job {
    fn new(receiver: mpsc::Receiver<Result<upgrade::Reply, AppError>>, commit: bool) -> Arc<Self> {
        Arc::new(Self {
            commit,
            state: Mutex::new(JobState {
                receiver: Some(receiver),
                outcome: None,
            }),
        })
    }
    pub(in crate::drafts::store) fn poll(&self) -> Result<Option<Arc<upgrade::Reply>>, Error> {
        let mut state = match self.state.try_lock() {
            Ok(state) => state,
            Err(std::sync::TryLockError::WouldBlock) => return Ok(None),
            Err(_) => return Err(Error::Storage),
        };
        if let Some(outcome) = &state.outcome {
            return outcome.clone().map(Some);
        }
        let result = match state.receiver.as_ref().ok_or(Error::Storage)?.try_recv() {
            Ok(result) => result.map(Arc::new).map_err(|_| Error::Storage),
            Err(mpsc::TryRecvError::Empty) => return Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(Error::Storage),
        };
        state.receiver = None;
        state.outcome = Some(result.clone());
        result.map(Some)
    }
    // Blocking by design, like protected::Reply::wait. Never call on renderer.
    pub(in crate::drafts::store) fn wait(&self) -> Outcome {
        let mut state = self.state.lock().map_err(|_| Error::Storage)?;
        if let Some(outcome) = &state.outcome {
            return outcome.clone();
        }
        let result = state
            .receiver
            .as_ref()
            .ok_or(Error::Storage)?
            .recv()
            .map_err(|_| Error::Storage)
            .and_then(|reply| reply.map(Arc::new).map_err(|_| Error::Storage));
        state.receiver = None;
        state.outcome = Some(result.clone());
        result
    }
}
#[derive(Default)]
struct Control {
    jobs: Vec<Arc<Job>>,
    active: Option<Arc<Job>>,
}
pub(crate) struct Queue {
    worker: Arc<Worker>,
    owner: Arc<upgrade::Owned>,
    proof: Arc<OnceLock<Arc<upgrade::Review>>>,
    control: Mutex<Control>,
    initialization: Mutex<initialization::Control>,
    stopping: Arc<AtomicBool>,
    alive: Arc<Mutex<bool>>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}
struct WorkerExit {
    worker: Arc<Worker>,
    alive: Arc<Mutex<bool>>,
}
impl Drop for WorkerExit {
    fn drop(&mut self) {
        // Serialize against submission. A terminated worker must not retain
        // unreachable senders or accept new work that can never complete.
        let mut alive = self.alive.lock().unwrap();
        *alive = false;
        let mut pending = self.worker.pending.lock().unwrap();
        if let Some(book) = &mut pending.accounting {
            book.abandon();
        }
        if let Some(admission) = &mut pending.admission {
            admission.dead = true;
        }
        pending.requests.clear();
        self.worker.accounted.notify_all();
    }
}
impl Queue {
    pub(in crate::drafts::store) fn fresh() -> Result<Arc<Self>, Error> {
        Self::start(upgrade::Owned::fresh().map_err(|_| Error::Storage)?)
    }
    pub(in crate::drafts::store) fn reopen(ticket: &str) -> Result<Arc<Self>, Error> {
        Self::start(upgrade::Owned::reopen_fixture(ticket).map_err(|_| Error::Storage)?)
    }
    fn start(owner: Arc<upgrade::Owned>) -> Result<Arc<Self>, Error> {
        let worker = Arc::new(Worker {
            protected_fixture: true,
            ..Worker::default()
        });
        let id = worker
            .upgrade_id
            .get_or_init(|| uuid::Uuid::new_v4().to_string());
        owner.bind_worker(id).map_err(|_| Error::Storage)?;
        worker.pending.lock().unwrap().accounting =
            Some(accounting::Book::owned(owner.account(), id));
        worker.pending.lock().unwrap().admission = Some(admission::State::new(owner.clone(), id));
        let proof: Arc<OnceLock<Arc<upgrade::Review>>> = Arc::new(OnceLock::new());
        let stopping = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(Mutex::new(true));
        let lifetime = WorkerExit {
            worker: worker.clone(),
            alive: alive.clone(),
        };
        let run = worker.clone();
        let retained = owner.clone();
        let opened = proof.clone();
        let stop = stopping.clone();
        let thread = std::thread::Builder::new()
            .name("protected-owned-fixture".into())
            .spawn(move || {
                let _lifetime = lifetime;
                loop {
                    {
                        let mut pending = run.pending.lock().unwrap();
                        while pending.requests.is_empty()
                            && pending.updates.is_empty()
                            && !stop.load(Ordering::Acquire)
                        {
                            pending = run.ready.wait(pending).unwrap();
                        }
                        if stop.load(Ordering::Acquire)
                            && pending.requests.is_empty()
                            && pending.updates.is_empty()
                        {
                            break;
                        }
                    }
                    run.cycle(|account| {
                        if account != retained.account() {
                            return Err(sync::invalid("owned route account changed"));
                        }
                        let proof = opened
                            .get()
                            .ok_or_else(|| sync::invalid("protected handoff unavailable"))?;
                        proof.open_owned(&retained, run.upgrade_id.get().unwrap())
                    });
                }
            })
            .map_err(|_| Error::Storage)?;
        Ok(Arc::new(Self {
            worker,
            owner,
            proof,
            control: Mutex::new(Control::default()),
            initialization: Mutex::new(initialization::Control::default()),
            stopping,
            alive,
            thread: Mutex::new(Some(thread)),
        }))
    }
    pub(super) fn diagnostics(&self) -> Result<accounting::Snapshot, Error> {
        let pending = self.worker.pending.lock().map_err(|_| Error::Storage)?;
        let book = pending.accounting.as_ref().ok_or(Error::Stale)?;
        Ok(book.snapshot(&pending))
    }
    pub(in crate::drafts::store) fn account(&self) -> &str {
        self.owner.account()
    }
    pub(in crate::drafts::store) fn worker_id(&self) -> &str {
        self.worker.upgrade_id.get().unwrap()
    }
    pub(in crate::drafts::store) fn opened(&self) -> Option<Arc<upgrade::Review>> {
        self.proof.get().cloned()
    }
    pub(in crate::drafts::store) fn retained_job(
        &self,
        index: usize,
        commit: bool,
    ) -> Result<Arc<Job>, Error> {
        let control = self.control.lock().map_err(|_| Error::Storage)?;
        let job = control.jobs.get(index).ok_or(Error::Stale)?;
        if job.commit != commit {
            return Err(Error::Stale);
        }
        Ok(job.clone())
    }
    pub(in crate::drafts::store) fn job_id(&self, job: &Arc<Job>) -> usize {
        self.control
            .lock()
            .unwrap()
            .jobs
            .iter()
            .position(|retained| Arc::ptr_eq(retained, job))
            .unwrap()
    }
    fn idle(control: &mut Control) -> Result<(), Error> {
        if let Some(active) = &control.active {
            match active.poll() {
                Ok(None) => return Err(Error::Pending),
                Ok(Some(_)) | Err(_) => control.active = None,
            }
        }
        Ok(())
    }
    pub(in crate::drafts::store) fn submit_upgrade(
        &self,
        action: upgrade::Action,
    ) -> Result<Arc<Job>, Error> {
        self.submit_configured(action, |_| {})
    }
    fn submit_configured(
        &self,
        action: upgrade::Action,
        configure: impl FnOnce(&mut upgrade::Command),
    ) -> Result<Arc<Job>, Error> {
        let mut control = self.control.lock().map_err(|_| Error::Storage)?;
        Self::idle(&mut control)?;
        if self.proof.get().is_some() {
            return Err(Error::Stale);
        }
        if control.jobs.len() >= MAX_JOBS {
            return Err(Error::Pending);
        }
        let commit = matches!(action, upgrade::Action::Commit(_));
        let (mut command, receiver) = upgrade::Command::new(self.owner.clone(), action);
        configure(&mut command);
        let job = Job::new(receiver, commit);
        // The route retains both the buffered result and its receiver, regardless
        // of dropped UI handles. There is no auto-retry or replacement job.
        control.jobs.push(job.clone());
        control.active = Some(job.clone());
        self.submit(Request::StoreUpgrade(Box::new(command)))?;
        Ok(job)
    }
    pub(in crate::drafts::store) fn open(&self, review: Arc<upgrade::Review>) -> Result<(), Error> {
        let mut control = self.control.lock().map_err(|_| Error::Storage)?;
        Self::idle(&mut control)?;
        if let Some(actual) = self.proof.get() {
            // Reattach to this exact completed handoff; never inspect, enqueue,
            // advance the generation or issue a fresh inventory permission.
            return if Arc::ptr_eq(actual, &review) {
                Ok(())
            } else {
                Err(Error::Stale)
            };
        }
        if control.jobs.len() >= MAX_JOBS {
            return Err(Error::Pending);
        }
        let (command, receiver) =
            upgrade::Command::new(self.owner.clone(), upgrade::Action::Handoff(review.clone()));
        let job = Job::new(receiver, false);
        control.jobs.push(job.clone());
        control.active = Some(job.clone());
        self.submit(Request::StoreUpgrade(Box::new(command)))?;
        // The actual Worker performs the final raw-inventory/file/owner/fence
        // validation. Keep this short route-admission serialization through its
        // completion; callers must run this blocking method off the renderer.
        let reply = job.wait()?;
        let upgrade::Reply::Opened(actual) = &*reply else {
            return Err(Error::Stale);
        };
        if !Arc::ptr_eq(&review, actual) {
            return Err(Error::Stale);
        }
        self.proof.set(actual.clone()).map_err(|_| Error::Stale)
    }
    pub(in crate::drafts::store) fn ticket(&self) -> Result<String, Error> {
        self.proof
            .get()
            .ok_or(Error::Stale)?
            .fixture_ticket()
            .map_err(|_| Error::Storage)
    }
    pub(super) fn submit_protected(&self, command: ProtectedCommand) {
        self.admit_protected(command);
    }
    fn submit(&self, request: Request) -> Result<(), Error> {
        let alive = self.alive.lock().map_err(|_| Error::Storage)?;
        if !*alive {
            return Err(Error::Storage);
        }
        self.worker.submit_bootstrap(request);
        Ok(())
    }
}
impl Drop for Queue {
    fn drop(&mut self) {
        if let Some(admission) = &mut self.worker.pending.lock().unwrap().admission {
            admission.dead = true;
        }
        self.stopping.store(true, Ordering::Release);
        self.worker.ready.notify_all();
        self.worker.accounted.notify_all();
        if let Some(thread) = self.thread.lock().unwrap().take() {
            let _ = thread.join();
        }
        // Join precedes release of the marker and retained file. No public close
        // certificate is issued: existing Arc owners must actually be dropped.
    }
}

#[cfg(test)]
#[path = "owned_queue_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "owned_initialization_tests.rs"]
mod initialization_tests;

#[cfg(test)]
#[path = "admission_tests.rs"]
mod admission_tests;

#[cfg(test)]
#[path = "completion_tests.rs"]
mod completion_tests;

#[cfg(test)]
#[path = "local_bridge_tests.rs"]
mod local_bridge_tests;

#[cfg(test)]
#[path = "slash_execution_tests.rs"]
mod slash_execution_tests;

#[path = "owned_operations.rs"]
pub(in crate::drafts::store) mod operations;
