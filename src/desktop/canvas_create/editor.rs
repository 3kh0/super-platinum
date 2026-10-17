//! Root-owned accepted-input custody. No DOM, credentials, or disk work here.
use super::{
    adapter::{self, Custody, Phase, Storage, Ticket},
    domain,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

const EDITORS: usize = 32;

#[derive(Clone, Default)]
pub(crate) struct Root {
    pub(super) delivery: Custody,
    editors: Arc<Mutex<BTreeMap<String, Arc<Editor>>>>,
}

pub(super) struct Editor {
    pub(super) storage: Storage,
    custody: Custody,
    inner: Mutex<Inner>,
    changed: watch::Sender<u64>,
}
struct Inner {
    base: domain::Draft,
    latest: domain::Content,
    serial: u64,
    saved_serial: u64,
    running: bool,
    failed: bool,
    ticket: Option<Ticket>,
    capture: Option<String>,
    sealed: bool,
    editable: bool,
    decision: Option<Decision>,
    retained_version: Option<domain::Content>,
}
#[derive(Clone)]
pub(super) struct Decision {
    pub base: domain::Draft,
    pub attempt: domain::Attempt,
    pub reason: domain::ArchiveReason,
    pub operation: String,
    pub writer: String,
}
#[derive(Clone)]
pub(super) struct Snapshot {
    pub base: domain::Draft,
    pub content: domain::Content,
    pub serial: u64,
    pub saved: bool,
    pub failed: bool,
    pub capture: Option<String>,
    pub sealed: bool,
}

fn key(owner: &domain::Owner) -> String {
    format!("{}/{}/{}", owner.account(), owner.team(), owner.user())
}
impl Root {
    pub(super) fn find(&self, owner: &domain::Owner) -> Option<Arc<Editor>> {
        self.editors
            .lock()
            .expect("Canvas editor lock")
            .get(&key(owner))
            .cloned()
    }
    pub(super) fn install(
        &self,
        storage: Storage,
        base: domain::Draft,
        editable: bool,
    ) -> domain::Result<Arc<Editor>> {
        if storage.owner() != base.owner() || base.archived() {
            return Err(domain::Fault::Stale);
        }
        let mut editors = self.editors.lock().expect("Canvas editor lock");
        if let Some(existing) = editors.get(&key(base.owner())) {
            return if existing.snapshot().base == base {
                Ok(existing.clone())
            } else {
                Err(domain::Fault::Stale)
            };
        }
        if editors.len() >= EDITORS {
            return Err(domain::Fault::Capacity);
        }
        let (changed, _) = watch::channel(0);
        let editor = Arc::new(Editor {
            storage,
            custody: self.delivery.clone(),
            changed,
            inner: Mutex::new(Inner {
                latest: base.content().clone(),
                base,
                serial: 0,
                saved_serial: 0,
                running: false,
                failed: false,
                ticket: None,
                capture: None,
                sealed: false,
                editable,
                decision: None,
                retained_version: None,
            }),
        });
        editors.insert(key(editor.storage.owner()), editor.clone());
        Ok(editor)
    }
    /// Only exact clean editor retirement. Held input is never evicted.
    pub(super) fn release(&self, editor: &Arc<Editor>) -> bool {
        let mut editors = self.editors.lock().expect("Canvas editor lock");
        let inner = editor.inner.lock().expect("Canvas editor lock");
        if inner.running
            || inner.failed && inner.retained_version.as_ref() != Some(&inner.latest)
            || inner.ticket.is_some()
            || inner.capture.is_some()
            || inner.decision.is_some()
            || inner.serial != inner.saved_serial
                && inner.retained_version.as_ref() != Some(&inner.latest)
        {
            return false;
        }
        let id = key(editor.storage.owner());
        if !editors
            .get(&id)
            .is_some_and(|entry| Arc::ptr_eq(entry, editor))
        {
            return false;
        }
        editors.remove(&id);
        true
    }
}
impl Editor {
    /// Make every root-held save operation terminal before displaying choices.
    /// A failed immutable operation is retried locally first; it cannot return
    /// after the review and resurrect a version which the user already retired.
    pub(super) async fn retain_for_review(self: &Arc<Self>) -> domain::Result<()> {
        if !self.snapshot().sealed {
            return Err(domain::Fault::Blocked);
        }
        {
            let i = self.inner.lock().expect("Canvas editor lock");
            if i.retained_version.as_ref() == Some(&i.latest) {
                return Ok(());
            }
        }
        let prior = self
            .inner
            .lock()
            .expect("Canvas editor lock")
            .ticket
            .clone();
        if let Some(prior) = prior {
            if matches!(prior.phase(), Phase::Held(_)) {
                prior.retry_storage()?;
            }
            let mut changes = prior.changes();
            loop {
                match prior.phase() {
                    Phase::Saving => {
                        changes
                            .changed()
                            .await
                            .map_err(|_| domain::Fault::Invalid)?;
                    }
                    Phase::Saved(saved) => {
                        let mut i = self.inner.lock().expect("Canvas editor lock");
                        if i.latest == *saved.content() {
                            i.saved_serial = i.serial;
                        }
                        i.base = saved;
                        i.failed = false;
                        i.ticket = None;
                        i.running = false;
                        self.custody.acknowledge(&prior);
                        drop(i);
                        self.notify();
                        break;
                    }
                    Phase::Conflict(_) | Phase::CaptureDisposed => {
                        let mut i = self.inner.lock().expect("Canvas editor lock");
                        let exact = prior.captured().as_ref() == Some(&i.latest);
                        if exact {
                            i.retained_version = Some(i.latest.clone());
                        }
                        i.ticket = None;
                        i.running = false;
                        self.custody.acknowledge(&prior);
                        if exact {
                            return Ok(());
                        }
                        break;
                    }
                    _ => return Err(domain::Fault::Blocked),
                }
            }
        }
        let s = self.snapshot();
        if s.saved {
            return Ok(());
        }
        // Fresh exact identity for the final sealed pair; never reuse an older
        // captured operation UUID for different content.
        let ticket = self.custody.reserve(self.storage.clone(), s.base)?;
        self.inner.lock().expect("Canvas editor lock").ticket = Some(ticket.clone());
        ticket.save(s.content.clone()).map_err(|e| e.0)?;
        let mut changes = ticket.changes();
        loop {
            match ticket.phase() {
                Phase::Saving => {
                    changes
                        .changed()
                        .await
                        .map_err(|_| domain::Fault::Invalid)?;
                }
                Phase::Saved(saved) => {
                    let mut i = self.inner.lock().expect("Canvas editor lock");
                    i.base = saved;
                    i.saved_serial = i.serial;
                    i.failed = false;
                    i.running = false;
                    i.ticket = None;
                    self.custody.acknowledge(&ticket);
                    drop(i);
                    self.notify();
                    return Ok(());
                }
                Phase::Conflict(conflict) if conflict.content() == &s.content => {
                    let mut i = self.inner.lock().expect("Canvas editor lock");
                    i.retained_version = Some(s.content);
                    i.ticket = None;
                    i.running = false;
                    self.custody.acknowledge(&ticket);
                    drop(i);
                    self.notify();
                    return Ok(());
                }
                _ => return Err(domain::Fault::Blocked),
            }
        }
    }
    pub(super) fn decision(
        &self,
        attempt: domain::Attempt,
        reason: domain::ArchiveReason,
    ) -> domain::Result<Decision> {
        let mut i = self.inner.lock().expect("Canvas editor lock");
        if let Some(saved) = &i.decision {
            return Ok(saved.clone());
        }
        if i.running || i.failed || i.serial != i.saved_serial || i.capture.is_some() {
            return Err(domain::Fault::Blocked);
        }
        let decision = Decision {
            base: i.base.clone(),
            attempt,
            reason,
            operation: uuid::Uuid::new_v4().to_string(),
            writer: uuid::Uuid::new_v4().to_string(),
        };
        i.decision = Some(decision.clone());
        Ok(decision)
    }
    pub(super) fn decision_saved(&self, operation: &str) {
        let mut i = self.inner.lock().expect("Canvas editor lock");
        if i.decision
            .as_ref()
            .is_some_and(|d| d.operation == operation)
        {
            i.decision = None;
        }
    }
    fn notify(&self) {
        self.changed.send_modify(|v| *v = v.wrapping_add(1));
    }
    pub(super) fn stop_editing(&self) {
        self.inner.lock().expect("Canvas editor lock").editable = false;
    }
    pub(super) fn changes(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }
    pub(super) fn snapshot(&self) -> Snapshot {
        let i = self.inner.lock().expect("Canvas editor lock");
        Snapshot {
            base: i.base.clone(),
            content: i.latest.clone(),
            serial: i.serial,
            saved: !i.running && !i.failed && i.saved_serial == i.serial,
            failed: i.failed,
            capture: i.capture.clone(),
            sealed: i.sealed,
        }
    }
    /// Coalesces only input that has not started persistence. In-flight captures
    /// and delivery tickets are immutable and remain separate root allocations.
    pub(super) fn accept(self: &Arc<Self>, content: domain::Content) -> domain::Result<u64> {
        let mut i = self.inner.lock().expect("Canvas editor lock");
        if i.sealed || !i.editable {
            return Err(domain::Fault::Blocked);
        }
        if i.latest == content {
            return Ok(i.serial);
        }
        i.serial = i.serial.checked_add(1).ok_or(domain::Fault::Capacity)?;
        i.latest = content;
        let serial = i.serial;
        let start = !i.running && !i.failed;
        if start {
            i.running = true;
        }
        drop(i);
        self.notify();
        if start {
            let editor = self.clone();
            adapter::retain(async move {
                editor.drive().await;
            });
        }
        Ok(serial)
    }
    /// Charges this editor's single bounded capture lane BEFORE document eval.
    pub(super) fn reserve_capture(&self) -> domain::Result<String> {
        let mut i = self.inner.lock().expect("Canvas editor lock");
        if i.capture.is_some() || i.sealed {
            return Err(domain::Fault::Blocked);
        }
        let id = uuid::Uuid::new_v4().to_string();
        i.capture = Some(id.clone());
        drop(i);
        self.notify();
        Ok(id)
    }
    /// Custody first, UI-current checks later. The native seal prevents further
    /// DOM edits; queued older input events cannot overwrite the captured pair.
    pub(super) fn retain_capture(
        self: &Arc<Self>,
        id: &str,
        content: domain::Content,
    ) -> domain::Result<u64> {
        let mut i = self.inner.lock().expect("Canvas editor lock");
        if i.capture.as_deref() != Some(id) {
            return Err(domain::Fault::Stale);
        }
        if i.latest != content {
            i.serial = i.serial.checked_add(1).ok_or(domain::Fault::Capacity)?;
            i.latest = content;
        }
        i.sealed = true;
        let serial = i.serial;
        let start = !i.running && !i.failed && i.saved_serial != serial;
        if start {
            i.running = true;
        }
        drop(i);
        self.notify();
        if start {
            let editor = self.clone();
            adapter::retain(async move {
                editor.drive().await;
            });
        }
        Ok(serial)
    }
    // Not used for a timeout: a possible late successful reply owns this slot.
    pub(super) fn release_capture(&self, id: &str) {
        let mut i = self.inner.lock().expect("Canvas editor lock");
        if i.capture.as_deref() == Some(id) {
            i.capture = None;
            i.sealed = false;
        }
        drop(i);
        self.notify();
    }
    pub(super) async fn saved(&self, serial: u64) -> domain::Result<domain::Draft> {
        let mut changes = self.changes();
        loop {
            let s = self.snapshot();
            if s.serial != serial {
                return Err(domain::Fault::Stale);
            }
            if s.failed {
                return Err(domain::Fault::Blocked);
            }
            if s.saved {
                return Ok(s.base);
            }
            changes
                .changed()
                .await
                .map_err(|_| domain::Fault::Invalid)?;
        }
    }
    pub(super) fn retry(self: &Arc<Self>) -> domain::Result<()> {
        let mut i = self.inner.lock().expect("Canvas editor lock");
        if i.running || !i.failed {
            return Err(domain::Fault::Blocked);
        }
        if let Some(ticket) = &i.ticket {
            ticket.retry_storage()?;
        }
        i.failed = false;
        i.running = true;
        drop(i);
        self.notify();
        let editor = self.clone();
        adapter::retain(async move {
            editor.drive().await;
        });
        Ok(())
    }
    async fn drive(self: Arc<Self>) {
        loop {
            let (base, content, serial, previous) = {
                let i = self.inner.lock().expect("Canvas editor lock");
                (i.base.clone(), i.latest.clone(), i.serial, i.ticket.clone())
            };
            let ticket = match previous {
                Some(ticket) => ticket,
                None => match self.custody.reserve(self.storage.clone(), base) {
                    Ok(ticket) => {
                        self.inner.lock().expect("Canvas editor lock").ticket =
                            Some(ticket.clone());
                        if ticket.save(content).is_err() {
                            self.failed();
                            return;
                        }
                        ticket
                    }
                    Err(_) => {
                        self.failed();
                        return;
                    }
                },
            };
            let mut changes = ticket.changes();
            let result = loop {
                match ticket.phase() {
                    Phase::Saving | Phase::CaptureReserved => {
                        if changes.changed().await.is_err() {
                            self.failed();
                            return;
                        }
                    }
                    Phase::Saved(saved) => break saved,
                    _ => {
                        self.failed();
                        return;
                    }
                }
            };
            let mut i = self.inner.lock().expect("Canvas editor lock");
            // Recovering a held job may save its older immutable snapshot. It
            // advances the base but must not claim the current newer serial.
            i.base = result.clone();
            if i.latest == *result.content() {
                i.saved_serial = i.serial;
            } else if i.serial == serial { /* An earlier failed capture was saved; keep newer pending. */
            }
            i.ticket = None;
            self.custody.acknowledge(&ticket);
            let done = i.saved_serial == i.serial;
            if done {
                i.running = false;
            }
            drop(i);
            self.notify();
            if done {
                return;
            }
        }
    }
    fn failed(&self) {
        let mut i = self.inner.lock().expect("Canvas editor lock");
        i.failed = true;
        i.running = false;
        drop(i);
        self.notify();
    }
}
