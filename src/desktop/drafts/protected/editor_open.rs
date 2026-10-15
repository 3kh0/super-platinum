//! Retained Editor-open ownership only. No queue fence or continuation authority.
use super::*;
use std::sync::{Mutex, Weak};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Reserved,
    Initializing,
    WaitingOwner,
    Reading,
    Registering,
    Installing,
    Finished,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Terminal {
    Running,
    Installed,
    Failed(durable::Error),
    Unknown,
}
pub(super) struct Evidence {
    pub phase: Phase,
    pub terminal: Terminal,
    // Preserve actual evidence even when installation or the waiter disappears.
    pub review: Option<Arc<durable::Review>>,
    pub writer: Option<durable::Writer>,
    editor: Option<Weak<editor::Editor>>,
}
pub(super) struct Job {
    pub id: String,
    pub scope: Scope,
    pub expected: Option<durable::Review>,
    account: Weak<Account>,
    pub evidence: Mutex<Evidence>,
    changed: tokio::sync::Notify,
}
#[derive(Clone)]
pub(super) struct Open(Arc<Job>);
pub(super) struct Request {
    pub owner: Owner,
    pub expected: Option<durable::Review>,
}
pub(super) struct Refused {
    pub error: durable::Error,
    pub request: Request,
}
// Created before spawn, not inside the first poll of the task.
struct Completion {
    account: Arc<Account>,
    job: Arc<Job>,
    finished: bool,
}
impl Request {
    /// Refusal returns the ORIGINAL request by move, before starting any work.
    /// Existing public convenience APIs retain their Result signature; this
    /// private boundary is the future host-owned refusal attachment point.
    pub fn begin(self) -> Result<Open, Refused> {
        let (owner, completion, runtime) = self.reserve()?;
        let opened = Open(completion.job.clone());
        let task_runtime = runtime.clone();
        runtime.spawn(Box::pin(async move {
            completion.run(owner, task_runtime).await;
        }));
        Ok(opened)
    }
    fn reserve(self) -> Result<(Owner, Completion, tokio::runtime::Handle), Refused> {
        let runtime = match tokio::runtime::Handle::try_current() {
            Ok(runtime) => runtime,
            Err(_) => {
                return Err(Refused {
                    error: durable::Error::Storage,
                    request: self,
                });
            }
        };
        let account = self.owner.account.clone();
        let mut slots = match account.editors.lock() {
            Ok(slots) => slots,
            Err(_) => {
                return Err(Refused {
                    error: durable::Error::Storage,
                    request: self,
                });
            }
        };
        let id = match slots.reserve_id() {
            Ok(id) => id,
            Err(error) => {
                return Err(Refused {
                    error,
                    request: self,
                });
            }
        };
        let Request { owner, expected } = self;
        let job = Arc::new(Job {
            id: id.clone(),
            scope: owner.scope().clone(),
            expected,
            account: Arc::downgrade(&account),
            evidence: Mutex::new(Evidence {
                phase: Phase::Reserved,
                terminal: Terminal::Running,
                review: None,
                writer: None,
                editor: None,
            }),
            changed: Default::default(),
        });
        slots.entries.insert(
            id,
            super::retention::Entry {
                editor: None,
                opening: Some(job.clone()),
            },
        );
        // Exact slot and completion owner both exist before scheduling.
        let completion = Completion {
            account: account.clone(),
            job: job.clone(),
            finished: false,
        };
        drop(slots);
        Ok((owner, completion, runtime))
    }
}
impl Open {
    pub fn id(&self) -> &str {
        &self.0.id
    }
    pub fn belongs_to(&self, account: &Arc<Account>) -> bool {
        Weak::ptr_eq(&self.0.account, &Arc::downgrade(account))
    }
    pub async fn ready(&self) {
        loop {
            let changed = self.0.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self
                .0
                .evidence
                .lock()
                .map_or(true, |e| e.terminal != Terminal::Running)
            {
                return;
            }
            changed.await;
        }
    }
    pub fn result(&self) -> Result<Arc<editor::Editor>, durable::Error> {
        let evidence = self
            .0
            .evidence
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        match evidence.terminal {
            Terminal::Running => Err(durable::Error::Pending),
            Terminal::Failed(error) => Err(error),
            Terminal::Unknown => Err(durable::Error::Storage),
            Terminal::Installed => evidence
                .editor
                .as_ref()
                .and_then(Weak::upgrade)
                .ok_or(durable::Error::Stale),
        }
    }
}
impl Job {
    fn phase(&self, phase: Phase) -> Result<(), durable::Error> {
        let mut evidence = self.evidence.lock().map_err(|_| durable::Error::Storage)?;
        if evidence.terminal != Terminal::Running {
            return Err(durable::Error::Stale);
        }
        evidence.phase = phase;
        Ok(())
    }
}
impl Completion {
    async fn run(mut self, owner: Owner, runtime: tokio::runtime::Handle) {
        let result = self.run_inner(owner, runtime).await;
        match result {
            Ok((editor, _owner_gate)) => {
                // One lock ordering: slot inventory -> job evidence. No Editor
                // state lock is acquired here. retire_clean removes this same
                // entry, including its weak job outcome.
                let installed = (|| {
                    let mut slots = self
                        .account
                        .editors
                        .lock()
                        .map_err(|_| durable::Error::Storage)?;
                    let entry = slots
                        .entries
                        .get_mut(&self.job.id)
                        .ok_or(durable::Error::Stale)?;
                    if entry.editor.is_some()
                        || entry
                            .opening
                            .as_ref()
                            .is_none_or(|job| !Arc::ptr_eq(job, &self.job))
                    {
                        return Err(durable::Error::Stale);
                    }
                    let mut evidence = self
                        .job
                        .evidence
                        .lock()
                        .map_err(|_| durable::Error::Storage)?;
                    if evidence.terminal != Terminal::Running {
                        return Err(durable::Error::Stale);
                    }
                    evidence.editor = Some(Arc::downgrade(&editor));
                    evidence.phase = Phase::Finished;
                    evidence.terminal = Terminal::Installed;
                    entry.editor = Some(editor);
                    Ok(())
                })();
                if let Err(error) = installed {
                    self.fail(error);
                }
            }
            Err(error) => self.fail(error),
        }
        self.finished = true;
        self.job.changed.notify_waiters();
    }
    fn fail(&self, error: durable::Error) {
        if let Ok(mut evidence) = self.job.evidence.lock() {
            // Keep the last execution phase and actual Review/Writer. An error
            // says nothing about rollback of a possible registration.
            evidence.terminal = Terminal::Failed(error);
        }
    }
    async fn run_inner(
        &self,
        owner: Owner,
        runtime: tokio::runtime::Handle,
    ) -> Result<(Arc<editor::Editor>, tokio::sync::OwnedMutexGuard<()>), durable::Error> {
        self.job.phase(Phase::Initializing)?;
        owner.account.initialize().await?;
        self.job.phase(Phase::WaitingOwner)?;
        let gate = owner.gate.clone().lock_owned().await;
        self.job.phase(Phase::Reading)?;
        let fresh = Arc::new(account::wait(owner.client()?.review(owner.scope().clone())).await?);
        {
            let mut evidence = self
                .job
                .evidence
                .lock()
                .map_err(|_| durable::Error::Storage)?;
            evidence.review = Some(fresh.clone());
        }
        if self
            .job
            .expected
            .as_ref()
            .is_some_and(|expected| !fresh.same(expected))
        {
            return Err(durable::Error::Stale);
        }
        self.job.phase(Phase::Registering)?;
        let writer = account::wait(owner.register((*fresh).clone())?).await?;
        {
            let mut evidence = self
                .job
                .evidence
                .lock()
                .map_err(|_| durable::Error::Storage)?;
            evidence.writer = Some(writer.clone());
        }
        if writer.scope() != owner.scope()
            || writer.enrollment() != fresh.row().lineage().enrollment()
        {
            return Err(durable::Error::Stale);
        }
        self.job.phase(Phase::Installing)?;
        // Preserve the existing serial owner gate through exact installation.
        Ok((
            editor::Editor::registered(
                owner.clone(),
                (*fresh).clone(),
                writer,
                runtime,
                self.job.id.clone(),
            ),
            gate,
        ))
    }
}
impl Drop for Completion {
    fn drop(&mut self) {
        if !self.finished {
            if let Ok(mut evidence) = self.job.evidence.lock() {
                evidence.terminal = Terminal::Unknown;
            }
            self.job.changed.notify_waiters();
        }
    }
}
pub(super) struct Inventory {
    pub id: String,
    pub scope: Option<Scope>,
    pub phase: Option<Phase>,
    pub terminal: Option<Terminal>,
    pub installed: bool,
}
impl Account {
    /// Includes unstarted reservations and completed-unconsumed failures.
    /// Diagnostic only: there is no idle/drained field or queue capability.
    pub(super) fn editor_inventory(&self) -> Result<Vec<Inventory>, durable::Error> {
        let slots = self.editors.lock().map_err(|_| durable::Error::Storage)?;
        slots
            .entries
            .iter()
            .map(|(id, entry)| {
                let (scope, phase, terminal) = match &entry.opening {
                    Some(job) => {
                        let evidence = job.evidence.lock().map_err(|_| durable::Error::Storage)?;
                        (
                            Some(job.scope.clone()),
                            Some(evidence.phase),
                            Some(evidence.terminal),
                        )
                    }
                    None => (None, None, None),
                };
                Ok(Inventory {
                    id: id.clone(),
                    scope,
                    phase,
                    terminal,
                    installed: entry.editor.is_some(),
                })
            })
            .collect()
    }
    pub(super) fn retained_openings(self: &Arc<Self>) -> Result<Vec<Open>, durable::Error> {
        let slots = self.editors.lock().map_err(|_| durable::Error::Storage)?;
        let mut opened = Vec::new();
        for entry in slots.entries.values() {
            if let Some(job) = &entry.opening {
                // Poison cannot disappear as a seemingly usable job list.
                drop(job.evidence.lock().map_err(|_| durable::Error::Storage)?);
                opened.push(Open(job.clone()));
            }
        }
        Ok(opened)
    }
}

#[cfg(test)]
#[path = "editor_open/tests.rs"]
mod tests;
