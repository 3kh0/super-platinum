//! Local routing only: no text projection, remote authority or registration replay.
use super::super::editor::Editor;
use super::*;
use crate::view::composer::protected_enrollment::{Admission, Capture, NativeIdentity};
#[cfg(test)]
pub(super) mod tests;

pub(crate) struct InstallRequest {
    attempt: String,
    adoption: String,
    selection: Selection,
    lane: Arc<Lane>,
    serial: u64,
    expected: durable::Review,
    editor: Arc<Editor>,
    registry: Arc<registry::Registry>,
    previous: Option<Arc<Editor>>,
    native: Option<NativeIdentity>,
    visit: Visit,
}
pub(crate) struct ReadyInstall {
    request: InstallRequest,
    _owner_gate: tokio::sync::OwnedMutexGuard<()>,
}
/// Receipt records a completed memory transition, not fresh native authority.
#[derive(Clone)]
pub(crate) struct Installed {
    pub(super) id: String,
    pub(super) adoption: String,
    pub(super) attempt: String,
    pub(super) scope: Scope,
    pub(super) lane: String,
    pub(super) serial: u64,
    pub(super) expected: durable::Review,
    pub(super) editor_serial: u64,
    pub(super) rebound_from: Option<String>,
    pub(super) account: Arc<Account>,
    pub(super) editor: Arc<Editor>,
}
impl Installed {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn attempt_id(&self) -> &str {
        &self.attempt
    }
    pub fn expected(&self) -> &durable::Review {
        &self.expected
    }
    pub fn editor_serial(&self) -> u64 {
        self.editor_serial
    }
    pub fn rebound_from(&self) -> Option<&str> {
        self.rebound_from.as_deref()
    }
    pub fn adoption_id(&self) -> &str {
        &self.adoption
    }
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn lane_id(&self) -> &str {
        &self.lane
    }
    pub fn lane_serial(&self) -> u64 {
        self.serial
    }
    pub fn editor(&self) -> &Arc<Editor> {
        &self.editor
    }
}
impl InstallRequest {
    pub fn id(&self) -> &str {
        &self.attempt
    }
    /// Original Review is re-read under Owner serialization. This does NOT lock
    /// SQLite against independent FIFO writers during the subsequent native wait.
    /// Their changes are caught by the next input's full checkpoint CAS.
    pub async fn prepare(self) -> Result<ReadyInstall, durable::Error> {
        let owner = self.editor.owner.clone();
        let guard = owner.gate.clone().lock_owned().await;
        let fresh = if let Some(route) = &owner.account.owned {
            route.review(owner.scope().clone()).await?
        } else {
            account::wait(owner.client()?.review(owner.scope().clone())).await?
        };
        if !fresh.same(&self.expected)
            || !self.selection.0.live.load(Ordering::Acquire)
            || !self.visit.current()
        {
            return Err(durable::Error::Stale);
        }
        Ok(ReadyInstall {
            request: self,
            _owner_gate: guard,
        })
    }
}
impl ReadyInstall {
    pub fn id(&self) -> &str {
        &self.request.attempt
    }
}
impl Host {
    pub fn prepare_installation(
        &self,
        shell: &ShellState,
        request: &Adoption,
        capture: &Capture,
        visit: Visit,
    ) -> Result<InstallRequest, durable::Error> {
        let slot = self
            .adoptions
            .get(&request.id)
            .ok_or(durable::Error::Stale)?;
        if !capture.matches(shell, slot.expected.scope(), slot.expected.local()) {
            return Err(durable::Error::Stale);
        }
        self.install_request(shell, request, Some(capture.identity()), visit)
    }
    fn install_request(
        &self,
        shell: &ShellState,
        request: &Adoption,
        native: Option<NativeIdentity>,
        visit: Visit,
    ) -> Result<InstallRequest, durable::Error> {
        let result = self.adoption(shell, request)?;
        if !result.current || !visit.current() {
            return Err(durable::Error::Stale);
        }
        let slot = self
            .adoptions
            .get(&request.id)
            .ok_or(durable::Error::Stale)?;
        let registry = self.discovery.registry(shell)?;
        let previous = registry.get(
            &slot.expected.scope().account_id,
            slot.selection.0.epoch,
            &slot.expected.scope().key,
        );
        // First local enrollment cannot displace an existing managed route.
        if previous.is_some() {
            return Err(durable::Error::ReviewRequired);
        }
        let route = self.routes.get(&(
            slot.expected.scope().account_id.clone(),
            slot.expected.scope().key.clone(),
        ));
        if !matches!(route, Some(route::Route::Local(id)) if id == slot.lane.id()) {
            return Err(durable::Error::Stale);
        }
        Ok(InstallRequest {
            attempt: barrier_id(),
            adoption: request.id.clone(),
            selection: slot.selection.clone(),
            lane: slot.lane.clone(),
            serial: slot.serial,
            expected: slot.expected.clone(),
            editor: result.editor,
            registry,
            previous,
            native,
            visit,
        })
    }
    /// Only the renderer's real, exact original Capture can create Admission.
    /// There is no production bool/closure/fake-proof entry to the commit below.
    pub fn install_native(
        &mut self,
        shell: &ShellState,
        ready: ReadyInstall,
        admission: &Admission,
    ) -> Result<Installed, durable::Error> {
        let r = &ready.request;
        if !r.native.as_ref().is_some_and(|identity| {
            admission.authorizes(identity, shell, r.expected.scope(), r.expected.local())
        }) {
            return Err(durable::Error::Stale);
        }
        self.commit_installation(shell, ready)
    }
    fn commit_installation(
        &mut self,
        shell: &ShellState,
        ready: ReadyInstall,
    ) -> Result<Installed, durable::Error> {
        let r = &ready.request;
        self.check(shell, &r.selection)?;
        let slot = self
            .adoptions
            .get(&r.adoption)
            .ok_or(durable::Error::Stale)?;
        let route_key = (
            r.expected.scope().account_id.clone(),
            r.expected.scope().key.clone(),
        );
        if slot.installed.is_some()
            || !slot.expected.same(&r.expected)
            || slot.serial != r.serial
            || !Arc::ptr_eq(&slot.lane, &r.lane)
            || !Arc::ptr_eq(&slot.job.result()?, &r.editor)
            || !Arc::ptr_eq(&self.discovery.registry(shell)?, &r.registry)
            || !matches!(self.routes.get(&route_key), Some(route::Route::Local(id)) if id == r.lane.id())
        {
            return Err(durable::Error::Stale);
        }
        if !r.visit.claim() {
            return Err(durable::Error::Stale);
        }
        // Closed actor transition: Editor -> LocalWriter -> Registry locks.
        // On any error the old local route is still live and no field changed.
        r.lane.writer().install_registered(
            r.serial,
            &r.expected,
            &r.registry,
            r.selection.0.epoch,
            r.previous.as_ref(),
            &r.editor,
        )?;
        let receipt = Installed {
            id: barrier_id(),
            adoption: r.adoption.clone(),
            attempt: r.attempt.clone(),
            expected: r.expected.clone(),
            editor_serial: 0,
            rebound_from: None,
            scope: r.expected.scope().clone(),
            lane: r.lane.id().into(),
            serial: r.serial,
            account: r.selection.0.account.clone(),
            editor: r.editor.clone(),
        };
        self.routes
            .insert(route_key, route::Route::Managed(receipt.clone()));
        self.locals.remove(r.lane.id());
        self.adoptions
            .get_mut(&r.adoption)
            .expect("validated adoption")
            .installed = Some(receipt.clone());
        Ok(receipt)
    }
    /// Free completed job capacity, not unresolved evidence. The route retains
    /// its receipt and Editor; old cloned Adoption/Ready handles become stale.
    pub fn acknowledge_installation(&mut self, receipt: &Installed) -> Result<(), durable::Error> {
        let slot = self
            .adoptions
            .get(&receipt.adoption)
            .ok_or(durable::Error::Stale)?;
        let route = self
            .routes
            .get(&(receipt.scope.account_id.clone(), receipt.scope.key.clone()));
        if !slot
            .installed
            .as_ref()
            .is_some_and(|v| v.id == receipt.id && Arc::ptr_eq(&v.editor, &receipt.editor))
            || !matches!(route, Some(route::Route::Managed(v)) if v.id == receipt.id && Arc::ptr_eq(&v.editor, &receipt.editor))
        {
            return Err(durable::Error::Stale);
        }
        self.adoptions.remove(&receipt.adoption);
        Ok(())
    }
}

#[cfg(test)]
impl Host {
    /// Real registration/read/commit; intentionally bypasses only native proof.
    pub(in super::super) async fn install_without_native_for_test(
        &mut self,
        shell: &ShellState,
        adoption: &Adoption,
    ) -> Result<Installed, durable::Error> {
        let ready = self
            .install_request(shell, adoption, None, Visit::new())?
            .prepare()
            .await?;
        let receipt = self.commit_installation(shell, ready)?;
        self.acknowledge_installation(&receipt)?;
        Ok(receipt)
    }
}
