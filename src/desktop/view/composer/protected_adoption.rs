//! Original native evidence survives registration and the final durable read.
//! Only routing changes; this module never clears or projects native content.
use super::protected_enrollment::{Admission, Capture};
use crate::{
    drafts::protected::{
        Visit,
        lifecycle::{Adoption, Host, Installed, ReadyInstall, ReadyRebind, Selection},
    },
    state::ShellState,
};
use dioxus::prelude::*;
use std::sync::Mutex;
use super_platinum_core::drafts::store::protected::Error;
pub(crate) struct Prepared {
    ready: ReadyInstall,
    capture: Capture,
}
pub(crate) struct Locked {
    ready: ReadyInstall,
    admission: Admission,
}
pub(crate) async fn prepare(
    state: Signal<ShellState>,
    host: &Mutex<Host>,
    adoption: &Adoption,
    capture: Capture,
    visit: Visit,
) -> Result<Prepared, Error> {
    let request = {
        let shell = state.try_peek().map_err(|_| Error::Stale)?;
        host.lock()
            .map_err(|_| Error::Storage)?
            .prepare_installation(&shell, adoption, &capture, visit)?
    };
    let ready = request.prepare().await?;
    Ok(Prepared { ready, capture })
}
impl Prepared {
    pub fn id(&self) -> &str {
        self.ready.id()
    }
    pub async fn lock(self) -> Result<Locked, Error> {
        let admission = self.capture.lock().await?;
        Ok(Locked {
            ready: self.ready,
            admission,
        })
    }
}
impl Locked {
    pub fn id(&self) -> &str {
        self.ready.id()
    }
    /// No await between real admission validation and atomic route installation.
    /// Consuming this value releases only its native lease after the operation.
    pub fn install(self, host: &mut Host, shell: &ShellState) -> Result<Installed, Error> {
        host.install_native(shell, self.ready, &self.admission)
    }
}

/// Owner return is a new native admission for the same retained Editor.
/// No registry repair occurs merely because selection returned to the account.
pub(crate) struct PreparedRebind {
    ready: ReadyRebind,
    capture: Capture,
}
pub(crate) struct LockedRebind {
    ready: ReadyRebind,
    admission: Admission,
}
pub(crate) async fn prepare_rebind(
    state: Signal<ShellState>,
    host: &Mutex<Host>,
    selected: &Selection,
    prior: &Installed,
    capture: Capture,
    visit: Visit,
) -> Result<PreparedRebind, Error> {
    let request = {
        let shell = state.try_peek().map_err(|_| Error::Stale)?;
        host.lock()
            .map_err(|_| Error::Storage)?
            .prepare_rebind(&shell, selected, prior, &capture, visit)?
    };
    let ready = request.prepare().await?;
    Ok(PreparedRebind { ready, capture })
}
impl PreparedRebind {
    pub fn id(&self) -> &str {
        self.ready.id()
    }
    pub async fn lock(self) -> Result<LockedRebind, Error> {
        let admission = self.capture.lock().await?;
        Ok(LockedRebind {
            ready: self.ready,
            admission,
        })
    }
}
impl LockedRebind {
    pub fn id(&self) -> &str {
        self.ready.id()
    }
    pub fn install(self, host: &mut Host, shell: &ShellState) -> Result<Installed, Error> {
        host.rebind_native(shell, self.ready, &self.admission)
    }
}
