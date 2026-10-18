//! Memory routing only after exact current Review and real native custody.
use super::*;
use crate::view::composer::protected_enrollment::{Admission, Capture, NativeIdentity};
pub(crate) struct ManagedInstallRequest {
    attempt: String,
    opening: String,
    selection: Selection,
    expected: durable::Review,
    serial: u64,
    editor: Arc<Editor>,
    registry: Arc<registry::Registry>,
    native: Option<NativeIdentity>,
    visit: Visit,
}
pub(crate) struct ReadyManagedInstall {
    request: ManagedInstallRequest,
    _gate: tokio::sync::OwnedMutexGuard<()>,
}
impl ManagedInstallRequest {
    pub fn id(&self) -> &str {
        &self.attempt
    }
    pub async fn prepare(self) -> Result<ReadyManagedInstall, durable::Error> {
        let owner = &self.editor.owner;
        let gate = owner.gate.clone().lock_owned().await;
        let fresh = if let Some(route) = &owner.account.owned {
            route.review(owner.scope().clone()).await?
        } else {
            account::wait(owner.client()?.review(owner.scope().clone())).await?
        };
        if !fresh.same(&self.expected)
            || !self.visit.current()
            || !self.selection.0.live.load(Ordering::Acquire)
        {
            return Err(durable::Error::Stale);
        }
        // This owner gate does not fence foreign SQLite writers. Later writes
        // still use the existing exact Review CAS; no remote authority is added.
        Ok(ReadyManagedInstall {
            request: self,
            _gate: gate,
        })
    }
}
impl ReadyManagedInstall {
    pub fn id(&self) -> &str {
        &self.request.attempt
    }
}
fn idle(state: &super::super::super::editor::State) -> bool {
    state.review.row().state().consumption().is_none()
        && !state.retired
        && !state.running
        && !state.busy
        && !state.parked
        && state.preserved.is_none()
        && state.pending.is_none()
        && state.error.is_none()
        && state.retry.is_none()
        && state.barrier.is_none()
        && state.released.is_none()
        && !state.handoff_lost
        && state.handoff_expected.is_none()
        && state.choice_retry.is_none()
}
impl Host {
    pub fn prepare_managed_installation(
        &self,
        shell: &ShellState,
        selected: &Selection,
        opened: &ManagedOpen,
        capture: &Capture,
        visit: Visit,
    ) -> Result<ManagedInstallRequest, durable::Error> {
        let request =
            self.managed_install_request(shell, selected, opened, Some(capture.identity()), visit)?;
        if !capture.matches(shell, request.expected.scope(), request.expected.local()) {
            return Err(durable::Error::Stale);
        }
        Ok(request)
    }
    pub(super) fn managed_install_request(
        &self,
        shell: &ShellState,
        selected: &Selection,
        opened: &ManagedOpen,
        native: Option<NativeIdentity>,
        visit: Visit,
    ) -> Result<ManagedInstallRequest, durable::Error> {
        self.check(shell, selected)?;
        let slot = self.managed.get(&opened.id).ok_or(durable::Error::Stale)?;
        if !Arc::ptr_eq(&slot.account, &selected.0.account) || !visit.current() {
            return Err(durable::Error::Stale);
        }
        let pending = slot.pending.lock().map_err(|_| durable::Error::Storage)?;
        if pending.is_some() {
            return Err(durable::Error::Pending);
        }
        let editor = slot.job.result()?;
        let state = editor.state.lock().map_err(|_| durable::Error::Storage)?;
        if !idle(&state) {
            return Err(durable::Error::ReviewRequired);
        }
        let registry = self.discovery.registry(shell)?;
        if registry
            .get(
                selected.0.account.id(),
                selected.0.epoch,
                &slot.expected.scope().key,
            )
            .is_some()
        {
            return Err(durable::Error::Stale);
        }
        if !matches!(self.routes.get(&(slot.expected.scope().account_id.clone(),slot.expected.scope().key.clone())),Some(route::Route::OpeningManaged(id)) if id==&opened.id)
        {
            return Err(durable::Error::Stale);
        }
        let expected = state.review.clone();
        let serial = state.serial;
        drop(state);
        Ok(ManagedInstallRequest {
            attempt: barrier_id(),
            opening: opened.id.clone(),
            selection: selected.clone(),
            expected,
            serial,
            editor,
            registry,
            native,
            visit,
        })
    }
    pub fn install_managed_native(
        &mut self,
        shell: &ShellState,
        ready: ReadyManagedInstall,
        admission: &Admission,
    ) -> Result<Installed, durable::Error> {
        let r = &ready.request;
        if !r.native.as_ref().is_some_and(|identity| {
            admission.authorizes(identity, shell, r.expected.scope(), r.expected.local())
        }) {
            return Err(durable::Error::Stale);
        }
        self.commit_managed_installation(shell, ready)
    }
    pub(super) fn commit_managed_installation(
        &mut self,
        shell: &ShellState,
        ready: ReadyManagedInstall,
    ) -> Result<Installed, durable::Error> {
        let r = &ready.request;
        self.check(shell, &r.selection)?;
        let slot = self.managed.get(&r.opening).ok_or(durable::Error::Stale)?;
        let key = (
            r.expected.scope().account_id.clone(),
            r.expected.scope().key.clone(),
        );
        if !Arc::ptr_eq(&slot.account, &r.selection.0.account)
            || !Arc::ptr_eq(&slot.job.result()?, &r.editor)
            || !Arc::ptr_eq(&self.discovery.registry(shell)?, &r.registry)
            || !matches!(self.routes.get(&key),Some(route::Route::OpeningManaged(id)) if id==&r.opening)
        {
            return Err(durable::Error::Stale);
        }
        // Retention inventory never holds the Editor state lock. Resolve the
        // unique retained identity first, then use pending -> Editor -> Registry.
        if existing(&slot.account, r.expected.scope())?.is_none_or(|e| !Arc::ptr_eq(&e, &r.editor))
        {
            return Err(durable::Error::ReviewRequired);
        }
        let pending = slot.pending.lock().map_err(|_| durable::Error::Storage)?;
        let state = r.editor.state.lock().map_err(|_| durable::Error::Storage)?;
        if pending.is_some()
            || !idle(&state)
            || state.serial != r.serial
            || !state.review.same(&r.expected)
            || !r.visit.claim()
        {
            return Err(durable::Error::Stale);
        }
        r.registry.replace(
            r.selection.0.account.id(),
            r.selection.0.epoch,
            &r.expected.scope().key,
            None,
            r.editor.clone(),
        )?;
        let receipt = Installed {
            id: barrier_id(),
            adoption: r.opening.clone(),
            attempt: r.attempt.clone(),
            scope: r.expected.scope().clone(),
            lane: r.opening.clone(),
            serial: r.serial,
            expected: r.expected.clone(),
            editor_serial: r.serial,
            rebound_from: None,
            account: r.selection.0.account.clone(),
            editor: r.editor.clone(),
        };
        drop(state);
        drop(pending);
        self.routes
            .insert(key, route::Route::Managed(receipt.clone()));
        self.managed.remove(&r.opening);
        Ok(receipt)
    }
    #[cfg(test)]
    pub(in super::super) async fn install_managed_without_native_for_test(
        &mut self,
        shell: &ShellState,
        selected: &Selection,
        opened: &ManagedOpen,
    ) -> Result<Installed, durable::Error> {
        // Executes the real worker read/owner checks/registry transition.
        // Only renderer Capture/Admission is intentionally absent in unit tests.
        let ready = self
            .managed_install_request(shell, selected, opened, None, Visit::new())?
            .prepare()
            .await?;
        self.commit_managed_installation(shell, ready)
    }
}
