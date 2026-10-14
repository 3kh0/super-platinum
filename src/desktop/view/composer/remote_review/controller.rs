//! Mounted-editor authority for explicit read/review/choice. No enrollment,
//! automatic request, generic local settlement or direct DOM replacement.
use super::choices;
use crate::{
    drafts::{Fault, checkpoint::{Editor, RemoteLease}, network::{Binding, Issue, RemoteProof}},
    state::{ComposerTarget, ShellState},
    view::composer::{field, field_scope, projection::{self, ProjectionDeferral, ProjectionResult}},
};
use dioxus::prelude::*;
use std::{rc::Rc, sync::Arc};
use super_platinum_core::drafts::{DraftKey, LocalDraft, store::sync::RemoteChoice};

#[derive(Clone, Copy)]
pub(super) enum Failure {
    Fault(Fault),
    Read(Option<Issue>),
    Cooldown,
}
impl From<Fault> for Failure {
    fn from(value: Fault) -> Self { Self::Fault(value) }
}
#[derive(Clone, Copy)]
pub(super) enum Completion {
    Applied,
    NewerInputPreserved,
    InputNotSaved,
    Deferred(ProjectionDeferral),
    ReviewAgain,
}

#[derive(Clone)]
struct Pair {
    live: LocalDraft,
    book: Option<LocalDraft>,
}
impl Pair {
    fn capture(shell: &ShellState, target: ComposerTarget, key: &DraftKey) -> Result<Self, Fault> {
        let live = match target {
            ComposerTarget::Channel => LocalDraft { composer: shell.core.composer.clone(), attachments: shell.core.composer_attachments.clone() },
            ComposerTarget::Thread => LocalDraft { composer: shell.core.thread_composer.clone(), attachments: shell.core.thread_composer_attachments.clone() },
        };
        let book = shell.core.drafts.items.get(key).cloned();
        if !live.attachments.is_empty() || book.as_ref().is_some_and(|book| !plain_same(book, &live))
            || (book.is_none() && !live.empty())
        {
            return Err(Fault::Unsupported);
        }
        Ok(Self { live, book })
    }
    fn same(&self, other: &Self) -> bool {
        plain_same(&self.live, &other.live) && match (&self.book, &other.book) {
            (Some(a), Some(b)) => plain_same(a, b),
            (None, None) => true,
            _ => false,
        }
    }
}
fn plain_same(a: &LocalDraft, b: &LocalDraft) -> bool {
    a.attachments.is_empty() && b.attachments.is_empty() && a.composer == b.composer
}

#[derive(Clone)]
struct Visit {
    account: String,
    epoch: u64,
    target: ComposerTarget,
    key: DraftKey,
    field_scope: String,
    mount: Rc<field::MountLease>,
    editor: Arc<Editor>,
    serial: u64,
    pair: Pair,
}
impl Visit {
    fn destination_current(&self, shell: &ShellState) -> bool {
        self.mount.current()
            && shell.core.active_account.as_deref() == Some(&self.account)
            && shell.core.account_epoch == self.epoch
            && shell.composer_key(self.target).as_ref() == Some(&self.key)
            && field_scope(shell, self.target) == self.field_scope
            && match self.target {
                ComposerTarget::Channel => shell.core.drafts.channel.as_ref() == Some(&self.key),
                ComposerTarget::Thread => shell.core.drafts.thread.as_ref() == Some(&self.key),
            }
            && shell.draft_writes.writers.get(&self.account, self.epoch, &self.key)
                .is_some_and(|editor| Arc::ptr_eq(&editor, &self.editor))
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.destination_current(shell)
            && self.editor.snapshot().serial == self.serial
            && Pair::capture(shell, self.target, &self.key).is_ok_and(|pair| self.pair.same(&pair))
    }
}

/// Contains authenticated credentials through its opaque proof; never Debug/log.
#[derive(Clone)]
pub(super) struct Prepared {
    visit: Visit,
    lease: RemoteLease,
    proof: RemoteProof,
    pub choices: Vec<choices::Choice>,
}
impl Prepared {
    pub fn current(&self, shell: &ShellState) -> bool {
        self.visit.current(shell) && self.proof.current(shell)
    }
}

pub(super) fn available(shell: &ShellState, target: ComposerTarget) -> bool {
    let Some(account) = shell.core.active_account.as_deref() else { return false; };
    let Some(key) = shell.composer_key(target) else { return false; };
    shell.draft_writes.writers.get(account, shell.core.account_epoch, &key)
        .is_some_and(|editor| editor.remote_review_available())
}

pub(super) async fn open(state: Signal<ShellState>, target: ComposerTarget) -> Result<Prepared, Failure> {
    let (visit, binding) = {
        let shell = state.try_peek().map_err(|_| Fault::Stale)?;
        let account = shell.core.active_account.clone().ok_or(Fault::Unmanaged)?;
        let key = shell.composer_key(target).ok_or(Fault::Stale)?;
        let editor = shell.draft_writes.writers.get(&account, shell.core.account_epoch, &key).ok_or(Fault::Unmanaged)?;
        let snapshot = editor.snapshot();
        if snapshot.running { return Err(Fault::Pending.into()); }
        let pair = Pair::capture(&shell, target, &key)?;
        let mount = field::current(&shell, target).ok_or(Fault::Stale)?;
        let binding = Binding::capture(&shell, &key.team).map_err(|_| Failure::Read(None))?;
        let visit = Visit { account, epoch: shell.core.account_epoch, target, key,
            field_scope: field_scope(&shell, target), mount, editor, serial: snapshot.serial, pair };
        if !visit.current(&shell) || !binding.current(&shell) { return Err(Fault::Stale.into()); }
        (visit, binding)
    };
    let lease = visit.editor.prepare_remote_review(visit.pair.live.clone()).await?;
    if lease.serial() != visit.serial
        || !plain_same(lease.before_local(), &visit.pair.live)
        || !lease.inventory().contains_local(&visit.pair.live)
        || visit.pair.book.as_ref().is_some_and(|book| !lease.inventory().contains_local(book))
        || !state.try_peek().is_ok_and(|shell| visit.current(&shell) && binding.current(&shell))
    { return Err(Fault::Stale.into()); }
    // Exactly one explicit read. No read-after-write, timer or retry loop.
    let read = binding.read_remote_review(lease.inventory().clone(), now_ms()).await;
    if !state.try_peek().is_ok_and(|shell| visit.current(&shell) && binding.current(&shell)) {
        return Err(Fault::Stale.into());
    }
    let Some(proof) = read.proof else {
        return Err(if read.cooldown_until_ms.is_some() { Failure::Cooldown } else { Failure::Read(read.issue) });
    };
    let choices = choices::build(proof.review());
    Ok(Prepared { visit, lease, proof, choices })
}

pub(super) async fn apply(
    mut state: Signal<ShellState>,
    prepared: Prepared,
    choice: RemoteChoice,
) -> Result<Completion, Failure> {
    let review = {
        let shell = state.try_peek().map_err(|_| Fault::Stale)?;
        if !prepared.current(&shell) || !choices::allowed(prepared.proof.review(), &choice) {
            return Err(Fault::Stale.into());
        }
        prepared.proof.into_current_review(&shell).map_err(|_| Fault::Stale)?
    };
    let visit = prepared.visit;
    let barrier = visit.editor.reconcile_remote(prepared.lease, review, choice).await?;
    // A durable choice is not a DOM receipt. Do not refresh the DraftBook or
    // replace text here: the immutable barrier parks input until strict ack.
    if !state.try_peek().is_ok_and(|shell| visit.current(&shell)) {
        return Ok(Completion::Deferred(ProjectionDeferral::Stale));
    }
    let ticket = match visit.editor.prepare_projection(&barrier.id).await {
        Ok(ticket) => ticket,
        Err(_) => return Ok(Completion::ReviewAgain),
    };
    let attempt = match projection::capture_projection(state, visit.target, visit.editor.clone(), ticket).await {
        ProjectionResult::Ready(attempt) => attempt,
        ProjectionResult::Changed(changed) => return Ok(preserve(changed, &visit.editor).await),
        ProjectionResult::Deferred(reason) => return Ok(Completion::Deferred(reason)),
    };
    let applied = match attempt.apply().await {
        ProjectionResult::Ready(applied) => applied,
        ProjectionResult::Changed(changed) => return Ok(preserve(changed, &visit.editor).await),
        ProjectionResult::Deferred(reason) => return Ok(Completion::Deferred(reason)),
    };
    let receipt = match applied.install() {
        Ok(receipt) => receipt,
        Err(_) => return Ok(Completion::ReviewAgain),
    };
    if visit.editor.acknowledge_projection(receipt).await.is_err() {
        return Ok(Completion::ReviewAgain);
    }
    if let Ok(mut shell) = state.try_write() && visit.destination_current(&shell) {
        shell.show_toast("The selected draft version is ready. Nothing was sent.");
    }
    Ok(Completion::Applied)
}

async fn preserve(changed: projection::ProjectionChanged, editor: &Arc<Editor>) -> Completion {
    match changed.preserve(editor).await {
        Ok(_) => Completion::NewerInputPreserved,
        Err(_) => Completion::InputNotSaved,
    }
}
fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_millis().try_into().unwrap_or(u64::MAX)).unwrap_or(0)
}

#[cfg(test)]
mod tests;
