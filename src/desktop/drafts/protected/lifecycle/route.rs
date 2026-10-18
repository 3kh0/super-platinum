//! One explicit per-scope route. Selected protected failures never fall back.
use super::*;
pub(super) enum Route {
    Local(String),
    OpeningManaged(String),
    Managed(Installed),
}
#[derive(serde::Serialize)]
pub(crate) struct RouteSnapshot {
    pub kind: &'static str,
    pub identity: String,
    pub serial: Option<u64>,
    pub writer: Option<String>,
    pub enrollment: Option<String>,
    pub registry_current: bool,
}
impl Host {
    pub fn submit(
        &self,
        shell: &ShellState,
        selected: &Selection,
        key: &DraftKey,
        local: &LocalDraft,
    ) -> Result<u64, durable::Error> {
        self.check(shell, selected)?;
        match self
            .routes
            .get(&(selected.0.account.id().into(), key.clone()))
            .ok_or(durable::Error::Stale)?
        {
            Route::Local(id) => {
                let slot = self.locals.get(id).ok_or(durable::Error::Stale)?;
                if !Arc::ptr_eq(&slot.account, &selected.0.account) {
                    return Err(durable::Error::Stale);
                }
                super::super::retention::input_size(local)?;
                let mut pending = slot.pending.lock().map_err(|_| durable::Error::Storage)?;
                match slot.job.result() {
                    Ok(lane) => {
                        let result = lane.writer().submit(local);
                        if result.is_ok() {
                            *pending = None;
                        } else {
                            *pending = Some(local.clone());
                        }
                        result
                    }
                    Err(error) => {
                        *pending = Some(local.clone());
                        Err(error)
                    }
                }
            }
            Route::OpeningManaged(id) => {
                let slot = self.managed.get(id).ok_or(durable::Error::Stale)?;
                if !Arc::ptr_eq(&slot.account, &selected.0.account) {
                    return Err(durable::Error::Stale);
                }
                slot.submit(local)
            }
            Route::Managed(receipt) => {
                if !Arc::ptr_eq(&receipt.account, &selected.0.account)
                    || !self.discovery.registry(shell)?.accepts_retained_input(
                        selected.0.account.id(),
                        selected.0.epoch,
                        key,
                        &receipt.editor,
                    )
                {
                    return Err(durable::Error::Stale);
                }
                // Selection/account/route remain exact even when replacement
                // invalidates renderer installation. This transfers bounded
                // input custody to the SAME Editor, without registering it or
                // granting send/rebind authority. Explicit native rebind follows.
                receipt.editor.submit(local)
            }
        }
    }
    pub fn installation(&self, account: &Arc<Account>, key: &DraftKey) -> Option<Installed> {
        match self.routes.get(&(account.id().into(), key.clone()))? {
            Route::Managed(receipt) if Arc::ptr_eq(account, &receipt.account) => {
                Some(receipt.clone())
            }
            _ => None,
        }
    }
    pub fn route_snapshot(
        &self,
        shell: &ShellState,
        key: &DraftKey,
    ) -> Result<RouteSnapshot, durable::Error> {
        let selected = self.selected.as_ref().ok_or(durable::Error::Stale)?;
        self.check(shell, selected)?;
        match self
            .routes
            .get(&(selected.0.account.id().into(), key.clone()))
            .ok_or(durable::Error::Stale)?
        {
            Route::Local(id) => Ok(RouteSnapshot {
                kind: "local",
                identity: id.clone(),
                serial: self
                    .locals
                    .get(id)
                    .and_then(|s| s.job.result().ok())
                    .map(|l| l.writer().snapshot().serial),
                writer: None,
                enrollment: None,
                registry_current: false,
            }),
            Route::OpeningManaged(id) => {
                let slot = self.managed.get(id).ok_or(durable::Error::Stale)?;
                let snapshot = slot.job.result().ok().map(|editor| editor.snapshot());
                Ok(RouteSnapshot {
                    kind: "opening-managed",
                    identity: id.clone(),
                    serial: snapshot.as_ref().map(|s| s.serial),
                    writer: snapshot.as_ref().map(|s| s.writer_id.clone()),
                    enrollment: snapshot.map(|s| s.enrollment),
                    registry_current: false,
                })
            }
            Route::Managed(receipt) => {
                let snapshot = receipt.editor.snapshot();
                Ok(RouteSnapshot {
                    kind: "managed",
                    identity: receipt.id().into(),
                    serial: Some(snapshot.serial),
                    writer: Some(snapshot.writer_id),
                    enrollment: Some(snapshot.enrollment),
                    registry_current: self.discovery.registry(shell)?.current(
                        selected.0.account.id(),
                        selected.0.epoch,
                        key,
                        &receipt.editor,
                    ),
                })
            }
        }
    }
}
