//! Caller payload/receiver/result retained before enqueue. Fixture-private only.
use super::*;
use crate::drafts::store::uploads::{Draft, Reservation, Stored, Ticket};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::drafts::store::worker) enum Delivery {
    Pending,
    Disconnected,
}
struct Status<T> {
    request: Option<Request>,
    receiver: Option<mpsc::Receiver<T>>,
    result: Option<Arc<T>>,
    disconnected: bool,
    submitted: Option<u64>,
    acknowledged: bool,
    next: Option<Key>,
}
pub(in crate::drafts::store::worker) struct Held<I, T> {
    input: Arc<I>,
    status: Mutex<Status<T>>,
    changed: std::sync::Condvar,
    #[cfg(test)]
    wait_probe: Mutex<Option<mpsc::Sender<&'static str>>>,
}
pub(in crate::drafts::store::worker) struct Operation<I, T> {
    key: Key,
    held: Arc<Held<I, T>>,
}
impl<I, T> Clone for Operation<I, T> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            held: self.held.clone(),
        }
    }
}
impl<I, T> Operation<I, T> {
    pub(in crate::drafts::store::worker) fn key(&self) -> Key {
        self.key.clone()
    }
    pub(in crate::drafts::store::worker) fn input(&self) -> Arc<I> {
        self.held.input.clone()
    }
    pub(in crate::drafts::store::worker) fn poll(&self) -> Result<Arc<T>, Delivery> {
        let mut status = self.held.status.lock().unwrap();
        if let Some(result) = &status.result {
            return Ok(result.clone());
        }
        if status.disconnected {
            return Err(Delivery::Disconnected);
        }
        let Some(receiver) = status.receiver.as_ref() else {
            return Err(Delivery::Pending);
        };
        match receiver.try_recv() {
            Ok(value) => {
                let value = Arc::new(value);
                status.result = Some(value.clone());
                Ok(value)
            }
            Err(mpsc::TryRecvError::Empty) => Err(Delivery::Pending),
            Err(mpsc::TryRecvError::Disconnected) => {
                status.disconnected = true;
                Err(Delivery::Disconnected)
            }
        }
    }
}
pub(in crate::drafts::store::worker) type Upload = Operation<Draft, Result<Reservation, AppError>>;
pub(in crate::drafts::store::worker) struct Finalize {
    pub ticket: Ticket,
    pub files: Vec<String>,
}
pub(in crate::drafts::store::worker) type UploadCompletion =
    Operation<Finalize, Result<Option<Stored>, AppError>>;
impl Worker {
    // Reserve capacity under the real mutex BEFORE input moves into its holder.
    // Build the Request from the retained immutable input after releasing Pending:
    // payload cloning belongs to this explicit input owner, never the Book/lock.
    fn hold<I: Send + Sync + 'static, T: Send + Sync + 'static>(
        &self,
        input: I,
        account: &str,
        family: &'static str,
        build: impl FnOnce(&I, mpsc::SyncSender<T>) -> Request,
    ) -> Result<Operation<I, T>, Refused<I>> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut pending = self.pending.lock().unwrap();
        if !bounded(account, None) {
            return Err(refusal(&mut pending, Reason::Metadata, input));
        }
        let id = match pending
            .admission
            .as_mut()
            .ok_or(Reason::Unsupported)
            .and_then(|state| state.reserve(account))
        {
            Ok(id) => id,
            Err(reason) => return Err(refusal(&mut pending, reason, input)),
        };
        let held = Arc::new(Held {
            input: Arc::new(input),
            status: Mutex::new(Status {
                request: None,
                receiver: Some(receiver),
                result: None,
                disconnected: false,
                submitted: None,
                acknowledged: false,
                next: None,
            }),
            changed: std::sync::Condvar::new(),
            #[cfg(test)]
            wait_probe: Mutex::new(None),
        });
        let key = pending
            .admission
            .as_mut()
            .unwrap()
            .register(id, None, family, held.clone());
        drop(pending);
        let request = build(&held.input, sender);
        held.status.lock().unwrap().request = Some(request);
        Ok(Operation { key, held })
    }
    pub(in crate::drafts::store::worker) fn hold_upload(
        &self,
        draft: Draft,
    ) -> Result<Upload, Refused<Draft>> {
        // This bounded account string is metadata, not a copy of the draft.
        if !bounded(&draft.account_id, None) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                draft,
            ));
        }
        let account = draft.account_id.clone();
        self.hold(draft, &account, "upload-reserve", |draft, sender| {
            Request::Upload(uploads::Command::Reserve(draft.clone(), sender))
        })
    }
    pub(in crate::drafts::store::worker) fn hold_review(
        &self,
        scope: Scope,
    ) -> Result<Operation<Scope, Result<LocalReview, AppError>>, Refused<Scope>> {
        if !bounded(&scope.account_id, Some(&scope.key)) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                scope,
            ));
        }
        let account = scope.account_id.clone();
        self.hold(scope, &account, "local-review", |scope, sender| {
            Request::ReadLocalReview(scope.clone(), sender)
        })
    }
    pub(in crate::drafts::store::worker) fn retained_keys(&self) -> Vec<RetainedKey> {
        let pending = self.pending.lock().unwrap();
        pending
            .admission
            .as_ref()
            .map(|state| {
                state
                    .slots
                    .iter()
                    .map(|(&id, entry)| RetainedKey {
                        key: Key {
                            route: state.identity.clone(),
                            id,
                            identity: entry.identity.clone(),
                        },
                        family: entry.family,
                        generation: entry.generation,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    pub(in crate::drafts::store::worker) fn reattach<
        I: Send + Sync + 'static,
        T: Send + Sync + 'static,
    >(
        &self,
        key: &Key,
    ) -> Result<Operation<I, T>, Reason> {
        let pending = self.pending.lock().unwrap();
        let state = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
        let held = state
            .entry(key)?
            .retained
            .clone()
            .downcast::<Held<I, T>>()
            .map_err(|_| Reason::Stale)?;
        Ok(Operation {
            key: key.clone(),
            held,
        })
    }
    pub(in crate::drafts::store::worker) fn submit_held<
        I: Send + Sync + 'static,
        T: Send + Sync + 'static,
    >(
        &self,
        operation: &Operation<I, T>,
    ) -> Result<(), Reason> {
        let mut pending = self.pending.lock().unwrap();
        let state = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
        let actual = state
            .entry(&operation.key)?
            .retained
            .clone()
            .downcast::<Held<I, T>>()
            .map_err(|_| Reason::Stale)?;
        if !Arc::ptr_eq(&actual, &operation.held) {
            return Err(Reason::Stale);
        }
        let mut status = actual.status.lock().unwrap();
        if status.submitted.is_some() {
            return Ok(());
        } // reattach, never re-enqueue
        let request = status.request.take().ok_or(Reason::Incomplete)?;
        match Self::try_submit_locked(&mut pending, &operation.key, request) {
            Ok(id) => {
                status.submitted = Some(id);
                drop(status);
                drop(pending);
                self.ready.notify_one();
                Ok(())
            }
            Err(refusal) => {
                status.request = Some(refusal.input);
                Err(refusal.reason)
            }
        }
    }
    pub(in crate::drafts::store::worker) fn acknowledge<
        I: Send + Sync + 'static,
        T: Send + Sync + 'static,
    >(
        &self,
        operation: &Operation<I, T>,
    ) -> Result<(), Reason> {
        let pending = self.pending.lock().unwrap();
        let state = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
        let actual = state
            .entry(&operation.key)?
            .retained
            .clone()
            .downcast::<Held<I, T>>()
            .map_err(|_| Reason::Stale)?;
        if !Arc::ptr_eq(&actual, &operation.held) {
            return Err(Reason::Stale);
        }
        let mut status = actual.status.lock().unwrap();
        let id = status.submitted.ok_or(Reason::Incomplete)?;
        if status.result.is_none()
            || pending
                .accounting
                .as_ref()
                .and_then(|b| b.terminal(id))
                .is_none()
        {
            return Err(Reason::Incomplete);
        }
        status.acknowledged = true;
        Ok(()) // no reclamation in C2.2, even after ack
    }
    // An exact prebound storage-only step from the actual accepted upload result.
    // No caller-supplied ticket, command or action name. No HTTP is implemented.
    // A raw completion arriving after Begin is NOT accepted by this first adapter;
    // registering whole-network work and broader family edges remains C2.3/C3.
    pub(in crate::drafts::store::worker) fn hold_upload_completion(
        &self,
        parent: &Upload,
        files: Vec<String>,
    ) -> Result<UploadCompletion, Refused<Vec<String>>> {
        let mut pending = self.pending.lock().unwrap();
        let generation = pending.admission.as_ref().map(|s| s.generation);
        let fail = |reason, input| Refused {
            reason,
            generation,
            input,
        };
        let checked = (|| {
            let state = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
            let entry = state.entry(&parent.key)?;
            if entry.generation != state.generation {
                return Err(Reason::Stale);
            }
            let actual = entry
                .retained
                .clone()
                .downcast::<Held<Draft, Result<Reservation, AppError>>>()
                .map_err(|_| Reason::Stale)?;
            if !Arc::ptr_eq(&actual, &parent.held) {
                return Err(Reason::Stale);
            }
            state.check(state.identity.owner.account(), false)?;
            let result = parent.poll();
            let status = actual.status.lock().unwrap();
            if status.next.is_some() {
                return Err(Reason::Stale);
            }
            let id = status.submitted.ok_or(Reason::Incomplete)?;
            let report = pending
                .accounting
                .as_ref()
                .and_then(|b| b.terminal(id))
                .ok_or(Reason::Incomplete)?;
            if report.execution != terminal::Execution::StorageAccepted {
                return Err(Reason::Stale);
            }
            let result = result.map_err(|_| Reason::Incomplete)?;
            let reservation = result.as_ref().as_ref().map_err(|_| Reason::Stale)?;
            if !reservation.created {
                return Err(Reason::Stale);
            }
            Ok(reservation.stored.ticket.clone())
        })();
        let ticket = match checked {
            Ok(ticket) => ticket,
            Err(reason) => return Err(fail(reason, files)),
        };
        let state = pending.admission.as_mut().unwrap();
        let account = state.identity.owner.account().to_owned();
        let id = match state.reserve(&account) {
            Ok(id) => id,
            Err(reason) => return Err(fail(reason, files)),
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let held = Arc::new(Held {
            input: Arc::new(Finalize { ticket, files }),
            status: Mutex::new(Status {
                request: None,
                receiver: Some(receiver),
                result: None,
                disconnected: false,
                submitted: None,
                acknowledged: false,
                next: None,
            }),
            changed: std::sync::Condvar::new(),
            #[cfg(test)]
            wait_probe: Mutex::new(None),
        });
        let key = state.register(id, Some(parent.key.id), "upload-finalize", held.clone());
        parent.held.status.lock().unwrap().next = Some(key.clone());
        drop(pending);
        let request = Request::Upload(uploads::Command::Finalize(
            held.input.ticket.clone(),
            held.input.files.clone(),
            sender,
        ));
        held.status.lock().unwrap().request = Some(request);
        Ok(Operation { key, held })
    }
    pub(in crate::drafts::store::worker) fn begin_fence(&self) -> Result<Fence, Reason> {
        let mut pending = self.pending.lock().unwrap();
        let book = pending.accounting.as_ref().ok_or(Reason::Unsupported)?;
        if !book.admission_room(0) {
            return Err(Reason::Incomplete);
        }
        let ids = book.work_ids();
        pending
            .admission
            .as_mut()
            .ok_or(Reason::Unsupported)?
            .begin(ids)
    }
    pub(in crate::drafts::store::worker) fn inspect_fence(
        &self,
        fence: &Fence,
    ) -> Result<(), Reason> {
        let pending = self.pending.lock().unwrap();
        if pending
            .accounting
            .as_ref()
            .is_none_or(|b| !b.admission_room(0))
        {
            return Err(Reason::Incomplete);
        }
        pending
            .admission
            .as_ref()
            .ok_or(Reason::Unsupported)?
            .inspect(fence)
    }
    pub(in crate::drafts::store::worker) fn resume_admission(
        &self,
        fence: &Fence,
    ) -> Result<(), Reason> {
        let mut pending = self.pending.lock().unwrap();
        pending
            .admission
            .as_mut()
            .ok_or(Reason::Unsupported)?
            .resume(fence)
    }
}

#[path = "retained_upload.rs"]
mod upload;

#[path = "retained_upload_cancel.rs"]
mod upload_cancel;
