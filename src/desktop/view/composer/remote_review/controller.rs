//! Mounted-editor authority for explicit read/review/choice. No enrollment,
//! automatic request, generic local settlement or direct DOM replacement.
use super::{choices, read};
use crate::{
    drafts::{
        Fault,
        checkpoint::{Editor, PreservedAlternate, RemoteLease},
        network::Issue,
    },
    state::{ComposerTarget, ShellState},
    view::composer::{
        field, field_scope,
        projection::{self, ProjectionDeferral, ProjectionResult},
    },
};
use dioxus::prelude::*;
use std::{rc::Rc, sync::Arc};
use super_platinum_core::drafts::{
    DraftKey, LocalDraft,
    store::sync::{RemoteChoice, RemoteReview},
};

#[derive(Clone, Copy)]
pub(super) enum Failure {
    Fault(Fault),
    Read(Option<Issue>),
    Cooldown,
}
impl From<Fault> for Failure {
    fn from(value: Fault) -> Self {
        Self::Fault(value)
    }
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
            ComposerTarget::Channel => LocalDraft {
                composer: shell.core.composer.clone(),
                attachments: shell.core.composer_attachments.clone(),
            },
            ComposerTarget::Thread => LocalDraft {
                composer: shell.core.thread_composer.clone(),
                attachments: shell.core.thread_composer_attachments.clone(),
            },
        };
        let book = shell.core.drafts.items.get(key).cloned();
        if !live.attachments.is_empty()
            || book.as_ref().is_some_and(|book| !plain_same(book, &live))
            || (book.is_none() && !live.empty())
        {
            return Err(Fault::Unsupported);
        }
        Ok(Self { live, book })
    }
    fn same(&self, other: &Self) -> bool {
        plain_same(&self.live, &other.live)
            && match (&self.book, &other.book) {
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
            && shell
                .draft_writes
                .writers
                .get(&self.account, self.epoch, &self.key)
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
    proof: read::Proof,
    source: read::Source,
    pub choices: Vec<choices::Choice>,
}
impl Prepared {
    pub fn destination_current(&self, shell: &ShellState) -> bool {
        self.visit.destination_current(shell)
    }
    pub fn team(&self) -> &str {
        &self.visit.key.team
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        self.visit.destination_current(shell)
            && self.visit.editor.remote_review_current(&self.lease)
            && Pair::capture(shell, self.visit.target, &self.visit.key)
                .is_ok_and(|pair| self.visit.pair.same(&pair))
            && self.proof.current(shell)
            && self.source.current(shell)
    }
}

pub(super) fn available(shell: &ShellState, target: ComposerTarget) -> bool {
    let Some(account) = shell.core.active_account.as_deref() else {
        return false;
    };
    let Some(key) = shell.composer_key(target) else {
        return false;
    };
    shell
        .draft_writes
        .writers
        .get(account, shell.core.account_epoch, &key)
        .is_some_and(|editor| editor.remote_review_available())
}

fn capture(
    state: Signal<ShellState>,
    target: ComposerTarget,
) -> Result<(Visit, read::Source), Failure> {
    let (visit, binding) = {
        let shell = state.try_peek().map_err(|_| Fault::Stale)?;
        let account = shell.core.active_account.clone().ok_or(Fault::Unmanaged)?;
        let key = shell.composer_key(target).ok_or(Fault::Stale)?;
        let editor = shell
            .draft_writes
            .writers
            .get(&account, shell.core.account_epoch, &key)
            .ok_or(Fault::Unmanaged)?;
        let snapshot = editor.snapshot();
        if snapshot.running {
            return Err(Fault::Pending.into());
        }
        let pair = Pair::capture(&shell, target, &key)?;
        let mount = field::current(&shell, target).ok_or(Fault::Stale)?;
        let binding = read::Source::capture(&shell, &key)?;
        let visit = Visit {
            account,
            epoch: shell.core.account_epoch,
            target,
            key,
            field_scope: field_scope(&shell, target),
            mount,
            editor,
            serial: snapshot.serial,
            pair,
        };
        if !visit.current(&shell) || !binding.current(&shell) {
            return Err(Fault::Stale.into());
        }
        (visit, binding)
    };
    Ok((visit, binding))
}

pub(super) async fn open(
    state: Signal<ShellState>,
    target: ComposerTarget,
) -> Result<Prepared, Failure> {
    let (mut visit, mut binding) = capture(state, target)?;
    let (lease, preserved) = prepare_for_review(
        &visit.editor,
        visit.serial,
        &visit.account,
        &visit.key,
        &visit.pair.live,
        || {
            state
                .try_peek()
                .is_ok_and(|shell| visit.current(&shell) && binding.current(&shell))
        },
    )
    .await?;
    if let Some(proof) = preserved {
        // This explicit review preserved the known previous version. It did not
        // sample native text or type an edit. Re-capture only our new serial;
        // changing mount, source or complete foreground still refuses the read.
        let (fresh, source) = capture(state, target)?;
        if !state
            .try_peek()
            .is_ok_and(|shell| visit.destination_current(&shell) && binding.current(&shell))
            || !Rc::ptr_eq(&visit.mount, &fresh.mount)
            || !visit.pair.same(&fresh.pair)
            || fresh.serial != proof.serial()
            || proof.scope().account_id != fresh.account
            || proof.scope().key != fresh.key
            || !plain_same(proof.local(), &fresh.pair.live)
        {
            return Err(Fault::Stale.into());
        }
        visit = fresh;
        binding = source;
    }
    if lease.serial() != visit.serial
        || !plain_same(lease.before_local(), &visit.pair.live)
        || !lease.inventory().contains_local(&visit.pair.live)
        || visit
            .pair
            .book
            .as_ref()
            .is_some_and(|book| !lease.inventory().contains_local(book))
        || !state
            .try_peek()
            .is_ok_and(|shell| visit.current(&shell) && binding.current(&shell))
    {
        return Err(Fault::Stale.into());
    }
    // Exactly one explicit read. No read-after-write, timer or retry loop.
    let proof = binding.read(lease.inventory().clone(), now_ms()).await;
    if !state
        .try_peek()
        .is_ok_and(|shell| visit.current(&shell) && binding.current(&shell))
    {
        return Err(Fault::Stale.into());
    }
    let proof = proof?;
    let choices = choices::build(proof.review());
    Ok(Prepared {
        visit,
        lease,
        proof,
        source: binding,
        choices,
    })
}

/// Explicit Review may preserve a previously chosen handoff's exact visible
/// predecessor when it is no longer in the inventory. The subsequent review is
/// entirely fresh and includes every writer; this never retries a Slack read.
async fn prepare_for_review(
    editor: &Arc<Editor>,
    serial: u64,
    account: &str,
    key: &DraftKey,
    live: &LocalDraft,
    is_current: impl Fn() -> bool,
) -> Result<(RemoteLease, Option<PreservedAlternate>), Fault> {
    match editor.prepare_remote_review(live.clone()).await {
        Ok(lease) => Ok((lease, None)),
        Err(Fault::AlternateEdits) => {
            if !is_current() {
                return Err(Fault::Stale);
            }
            let barrier = finish_barrier(editor, serial, account, key, live)?;
            let proof = editor
                .preserve_remote_predecessor(barrier.id.clone(), serial, live.clone())
                .await?;
            let lease = editor.prepare_remote_review(live.clone()).await?;
            if lease.serial() != proof.serial() || !plain_same(lease.before_local(), proof.local())
            {
                return Err(Fault::Stale);
            }
            Ok((lease, Some(proof)))
        }
        Err(error) => Err(error),
    }
}

pub(super) async fn apply(
    state: Signal<ShellState>,
    prepared: Prepared,
    choice: RemoteChoice,
) -> Result<Completion, Failure> {
    let review = {
        let shell = state.try_peek().map_err(|_| Fault::Stale)?;
        if !prepared.current(&shell) || !choices::allowed(prepared.proof.review(), &choice) {
            return Err(Fault::Stale.into());
        }
        prepared
            .proof
            .into_current_review(&shell)
            .map_err(|_| Fault::Stale)?
    };
    let visit = prepared.visit;
    prepared.source.checkpoint("before-commit").await;
    if !state
        .try_peek()
        .is_ok_and(|shell| visit.current(&shell) && prepared.source.current(&shell))
    {
        return Err(Fault::Stale.into());
    }
    let barrier = commit_choice(&visit.editor, prepared.lease, review, choice).await?;
    prepared.source.checkpoint("after-commit").await;
    handoff(state, visit, prepared.source, &barrier.id).await
}

pub(super) fn can_finish(shell: &ShellState, target: ComposerTarget) -> bool {
    let Some(account) = shell.core.active_account.as_deref() else {
        return false;
    };
    let Some(key) = shell.composer_key(target) else {
        return false;
    };
    shell
        .draft_writes
        .writers
        .get(account, shell.core.account_epoch, &key)
        .is_some_and(|editor| editor.projection_finish_available())
}

/// A fresh mounted visit may finish the SAME durable choice. This performs no
/// remote read, reconciliation or barrier refresh. Exact native predecessor and
/// current full durable inventory are still checked by the projection protocol.
pub(super) async fn finish(
    state: Signal<ShellState>,
    target: ComposerTarget,
) -> Result<Completion, Failure> {
    let (visit, source) = capture(state, target)?;
    let barrier = finish_barrier(
        &visit.editor,
        visit.serial,
        &visit.account,
        &visit.key,
        &visit.pair.live,
    )?;
    handoff(state, visit, source, &barrier.id).await
}

fn finish_barrier(
    editor: &Arc<Editor>,
    serial: u64,
    account: &str,
    key: &DraftKey,
    live: &LocalDraft,
) -> Result<Arc<crate::drafts::checkpoint::ProjectionBarrier>, Fault> {
    let snapshot = editor.snapshot();
    let barrier = snapshot.projection.ok_or(Fault::Stale)?;
    if barrier.after.is_none()
        || snapshot.running
        || snapshot.serial != serial
        || barrier.serial != serial
        || barrier.scope.account_id != account
        || &barrier.scope.key != key
        || !plain_same(&barrier.before.local, live)
    {
        return Err(Fault::Stale);
    }
    Ok(barrier)
}

async fn handoff(
    mut state: Signal<ShellState>,
    visit: Visit,
    source: read::Source,
    barrier_id: &str,
) -> Result<Completion, Failure> {
    // A durable choice is not a DOM receipt. Do not refresh the DraftBook or
    // replace text here: the immutable barrier parks input until strict ack.
    if !state
        .try_peek()
        .is_ok_and(|shell| visit.current(&shell) && source.current(&shell))
    {
        return Ok(Completion::Deferred(ProjectionDeferral::Stale));
    }
    let ticket = match visit.editor.prepare_projection(barrier_id).await {
        Ok(ticket) => ticket,
        Err(_) => return Ok(Completion::ReviewAgain),
    };
    if !state
        .try_peek()
        .is_ok_and(|shell| visit.current(&shell) && source.current(&shell))
    {
        return Ok(Completion::Deferred(ProjectionDeferral::Stale));
    }
    let attempt =
        match projection::capture_projection(state, visit.target, visit.editor.clone(), ticket)
            .await
        {
            ProjectionResult::Ready(attempt) => attempt,
            ProjectionResult::Changed(changed) => return Ok(preserve(changed, &visit.editor).await),
            ProjectionResult::Deferred(reason) => return Ok(Completion::Deferred(reason)),
        };
    if !state
        .try_peek()
        .is_ok_and(|shell| visit.current(&shell) && source.current(&shell))
    {
        return Ok(Completion::Deferred(ProjectionDeferral::Stale));
    }
    let applied = match attempt.apply().await {
        ProjectionResult::Ready(applied) => applied,
        ProjectionResult::Changed(changed) => return Ok(preserve(changed, &visit.editor).await),
        ProjectionResult::Deferred(reason) => return Ok(Completion::Deferred(reason)),
    };
    if !state
        .try_peek()
        .is_ok_and(|shell| visit.current(&shell) && source.current(&shell))
    {
        return Ok(Completion::Deferred(ProjectionDeferral::Stale));
    }
    let receipt = match applied.install() {
        Ok(receipt) => receipt,
        Err(_) => return Ok(Completion::ReviewAgain),
    };
    if visit.editor.acknowledge_projection(receipt).await.is_err() {
        return Ok(Completion::ReviewAgain);
    }
    if let Ok(mut shell) = state.try_write()
        && visit.destination_current(&shell)
        && source.current(&shell)
    {
        shell.show_toast("The selected draft version is ready. Nothing was sent.");
    }
    Ok(Completion::Applied)
}

// This is the only durable choice path used by the controller. Keep UI choice
// admission and the Editor's exact-inventory CAS together; display copies never
// become authority and a committed choice still requires native projection.
async fn commit_choice(
    editor: &Arc<Editor>,
    lease: RemoteLease,
    review: RemoteReview,
    choice: RemoteChoice,
) -> Result<Arc<crate::drafts::checkpoint::ProjectionBarrier>, Fault> {
    if !choices::allowed(&review, &choice) {
        return Err(Fault::Unsupported);
    }
    editor.reconcile_remote(lease, review, choice).await
}

async fn preserve(changed: projection::ProjectionChanged, editor: &Arc<Editor>) -> Completion {
    match changed.preserve(editor).await {
        Ok(_) => Completion::NewerInputPreserved,
        Err(_) => Completion::InputNotSaved,
    }
}
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_millis().try_into().unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
