//! Finished Send -> existing LocalWriter. All fallible admission precedes removal.
use super::*;
use crate::drafts::dispatch::Outcome;
impl Host {
    pub(crate) fn complete_sent(
        &mut self,
        shell: &ShellState,
        account: &Arc<Account>,
        key: &DraftKey,
        operation: &operation::Operation,
    ) -> Result<(), durable::Error> {
        let selected = self.selected.as_ref().ok_or(durable::Error::Stale)?;
        self.check(shell, selected)?;
        if !Arc::ptr_eq(&selected.0.account, account) || self.locals.len() >= MAX_LOCAL_LANES {
            return Err(durable::Error::Pending);
        }
        let prior = self
            .installation(account, key)
            .ok_or(durable::Error::Stale)?;
        let editor = prior.editor().clone();
        if !operation.belongs(&editor)
            || !matches!(&operation.record().request, operation::Request::Send)
            || self.adoptions.contains_key(prior.adoption_id())
        {
            return Err(durable::Error::Stale);
        }
        if account
            .retained_operations()?
            .iter()
            .any(|v| v.belongs(&editor) && !v.same(operation))
        {
            return Err(durable::Error::ReviewRequired);
        }
        let (send, release, review, lease) = {
            let evidence = operation
                .record()
                .evidence
                .lock()
                .map_err(|_| durable::Error::Storage)?;
            if evidence.status != operation::Status::Finished
                || evidence.handoff != Some(Ok(()))
                || !evidence.finished.as_ref().is_some_and(|v| {
                    v.local_handoff.is_ok()
                        && v.inventory_published
                        && matches!(&v.dispatched.attempt.outcome, Outcome::Sent { .. })
                })
            {
                return Err(durable::Error::Pending);
            }
            (
                evidence.owned_send.clone().ok_or(durable::Error::Pending)?,
                evidence
                    .owned_release
                    .clone()
                    .ok_or(durable::Error::Pending)?,
                evidence.recorded.clone().ok_or(durable::Error::Pending)?,
                evidence
                    .local_lease
                    .clone()
                    .ok_or(durable::Error::Pending)?,
            )
        };
        let registry = self.discovery.registry(shell)?;
        let mut state = editor.state.lock().map_err(|_| durable::Error::Storage)?;
        if state.retired
            || state.busy
            || state.running
            || state.parked
            || state.pending.is_some()
            || state.error.is_some()
            || state.retry.is_some()
            || state.choice_retry.is_some()
            || state.barrier.is_some()
            || state.handoff_lost
            || state.handoff_expected.is_some()
        {
            return Err(durable::Error::ReviewRequired);
        }
        let writer = state.released.clone().ok_or(durable::Error::Pending)?;
        if !writer.matches_lease(&lease) {
            return Err(durable::Error::Stale);
        }
        let mut retained = account
            .editors
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        if retained
            .get(editor.retained_id())
            .and_then(Option::as_ref)
            .is_none_or(|v| !Arc::ptr_eq(v, &editor))
            || !registry.current(account.id(), selected.epoch(), key, &editor)
        {
            return Err(durable::Error::Stale);
        }
        // Repeated completion handling rejoins each acknowledgment already held
        // by this exact desktop family. A fenced/failed receipt remains retained.
        send.acknowledge_sent(&review)?;
        release.acknowledge_handoff(&lease)?;
        let id = barrier_id();
        let job = Job::new();
        job.completion().finish(Ok(Arc::new(Lane {
            id: id.clone(),
            account: account.clone(),
            scope: prior.scope().clone(),
            writer,
        })));
        if !registry.remove(account.id(), selected.epoch(), key, &editor) {
            return Err(durable::Error::Stale);
        }
        // No await or fallible call follows removal. Pending/failed input in the
        // LocalWriter is carried intact; old Editor clones cannot submit again.
        state.retired = true;
        retained.remove(editor.retained_id());
        self.locals.insert(
            id.clone(),
            Slot {
                account: account.clone(),
                scope: prior.scope().clone(),
                job,
                pending: Arc::new(std::sync::Mutex::new(None)),
            },
        );
        self.routes
            .insert((account.id().into(), key.clone()), route::Route::Local(id));
        Ok(())
    }
}

#[cfg(test)]
impl Host {
    /// Test setup only: the supplied Editor already came from actual registration.
    pub(in super::super::super) fn seed_registered_for_test(
        &mut self,
        shell: &ShellState,
        editor: Arc<editor::Editor>,
    ) -> Result<(), durable::Error> {
        let account = editor.owner.account.clone();
        let selected = self.select(shell, account.clone())?;
        let expected = editor.snapshot().view.inventory().managed()?.clone();
        self.discovery.registry(shell)?.replace(
            account.id(),
            selected.epoch(),
            &expected.scope().key,
            None,
            editor.clone(),
        )?;
        let receipt = Installed {
            id: barrier_id(),
            adoption: barrier_id(),
            attempt: barrier_id(),
            scope: expected.scope().clone(),
            lane: barrier_id(),
            serial: 0,
            expected,
            editor_serial: editor.snapshot().serial,
            rebound_from: None,
            account,
            editor,
        };
        self.routes.insert(
            (receipt.scope.account_id.clone(), receipt.scope.key.clone()),
            route::Route::Managed(receipt),
        );
        Ok(())
    }
}
