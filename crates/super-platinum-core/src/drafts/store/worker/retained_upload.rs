//! Closed upload operations reuse the original retained holder and executor.
use super::*;

impl<I, T> Operation<I, T> {
    #[cfg(test)]
    pub(in crate::drafts::store::worker) fn wait_probe(&self, probe: mpsc::Sender<&'static str>) {
        *self.held.wait_probe.lock().unwrap() = Some(probe);
    }
    #[cfg(test)]
    pub(in crate::drafts::store::worker) fn wait_probe_barrier(&self) {
        drop(self.held.status.lock().unwrap());
    }

    fn wait_reply(&self) -> Result<(u64, Arc<T>), Reason> {
        let mut state = self.held.status.lock().map_err(|_| Reason::Incomplete)?;
        loop {
            let id = state.submitted.ok_or(Reason::Incomplete)?;
            if let Some(value) = &state.result {
                return Ok((id, value.clone()));
            }
            if state.disconnected {
                return Err(Reason::Incomplete);
            }
            if let Some(receiver) = state.receiver.take() {
                #[cfg(test)]
                if let Some(probe) = self.held.wait_probe.lock().unwrap().as_ref() {
                    let _ = probe.send("receiver");
                }
                drop(state);
                let result = receiver.recv();
                state = self.held.status.lock().map_err(|_| Reason::Incomplete)?;
                match result {
                    Ok(value) => state.result = Some(Arc::new(value)),
                    Err(_) => state.disconnected = true,
                }
                self.held.changed.notify_all();
            } else {
                #[cfg(test)]
                if let Some(probe) = self.held.wait_probe.lock().unwrap().as_ref() {
                    let _ = probe.send("condvar");
                }
                state = self
                    .held
                    .changed
                    .wait(state)
                    .map_err(|_| Reason::Incomplete)?;
            }
        }
    }
}
impl Worker {
    pub(in crate::drafts::store::worker) fn observe_upload_canceled(
        &self,
        parent: &Upload,
        child: &Operation<Stored, Result<bool, AppError>>,
        retire: bool,
    ) -> Result<(), Reason> {
        let reserved = parent.poll().map_err(|_| Reason::Incomplete)?;
        let reserved = reserved.as_ref().as_ref().map_err(|_| Reason::Incomplete)?;
        let result = child.poll().map_err(|_| Reason::Incomplete)?;
        if !reserved.created || !matches!(result.as_ref(), Ok(true)) {
            return Err(Reason::Incomplete);
        }
        if reserved.stored != *child.held.input {
            return Err(Reason::Stale);
        }
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let first = terminal(&pending, parent, "upload-reserve", None, Some(&child.key))?;
        let last = terminal(&pending, child, "upload-cancel", Some(parent.key.id), None)?;
        finish(
            &mut pending,
            &[parent.key.clone(), child.key.clone()],
            &[first, last],
            retire,
        )
    }
    pub(in crate::drafts::store::worker) fn reattach_upload_cancellation(
        &self,
        parent: &Upload,
        key: &Key,
    ) -> Result<Operation<Stored, Result<bool, AppError>>, Reason> {
        let child = self.reattach::<Stored, Result<bool, AppError>>(key)?;
        let pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let admission = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
        let p = admission.entry(&parent.key)?;
        let c = admission.entry(key)?;
        if p.family != "upload-reserve"
            || p.parent.is_some()
            || c.family != "upload-cancel"
            || c.parent != Some(parent.key.id)
        {
            return Err(Reason::Stale);
        }
        let actual = p
            .retained
            .clone()
            .downcast::<Held<Draft, Result<Reservation, AppError>>>()
            .map_err(|_| Reason::Stale)?;
        if !Arc::ptr_eq(&actual, &parent.held)
            || actual
                .status
                .lock()
                .map_err(|_| Reason::Incomplete)?
                .next
                .as_ref()
                .is_none_or(|next| !same(next, key))
        {
            return Err(Reason::Stale);
        }
        Ok(child)
    }
    pub(in crate::drafts::store::worker) fn wait_retained<
        I: Send + Sync + 'static,
        T: Send + Sync + 'static,
    >(
        &self,
        operation: &Operation<I, T>,
    ) -> Result<Arc<T>, Reason> {
        let actual = self.reattach::<I, T>(&operation.key)?;
        if !Arc::ptr_eq(&actual.held, &operation.held) {
            return Err(Reason::Stale);
        }
        let (id, value) = operation.wait_reply()?;
        self.wait_accounted(&operation.key, id)?;
        Ok(value)
    }
    pub(in crate::drafts::store::worker) fn hold_upload_inventory(
        &self,
        account: String,
    ) -> Result<Operation<String, Result<Vec<Stored>, AppError>>, Refused<String>> {
        let id = account.clone();
        self.hold(account, &id, "upload-load", |account, reply| {
            Request::Upload(uploads::Command::Load(account.clone(), reply))
        })
    }
    pub(in crate::drafts::store::worker) fn hold_upload_retirement(
        &self,
        stored: Stored,
    ) -> Result<Operation<Stored, Result<bool, AppError>>, Refused<Stored>> {
        let account = stored.ticket.account_id().to_owned();
        self.hold(stored, &account, "upload-retire", |stored, reply| {
            Request::Upload(uploads::Command::Retire(stored.clone(), reply))
        })
    }
}

fn same(a: &Key, b: &Key) -> bool {
    a.id == b.id && Arc::ptr_eq(&a.route, &b.route) && Arc::ptr_eq(&a.identity, &b.identity)
}
fn terminal<I: Send + Sync + 'static, T: Send + Sync + 'static>(
    pending: &Pending,
    operation: &Operation<I, T>,
    family: &str,
    parent: Option<u64>,
    next: Option<&Key>,
) -> Result<u64, Reason> {
    let admission = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
    admission.check(admission.identity.owner.account(), false)?;
    let entry = admission.entry(&operation.key)?;
    if entry.family != family || entry.parent != parent {
        return Err(Reason::Stale);
    }
    let held = entry
        .retained
        .clone()
        .downcast::<Held<I, T>>()
        .map_err(|_| Reason::Stale)?;
    if !Arc::ptr_eq(&held, &operation.held) {
        return Err(Reason::Stale);
    }
    let state = held.status.lock().map_err(|_| Reason::Incomplete)?;
    if state.request.is_some()
        || state.result.is_none()
        || state.disconnected
        || !match (&state.next, next) {
            (None, None) => true,
            (Some(a), Some(b)) => same(a, b),
            _ => false,
        }
    {
        return Err(Reason::Incomplete);
    }
    state.submitted.ok_or(Reason::Incomplete)
}
fn finish(pending: &mut Pending, keys: &[Key], ids: &[u64], retire: bool) -> Result<(), Reason> {
    let admission = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
    if admission.slots.iter().any(|(id, entry)| {
        entry
            .parent
            .is_some_and(|parent| keys.iter().any(|k| k.id == parent))
            && !keys.iter().any(|k| k.id == *id)
    }) {
        return Err(Reason::Incomplete);
    }
    let book = pending.accounting.as_ref().ok_or(Reason::Incomplete)?;
    let reports = ids
        .iter()
        .map(|id| {
            book.terminal(*id)
                .map(|r| (*id, r))
                .ok_or(Reason::Incomplete)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if keys.len() != reports.len()
        || reports.iter().any(|(_, r)| {
            !matches!(
                r.execution,
                terminal::Execution::ReadObserved | terminal::Execution::StorageAccepted
            ) || r.inventory != terminal::Inventory::NotAssessed
        })
    {
        return Err(Reason::Incomplete);
    }
    if retire {
        if !pending
            .accounting
            .as_mut()
            .unwrap()
            .acknowledge_family(&reports)
        {
            return Err(Reason::Incomplete);
        }
        for key in keys {
            pending.admission.as_mut().unwrap().slots.remove(&key.id);
        }
    }
    Ok(())
}
impl Worker {
    pub(in crate::drafts::store::worker) fn observe_upload_existing(
        &self,
        job: &Upload,
        retire: bool,
    ) -> Result<Reservation, Reason> {
        let value = job.poll().map_err(|_| Reason::Incomplete)?;
        let value = value.as_ref().as_ref().map_err(|_| Reason::Incomplete)?;
        if value.created {
            return Err(Reason::Incomplete);
        }
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let id = terminal(&pending, job, "upload-reserve", None, None)?;
        finish(&mut pending, &[job.key.clone()], &[id], retire)?;
        Ok(value.clone())
    }
    pub(in crate::drafts::store::worker) fn observe_upload_loaded(
        &self,
        job: &Operation<String, Result<Vec<Stored>, AppError>>,
        retire: bool,
    ) -> Result<Vec<Stored>, Reason> {
        let value = job.poll().map_err(|_| Reason::Incomplete)?;
        let value = value.as_ref().as_ref().map_err(|_| Reason::Incomplete)?;
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let id = terminal(&pending, job, "upload-load", None, None)?;
        finish(&mut pending, &[job.key.clone()], &[id], retire)?;
        Ok(value.clone())
    }
    pub(in crate::drafts::store::worker) fn observe_upload_retired(
        &self,
        job: &Operation<Stored, Result<bool, AppError>>,
        retire: bool,
    ) -> Result<(), Reason> {
        let value = job.poll().map_err(|_| Reason::Incomplete)?;
        if !matches!(value.as_ref(), Ok(true)) {
            return Err(Reason::Incomplete);
        }
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let id = terminal(&pending, job, "upload-retire", None, None)?;
        finish(&mut pending, &[job.key.clone()], &[id], retire)
    }
    pub(in crate::drafts::store::worker) fn observe_upload_finalized(
        &self,
        parent: &Upload,
        child: &UploadCompletion,
        retire: bool,
    ) -> Result<Stored, Reason> {
        let reserved = parent.poll().map_err(|_| Reason::Incomplete)?;
        let reserved = reserved.as_ref().as_ref().map_err(|_| Reason::Incomplete)?;
        let final_result = child.poll().map_err(|_| Reason::Incomplete)?;
        let stored = final_result
            .as_ref()
            .as_ref()
            .map_err(|_| Reason::Incomplete)?
            .as_ref()
            .ok_or(Reason::Incomplete)?;
        if !reserved.created
            || stored.ticket != reserved.stored.ticket
            || stored.ticket != child.held.input.ticket
        {
            return Err(Reason::Stale);
        }
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let first = terminal(&pending, parent, "upload-reserve", None, Some(&child.key))?;
        let last = terminal(
            &pending,
            child,
            "upload-finalize",
            Some(parent.key.id),
            None,
        )?;
        finish(
            &mut pending,
            &[parent.key.clone(), child.key.clone()],
            &[first, last],
            retire,
        )?;
        Ok(stored.clone())
    }
}

impl Worker {
    pub(in crate::drafts::store::worker) fn reattach_upload_completion(
        &self,
        parent: &Upload,
        key: &Key,
    ) -> Result<UploadCompletion, Reason> {
        let child = self.reattach::<Finalize, Result<Option<Stored>, AppError>>(key)?;
        let pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let admission = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
        let p = admission.entry(&parent.key)?;
        let c = admission.entry(key)?;
        if p.family != "upload-reserve"
            || p.parent.is_some()
            || c.family != "upload-finalize"
            || c.parent != Some(parent.key.id)
        {
            return Err(Reason::Stale);
        }
        let actual = p
            .retained
            .clone()
            .downcast::<Held<Draft, Result<Reservation, AppError>>>()
            .map_err(|_| Reason::Stale)?;
        if !Arc::ptr_eq(&actual, &parent.held)
            || actual
                .status
                .lock()
                .map_err(|_| Reason::Incomplete)?
                .next
                .as_ref()
                .is_none_or(|next| !same(next, key))
        {
            return Err(Reason::Stale);
        }
        Ok(child)
    }
}
