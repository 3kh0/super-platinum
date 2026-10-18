//! Exact created-reservation retirement; no network cancellation certificate.
use super::*;
impl Worker {
    pub(in crate::drafts::store::worker) fn hold_upload_cancellation(
        &self,
        parent: &Upload,
    ) -> Result<Operation<Stored, Result<bool, AppError>>, Reason> {
        let mut pending = self.pending.lock().unwrap();
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
            Ok(reservation.stored.clone())
        })();
        let stored = match checked {
            Ok(stored) => stored,
            Err(reason) => return Err(reason),
        };
        let state = pending.admission.as_mut().unwrap();
        let account = state.identity.owner.account().to_owned();
        let id = match state.reserve(&account) {
            Ok(id) => id,
            Err(reason) => return Err(reason),
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let held = Arc::new(Held {
            input: Arc::new(stored),
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
        let key = state.register(id, Some(parent.key.id), "upload-cancel", held.clone());
        parent.held.status.lock().unwrap().next = Some(key.clone());
        drop(pending);
        let request = Request::Upload(uploads::Command::Retire((*held.input).clone(), sender));
        held.status.lock().unwrap().request = Some(request);
        Ok(Operation { key, held })
    }
}
