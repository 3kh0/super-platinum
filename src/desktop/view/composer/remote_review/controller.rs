//! The existing review surface selects one backend. Fixtures retain their old
//! checkpoint path; ordinary configured accounts use the same owned Host.
use super::{choices, read};
use crate::{
    drafts::{Fault, protected::production::baseline as owned},
    state::{ComposerTarget, ShellState},
    view::composer::field,
};
use dioxus::prelude::*;
use std::rc::Rc;
use super_platinum_core::drafts::{store::protected::Error, store::sync::RemoteChoice};
#[path = "legacy_controller.rs"]
mod legacy;
pub(super) use legacy::{Completion, Failure};

#[derive(Clone)]
enum Backend {
    Legacy(legacy::Prepared),
    Owned(owned::Prepared, Rc<field::MountLease>),
}
#[derive(Clone)]
pub(super) struct Prepared {
    backend: Backend,
    pub choices: Vec<choices::Choice>,
}
impl Prepared {
    pub fn destination_current(&self, shell: &ShellState) -> bool {
        match &self.backend {
            Backend::Legacy(v) => v.destination_current(shell),
            Backend::Owned(v, m) => m.current() && v.destination_current(shell),
        }
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        match &self.backend {
            Backend::Legacy(v) => v.current(shell),
            Backend::Owned(v, m) => m.current() && v.current(shell),
        }
    }
    pub fn team(&self) -> &str {
        match &self.backend {
            Backend::Legacy(v) => v.team(),
            Backend::Owned(v, _) => v.team(),
        }
    }
}
fn failure(error: Error) -> Failure {
    match error {
        Error::Cooldown(_) => Failure::Cooldown,
        Error::Stale => Failure::Fault(Fault::Stale),
        Error::Pending => Failure::Fault(Fault::Pending),
        Error::Storage => Failure::Fault(Fault::Storage),
        Error::Gap => Failure::Read(None),
        _ => Failure::Fault(Fault::Unsupported),
    }
}
pub(super) fn available(shell: &ShellState, target: ComposerTarget) -> bool {
    if !owned::configured_backend() {
        legacy::available(shell, target)
    } else {
        owned::available(shell, target)
    }
}
pub(super) async fn open(
    state: Signal<ShellState>,
    target: ComposerTarget,
) -> Result<Prepared, Failure> {
    if !owned::configured_backend() {
        let value = legacy::open(state, target).await?;
        return Ok(Prepared {
            choices: value.choices.clone(),
            backend: Backend::Legacy(value),
        });
    }
    let mount = field::current(&state.peek(), target).ok_or(Failure::Fault(Fault::Stale))?;
    let value = owned::open(state, target).await.map_err(failure)?;
    if !mount.current() {
        return Err(Failure::Fault(Fault::Stale));
    }
    Ok(Prepared {
        choices: choices::build_protected(&value.evidence),
        backend: Backend::Owned(value, mount),
    })
}
pub(super) async fn apply(
    state: Signal<ShellState>,
    prepared: Prepared,
    choice: RemoteChoice,
) -> Result<Completion, Failure> {
    if !prepared.current(&state.peek())
        || !prepared
            .choices
            .iter()
            .any(|row| row.choice.as_ref() == Some(&choice) && row.unavailable.is_none())
    {
        return Err(Failure::Fault(Fault::Stale));
    }
    match prepared.backend {
        Backend::Legacy(value) => legacy::apply(state, value, choice).await,
        Backend::Owned(value, _) => owned::apply(state, value, choice)
            .await
            .map(|_| Completion::Applied)
            .map_err(failure),
    }
}
pub(super) fn can_finish(shell: &ShellState, target: ComposerTarget) -> bool {
    if !owned::configured_backend() {
        legacy::can_finish(shell, target)
    } else {
        owned::can_finish(shell, target)
    }
}
pub(super) async fn finish(
    state: Signal<ShellState>,
    target: ComposerTarget,
) -> Result<Completion, Failure> {
    if !owned::configured_backend() {
        legacy::finish(state, target).await
    } else {
        owned::finish(state, target)
            .await
            .map(|_| Completion::Applied)
            .map_err(failure)
    }
}
