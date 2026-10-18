//! Root-owned worker: UI/source fences end at dispatch; result custody does not.
use super::*;
use recovery::{Custody, Storage, now};
use std::sync::Mutex;
use super_platinum_core::file_sharing::{Intent, Scope, store::Begin};

pub(super) struct Lease {
    pub id: String,
    pub custody: Arc<Mutex<Custody>>,
    admitted: bool,
    pub(super) visit: Option<Arc<drafts::Visit>>,
}
impl Lease {
    pub(super) fn reserve(
        custody: Arc<Mutex<Custody>>,
        key: &admission::Key,
        id: String,
    ) -> Result<Self, Refusal> {
        custody
            .lock()
            .map_err(|_| Refusal::Storage)?
            .reserve(key, &id, now())?;
        Ok(Self {
            id,
            custody,
            admitted: false,
            visit: None,
        })
    }
    pub(super) fn admit(&mut self, receipt: &Receipt) -> Result<(), Refusal> {
        self.admitted = true;
        self.custody
            .lock()
            .map_err(|_| Refusal::Storage)?
            .pending
            .get_mut(&self.id)
            .ok_or(Refusal::Storage)?
            .receipt = Some(receipt.clone());
        Ok(())
    }
    fn settled(&mut self) {
        if let Ok(mut held) = self.custody.lock() {
            held.release(&self.id);
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if !self.admitted {
            if let Ok(mut held) = self.custody.lock() {
                held.release(&self.id);
            }
        } else if let Ok(mut held) = self.custody.lock() {
            if let Some(p) = held.pending.get_mut(&self.id) {
                p.storage_failed = true;
            }
        }
    }
}
mod direct_resolution;
struct Worker {
    signal: Signal<ShellState>,
    source: admission::Source,
    destination: Destination,
    source_revision: Option<u64>,
    lease: Lease,
    visit: Arc<drafts::Visit>,
    capture: Option<durable::capture::Capture>,
    #[cfg(test)]
    extracted_note: Option<Vec<Value>>,
}
impl Worker {
    fn current(&self, shell: &ShellState) -> bool {
        self.source.current(shell)
            && self.source_revision.is_none_or(|revision| {
                !crate::file_events::changed_since(
                    shell,
                    &self.source.context.key.team,
                    &self.source.id,
                    revision,
                )
            })
            && recipients::chosen(shell, &self.lease.id, &self.destination.id)
            && visible(shell).is_some_and(|d| {
                d.id == self.lease.id
                    && d.phase == Phase::Preparing
                    && !d.closing
                    && d.destination.as_ref() == Some(&self.destination)
            })
            && admission::selected(shell, &self.destination.id).as_ref() == Some(&self.destination)
    }
    fn check(&self) -> Result<(), Refusal> {
        if !self.current(&self.signal.peek()) {
            return Err(Refusal::Changed);
        }
        self.lease
            .custody
            .lock()
            .map_err(|_| Refusal::Storage)?
            .check(&self.source.context.key, now())
    }
    fn read_error(&self, error: &Error) {
        if let Ok(mut held) = self.lease.custody.lock() {
            held.defer(&self.source.context.key, error, false, now());
        }
    }
    fn stop(&mut self, error: Refusal) {
        let mut shell = self.signal.write();
        if self.source.context.current(&shell)
            && shell.file_share.dialog.as_ref().is_some_and(|d| {
                d.id == self.lease.id
                    && !d.closing
                    && matches!(d.phase, Phase::Preparing | Phase::Sending)
            })
        {
            let dialog = shell.file_share.dialog.as_mut().unwrap();
            dialog.phase = if self.lease.admitted {
                Phase::StorageFailed
            } else if dialog.durable.is_some() {
                Phase::Editing
            } else {
                Phase::LoadFailed
            };
            dialog.error = Some(error.message());
        }
    }
    fn publish(&mut self, receipt: Receipt) {
        let mut shell = self.signal.write();
        if self.source.context.current(&shell)
            && shell.file_share.dialog.as_ref().is_some_and(|d| {
                d.id == self.lease.id
                    && !d.closing
                    && !d.draft_local
                    && matches!(d.phase, Phase::Preparing | Phase::Sending)
            })
        {
            let dialog = shell.file_share.dialog.as_mut().unwrap();
            dialog.receipt = Some(receipt);
            dialog.phase = Phase::Outcome;
            dialog.error = None;
        }
    }
    async fn run(&mut self) -> Result<(), Refusal> {
        // Refuse fixture/default-storage crossover before any transport call.
        let storage = Storage::capture(&self.source.context)?;
        self.check()?;
        #[cfg(test)]
        let extracted = self.extracted_note.take();
        #[cfg(not(test))]
        let extracted: Option<Vec<Value>> = None;
        let note = match extracted {
            Some(note) => note,
            None => read_note(&self.lease.id).await.ok_or(Refusal::Note)?,
        };
        let capture = self.capture.take().ok_or(Refusal::Changed)?;
        let queue = capture.queue.clone();
        let ticket = capture
            .persist(note, Some(self.destination.clone()), false)
            .await?;
        let outcome = ticket.wait().await;
        {
            let mut shell = self.signal.write();
            if self.source.context.current(&shell) {
                durable::apply_capture(&mut shell, &self.lease.id, &self.visit, &ticket, &outcome);
            }
        }
        let mut saved = match outcome {
            draft_io::Outcome::Saved(draft) => {
                let _ = queue.acknowledge(&ticket);
                draft
            }
            draft_io::Outcome::Blocked(receipt) => {
                if self.current(&self.signal.peek()) {
                    self.show_prior(receipt)?;
                }
                return Err(Refusal::Changed);
            }
            draft_io::Outcome::Conflict(_) => return Err(Refusal::Changed),
            _ => return Err(Refusal::Storage),
        };
        self.check()?;
        saved = self.resolve_person(saved).await?;
        self.check()?;
        let selected = saved
            .content
            .destination
            .as_ref()
            .ok_or(Refusal::Destination)?;
        let intent = Intent::new(
            saved.scope.file.clone(),
            selected.id.clone(),
            (!saved.content.note.is_empty()).then_some(saved.content.note.clone()),
            None,
        )
        .map_err(|_| Refusal::Note)?;
        let scope = Scope {
            team: saved.scope.team.clone(),
            user: saved.scope.user.clone(),
            file: saved.scope.file.clone(),
            destination: selected.id.clone(),
        };
        // Validate account/database and show an existing unresolved record before
        // reading Slack. Replacing the dialog ID mounts the saved note afresh.
        let query = scope.clone();
        let prior = storage.run(move |j| j.get(&query)).await?;
        self.check()?;
        if let Some(receipt) = prior.filter(Receipt::unresolved) {
            self.show_prior(receipt)?;
            return Ok(());
        }
        let key = self.source.context.key.clone();
        if storage
            .run(move |j| j.retry_at(&key.team, &key.user))
            .await?
            > now()
        {
            return Err(Refusal::Cooldown);
        }
        self.check()?;
        let revision = self
            .source_revision
            .unwrap_or_else(|| crate::file_events::snapshot_revision(&self.signal.peek()));
        let context = &self.source.context;
        let raw = context.file_info(&self.source.id).await.map_err(|e| {
            self.read_error(&e);
            Refusal::Read
        })?;
        self.check()?;
        self.source.fresh(raw)?;
        if crate::file_events::changed_since(
            &self.signal.peek(),
            &scope.team,
            &scope.file,
            revision,
        ) {
            return Err(Refusal::Changed);
        }
        let snapshot = self.policy().await?;
        self.check()?;
        if snapshot.eligibility() != Eligibility::Allowed {
            return Err(Refusal::Destination);
        }
        let prepared = intent.request().map_err(|_| Refusal::Note)?;
        let request = api::file_share::existing_file_share(
            &self.source.context.client,
            &self.source.context.session,
            &prepared,
        );
        self.check()?;
        let begin = storage
            .run(move |j| j.begin_from_draft(&saved, now()))
            .await?;
        let receipt = match begin {
            Begin::Started(r) => r,
            Begin::Blocked(r) => {
                if self.current(&self.signal.peek()) {
                    self.show_prior(r)?;
                }
                return Ok(());
            }
            Begin::CoolingDown { .. } => return Err(Refusal::Cooldown),
            Begin::Capacity => return Err(Refusal::Capacity),
        };
        self.lease.admit(&receipt)?;
        // Live is a no-op; an explicit fixture may pause this exact durable
        // attempt so the unchanged final fence can be exercised after a race.
        self.source
            .context
            .after_begin(&receipt)
            .await
            .map_err(|_| Refusal::Read)?;
        let current = {
            let shell = self.signal.peek();
            self.current(&shell)
                && !crate::file_events::changed_since(&shell, &scope.team, &scope.file, revision)
                && shell
                    .file_share
                    .policies
                    .get(&scope.destination)
                    .is_some_and(|o| o.current(&shell, &snapshot))
        };
        if !current {
            let query = scope.clone();
            let attempt = receipt.attempt_id.clone();
            let canceled = storage
                .run(move |j| j.cancel_before_dispatch(&query, &attempt, now()))
                .await?
                .ok_or(Refusal::Storage)?;
            self.lease.settled();
            self.publish(canceled);
            return Ok(());
        }
        {
            let mut shell = self.signal.write();
            let dialog = shell.file_share.dialog.as_mut().ok_or(Refusal::Changed)?;
            dialog.phase = Phase::Sending;
            dialog.receipt = Some(receipt.clone());
        }
        // Exactly one write. Never gate its returned result on current UI/source.
        let result = self.source.context.share(&receipt, request).await;
        let settled = settle(&storage, &self.lease, &receipt, result).await?;
        if let Ok(mut held) = self.lease.custody.lock() {
            held.accepted_note(&self.visit, &settled);
        }
        self.lease.settled();
        self.publish(settled);
        Ok(())
    }
    fn show_prior(&mut self, receipt: Receipt) -> Result<(), Refusal> {
        let mut shell = self.signal.write();
        let focus = shell
            .file_share
            .dialog
            .as_ref()
            .map(|d| d.return_focus.clone())
            .unwrap_or_default();
        recovery::show_receipt(&mut shell, receipt, focus, self.source.link.clone())?;
        Ok(())
    }
    async fn policy(&mut self) -> Result<policy_owner::Snapshot, Refusal> {
        self.check()?;
        let read = {
            let mut shell = self.signal.write();
            let mut state = std::mem::take(&mut shell.file_share);
            let result = state.begin_policy(&shell, &self.destination.id);
            shell.file_share = state;
            result?
        };
        let mut ticket = Ticket {
            custody: self.lease.custody.clone(),
            channel: self.destination.id.clone(),
            read: Some(read),
        };
        let result = ticket.read.as_ref().ok_or(Refusal::Changed)?.fetch().await;
        let raw = match result {
            Ok(raw) => raw,
            Err(error) => {
                self.read_error(&error);
                return Err(Refusal::Read);
            }
        };
        let mut shell = self.signal.write();
        let mut state = std::mem::take(&mut shell.file_share);
        let result = state
            .policies
            .get_mut(&self.destination.id)
            .ok_or(Refusal::Changed)
            .and_then(|owner| {
                owner
                    .finish_response(&shell, ticket.read.take().ok_or(Refusal::Changed)?, raw)
                    .map_err(|_| Refusal::Changed)
            });
        shell.file_share = state;
        result
    }
}
// No Signal or current account is accepted: settlement cannot accidentally be
// conditional on a visible row. Tests exercise this exact production function.
pub(super) async fn settle(
    storage: &Storage,
    lease: &Lease,
    receipt: &Receipt,
    result: Result<Value, Error>,
) -> Result<Receipt, Refusal> {
    let received_at = now();
    {
        let mut held = lease.custody.lock().map_err(|_| Refusal::Storage)?;
        let pending = held.pending.get(&lease.id).ok_or(Refusal::Changed)?;
        if pending.key != storage.key
            || receipt.scope.team != storage.key.team
            || receipt.scope.user != storage.key.user
            || pending.receipt.as_ref() != Some(receipt)
            || pending.settlement.is_some()
        {
            return Err(Refusal::Changed);
        }
        if let Err(error) = &result {
            held.defer(&storage.key, error, true, received_at);
        }
        held.retain(&lease.id, &result);
        held.pending
            .get_mut(&lease.id)
            .ok_or(Refusal::Changed)?
            .settlement = Some(retry::Settlement::new(
            storage.clone(),
            received_at,
            lease.visit.clone(),
        ));
    }
    let scope = receipt.scope.clone();
    let attempt = receipt.attempt_id.clone();
    let result = storage
        .run(move |j| j.finish_result(&scope, &attempt, result, received_at))
        .await
        .and_then(|r| r.ok_or(Refusal::Storage));
    if result.is_err() {
        if let Ok(mut held) = lease.custody.lock() {
            if let Some(p) = held.pending.get_mut(&lease.id) {
                p.storage_failed = true;
            }
        }
    }
    result
}
// A canceled read retires only its own ticket. Closing a component does not
// cancel the root worker, but this also handles actual root-task cancellation.
pub(super) struct Ticket {
    pub custody: Arc<Mutex<Custody>>,
    pub channel: String,
    pub read: Option<policy_owner::Read>,
}
impl Drop for Ticket {
    fn drop(&mut self) {
        if let Some(read) = self.read.take() {
            // A task may be canceled while ShellState is borrowed. Queue exact
            // ticket retirement; next admission/event drains before begin.
            if let Ok(mut held) = self.custody.lock() {
                if held.canceled_reads.len() < 32 {
                    held.canceled_reads.push((self.channel.clone(), read));
                }
            }
        }
    }
}

fn capture_worker(mut signal: Signal<ShellState>, id: String) -> Option<Worker> {
    let worker_signal = signal;
    let worker = {
        let mut shell = signal.write();
        let captured: Result<Worker, Refusal> = (|| {
            let dialog = visible(&shell)
                .filter(|d| d.id == id && d.phase == Phase::Editing)
                .ok_or(Refusal::Changed)?;
            let source = dialog.admission.clone().ok_or(Refusal::Source)?;
            if !source.current(&shell) {
                return Err(Refusal::Changed);
            }
            let destination = dialog.destination.clone().ok_or(Refusal::Destination)?;
            let visit = dialog.draft_visit.clone().ok_or(Refusal::Changed)?;
            let source_revision = destination
                .id
                .starts_with("person:")
                .then(|| crate::file_events::snapshot_revision(&shell));
            let mut lease = Lease::reserve(
                shell.file_share.custody.clone(),
                &source.context.key,
                id.clone(),
            )?;
            lease.visit = Some(visit.clone());
            let capture = durable::capture::Capture::prepare(&mut shell, &id)?;
            Ok(Worker {
                signal: worker_signal,
                source,
                destination,
                source_revision,
                lease,
                visit,
                capture: Some(capture),
                #[cfg(test)]
                extracted_note: None,
            })
        })();
        match captured {
            Ok(worker) => {
                let d = shell.file_share.dialog.as_mut().unwrap();
                d.phase = Phase::Preparing;
                d.error = None;
                Some(worker)
            }
            Err(error) => {
                if let Some(d) = shell.file_share.dialog.as_mut().filter(|d| d.id == id) {
                    d.error = Some(error.message());
                }
                None
            }
        }
    };
    worker
}
pub(crate) fn start(signal: Signal<ShellState>, id: String) {
    if let Some(mut worker) = capture_worker(signal, id) {
        dioxus::core::spawn_forever(async move {
            if let Err(error) = worker.run().await {
                worker.stop(error);
            }
        });
    }
}
#[cfg(test)]
mod fixture_tests;

pub(super) async fn read_note(dialog: &str) -> Option<Vec<Value>> {
    let id = serde_json::to_string(dialog).ok()?;
    let mut eval = document::eval(&format!(
        "await(async()=>{{let note=null;try{{const root=document.getElementById('file-share-dialog');const editor=root?.querySelector('#file-share-note');if(root?.dataset.fileShareId==={id}&&editor?.isConnected)note=window.superPlatinumRichEditor.read('file-share-note');}}catch{{}}dioxus.send(note);await dioxus.recv();}})();"
    ));
    let note = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        eval.recv::<Option<Vec<Value>>>(),
    )
    .await
    .ok()
    .and_then(Result::ok)
    .flatten();
    let _ = eval.send("received");
    note
}
