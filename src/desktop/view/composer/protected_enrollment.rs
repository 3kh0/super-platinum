//! Opt-in enrollment consent uses the same native lease as protected Consume.
//! It never clears or projects a field, creates a writer, or dispatches HTTP.
use super::{field, field_scope, protected_admission};
use crate::state::{ComposerTarget, ShellState};
use dioxus::prelude::*;
use std::rc::Rc;
use super_platinum_core::drafts::{DraftKey, LocalDraft, store::protected::Error};

#[derive(Clone)]
pub(crate) struct NativeIdentity(Rc<()>);
impl NativeIdentity {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
pub(crate) struct Capture {
    identity: NativeIdentity,
    state: Signal<ShellState>,
    target: ComposerTarget,
    account: String,
    epoch: u64,
    view: u64,
    key: DraftKey,
    local: LocalDraft,
    mount: Rc<field::MountLease>,
    evidence: protected_admission::Evidence,
}
pub(crate) struct Admission {
    capture: Capture,
    lease: protected_admission::Lease,
}
fn local(s: &ShellState, target: ComposerTarget) -> LocalDraft {
    match target {
        ComposerTarget::Channel => LocalDraft {
            composer: s.core.composer.clone(),
            attachments: s.core.composer_attachments.clone(),
        },
        ComposerTarget::Thread => LocalDraft {
            composer: s.core.thread_composer.clone(),
            attachments: s.core.thread_composer_attachments.clone(),
        },
    }
}
fn same(a: &LocalDraft, b: &LocalDraft) -> bool {
    a.attachments.is_empty() && b.attachments.is_empty() && a.composer == b.composer
}
pub(crate) async fn capture(
    state: Signal<ShellState>,
    target: ComposerTarget,
    expected: &LocalDraft,
) -> Result<Capture, Error> {
    let (account, epoch, view, key, mount) = {
        let s = state.try_peek().map_err(|_| Error::Stale)?;
        let key = s.composer_key(target).ok_or(Error::Stale)?;
        let mount = field::current(&s, target).ok_or(Error::Stale)?;
        if !same(&local(&s, target), expected)
            || !s
                .core
                .drafts
                .items
                .get(&key)
                .is_some_and(|v| same(v, expected))
        {
            return Err(Error::ReviewRequired);
        }
        (
            s.core.active_account.clone().ok_or(Error::Stale)?,
            s.core.account_epoch,
            s.draft_writes.view_id,
            key,
            mount,
        )
    };
    let id = if target == ComposerTarget::Thread {
        "thread-composer"
    } else {
        "channel-composer"
    };
    let evidence = protected_admission::Evidence::capture(id, &mount, &expected.composer.text)
        .await
        .map_err(|failure| match failure {
            protected_admission::Failure::Changed(_) => Error::ReviewRequired,
            protected_admission::Failure::Deferred => Error::Stale,
        })?;
    let capture = Capture {
        identity: NativeIdentity(Rc::new(())),
        state,
        target,
        account,
        epoch,
        view,
        key,
        local: expected.clone(),
        mount,
        evidence,
    };
    if !state.try_peek().is_ok_and(|s| capture.current(&s)) {
        return Err(Error::Stale);
    }
    Ok(capture)
}
impl Capture {
    pub(crate) fn identity(&self) -> NativeIdentity {
        self.identity.clone()
    }
    pub(crate) fn matches(
        &self,
        shell: &ShellState,
        scope: &super_platinum_core::drafts::sync::Scope,
        expected: &LocalDraft,
    ) -> bool {
        self.account == scope.account_id
            && self.key == scope.key
            && same(&self.local, expected)
            && self.current(shell)
    }
    pub fn current(&self, s: &ShellState) -> bool {
        self.mount.current()
            && field_scope(s, self.target) == self.mount.scope
            && s.core.active_account.as_deref() == Some(self.account.as_str())
            && s.core.account_epoch == self.epoch
            && s.draft_writes.view_id == self.view
            && s.composer_key(self.target).as_ref() == Some(&self.key)
            && same(&local(s, self.target), &self.local)
            && s.core
                .drafts
                .items
                .get(&self.key)
                .is_some_and(|v| same(v, &self.local))
    }
    pub async fn lock(self) -> Result<Admission, Error> {
        let lease =
            self.evidence
                .lock(self.mount.clone())
                .await
                .map_err(|failure| match failure {
                    protected_admission::Failure::Changed(_) => Error::ReviewRequired,
                    protected_admission::Failure::Deferred => Error::Stale,
                })?;
        if !self.state.try_peek().is_ok_and(|s| self.current(&s)) {
            return Err(Error::Stale);
        }
        Ok(Admission {
            capture: self,
            lease,
        })
    }
}
impl Admission {
    pub(crate) fn matches(
        &self,
        shell: &ShellState,
        scope: &super_platinum_core::drafts::sync::Scope,
        expected: &LocalDraft,
    ) -> bool {
        self.capture.matches(shell, scope, expected) && self.current(shell)
    }
    pub(crate) fn authorizes(
        &self,
        identity: &NativeIdentity,
        shell: &ShellState,
        scope: &super_platinum_core::drafts::sync::Scope,
        expected: &LocalDraft,
    ) -> bool {
        self.capture.identity.same(identity)
            && self.capture.matches(shell, scope, expected)
            && self.current(shell)
    }
    /// Synchronous final check immediately before the real FIFO enqueue. The
    /// native field is readOnly under the original short lease, never rearmed.
    pub fn current(&self, s: &ShellState) -> bool {
        self.lease.current() && self.capture.current(s)
    }
}
