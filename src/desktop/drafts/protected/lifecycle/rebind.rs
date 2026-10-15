//! Explicit fresh native consent for the SAME retained installed Editor.
//! A replacement shell alone cannot call this or recover a route implicitly.
use super::*;
use crate::view::composer::protected_enrollment::{Admission, Capture, NativeIdentity};
pub(crate) struct RebindRequest {
    prior: Installed,
    selection: Selection,
    expected: durable::Review,
    serial: u64,
    registry: Arc<registry::Registry>,
    native: Option<NativeIdentity>,
    visit: Visit,
    attempt: String,
}
pub(crate) struct ReadyRebind {
    request: RebindRequest,
    _gate: tokio::sync::OwnedMutexGuard<()>,
}
impl RebindRequest {
    pub fn id(&self) -> &str {
        &self.attempt
    }
    pub async fn prepare(self) -> Result<ReadyRebind, durable::Error> {
        let owner = self.prior.editor.owner.clone();
        let guard = owner.gate.clone().lock_owned().await;
        let fresh = account::wait(owner.client()?.review(owner.scope().clone())).await?;
        if !fresh.same(&self.expected)
            || !self.visit.current()
            || !self.selection.0.live.load(Ordering::Acquire)
        {
            return Err(durable::Error::Stale);
        }
        Ok(ReadyRebind {
            request: self,
            _gate: guard,
        })
    }
}
impl ReadyRebind {
    pub fn id(&self) -> &str {
        &self.request.attempt
    }
}
impl Host {
    pub fn prepare_rebind(
        &self,
        shell: &ShellState,
        selected: &Selection,
        prior: &Installed,
        capture: &Capture,
        visit: Visit,
    ) -> Result<RebindRequest, durable::Error> {
        let expected = self
            .discovery
            .entry(shell, &prior.scope.key)
            .ok_or(durable::Error::Stale)?
            .inventory()
            .managed()?
            .clone();
        if !capture.matches(shell, expected.scope(), expected.local()) {
            return Err(durable::Error::Stale);
        }
        self.rebind_request(shell, selected, prior, Some(capture.identity()), visit)
    }
    fn rebind_request(
        &self,
        shell: &ShellState,
        selected: &Selection,
        prior: &Installed,
        native: Option<NativeIdentity>,
        visit: Visit,
    ) -> Result<RebindRequest, durable::Error> {
        self.check(shell, selected)?;
        if !Arc::ptr_eq(&prior.account, &selected.0.account)
            || self.adoptions.contains_key(&prior.adoption)
        {
            return Err(durable::Error::Pending);
        }
        if !matches!(self.routes.get(&(prior.scope.account_id.clone(),prior.scope.key.clone())),Some(route::Route::Managed(r)) if r.id==prior.id&&Arc::ptr_eq(&r.editor,&prior.editor))
        {
            return Err(durable::Error::Stale);
        }
        let expected = self
            .discovery
            .entry(shell, &prior.scope.key)
            .ok_or(durable::Error::Stale)?
            .inventory()
            .managed()?
            .clone();
        let snapshot = prior.editor.snapshot();
        if expected.row().lineage().basis() != durable::Basis::Unverified
            || expected.row().state().consumption().is_some()
            || !matches!(
                expected.row().state().workflow(),
                super_platinum_core::drafts::sync::Workflow::Idle
            )
            || !matches!(snapshot.view.inventory(),Inventory::Managed(r) if r.same(&expected))
            || snapshot.running
            || snapshot.pending.is_some()
            || snapshot.parked
            || snapshot.error.is_some()
            || !snapshot.failed_inputs.is_empty()
            || snapshot.projection.is_some()
            || snapshot.choice_retry_available
            || !visit.current()
        {
            return Err(durable::Error::ReviewRequired);
        }
        let registry = self.discovery.registry(shell)?;
        if registry
            .get(selected.0.account.id(), selected.0.epoch, &prior.scope.key)
            .is_some()
        {
            return Err(durable::Error::Stale);
        }
        Ok(RebindRequest {
            prior: prior.clone(),
            selection: selected.clone(),
            expected,
            serial: snapshot.serial,
            registry,
            native,
            visit,
            attempt: barrier_id(),
        })
    }
    pub fn rebind_native(
        &mut self,
        shell: &ShellState,
        ready: ReadyRebind,
        admission: &Admission,
    ) -> Result<Installed, durable::Error> {
        let r = &ready.request;
        if !r.native.as_ref().is_some_and(|identity| {
            admission.authorizes(identity, shell, r.expected.scope(), r.expected.local())
        }) {
            return Err(durable::Error::Stale);
        }
        self.commit_rebind(shell, ready)
    }
    fn commit_rebind(
        &mut self,
        shell: &ShellState,
        ready: ReadyRebind,
    ) -> Result<Installed, durable::Error> {
        let r = &ready.request;
        self.check(shell, &r.selection)?;
        if !Arc::ptr_eq(&self.discovery.registry(shell)?, &r.registry)
            || !matches!(self.routes.get(&(r.prior.scope.account_id.clone(),r.prior.scope.key.clone())),Some(route::Route::Managed(old)) if old.id==r.prior.id&&Arc::ptr_eq(&old.editor,&r.prior.editor))
        {
            return Err(durable::Error::Stale);
        }
        let editor = &r.prior.editor;
        let state = editor.state.lock().map_err(|_| durable::Error::Storage)?;
        if state.retired
            || state.serial != r.serial
            || !state.review.same(&r.expected)
            || state.running
            || state.busy
            || state.parked
            || state.pending.is_some()
            || state.error.is_some()
            || state.retry.is_some()
            || state.barrier.is_some()
            || state.released.is_some()
            || state.handoff_lost
            || state.choice_retry.is_some()
            || !r.visit.claim()
        {
            return Err(durable::Error::Stale);
        }
        r.registry.replace(
            r.selection.0.account.id(),
            r.selection.0.epoch,
            &r.prior.scope.key,
            None,
            editor.clone(),
        )?;
        let receipt = Installed {
            id: barrier_id(),
            adoption: r.prior.adoption.clone(),
            attempt: r.attempt.clone(),
            scope: r.prior.scope.clone(),
            lane: r.prior.lane.clone(),
            serial: r.prior.serial,
            expected: r.expected.clone(),
            editor_serial: r.serial,
            rebound_from: Some(r.prior.id.clone()),
            account: r.prior.account.clone(),
            editor: editor.clone(),
        };
        self.routes.insert(
            (receipt.scope.account_id.clone(), receipt.scope.key.clone()),
            route::Route::Managed(receipt.clone()),
        );
        Ok(receipt)
    }
    /// Called only after the exact Editor's existing clean retirement completed.
    /// It cannot discard pending input, revive a writer or remove a replacement.
    pub fn forget_retired_installation(
        &mut self,
        shell: &ShellState,
        receipt: &Installed,
    ) -> Result<(), durable::Error> {
        let selected = self.selected.as_ref().ok_or(durable::Error::Stale)?;
        self.check(shell, selected)?;
        if self.adoptions.contains_key(&receipt.adoption)
            || !Arc::ptr_eq(&selected.0.account, &receipt.account)
            || !matches!(self.routes.get(&(receipt.scope.account_id.clone(),receipt.scope.key.clone())),Some(route::Route::Managed(old)) if old.id==receipt.id&&Arc::ptr_eq(&old.editor,&receipt.editor))
            || !receipt
                .editor
                .state
                .lock()
                .map_err(|_| durable::Error::Storage)?
                .retired
        {
            return Err(durable::Error::Stale);
        }
        self.discovery.registry(shell)?.remove(
            receipt.account.id(),
            selected.0.epoch,
            &receipt.scope.key,
            &receipt.editor,
        );
        self.routes
            .remove(&(receipt.scope.account_id.clone(), receipt.scope.key.clone()));
        Ok(())
    }
}

#[cfg(test)]
mod tests;
