//! Existing unmanaged scheduling remains separate. No enrollment or autosave.
use super::{
    Account, Fault, View,
    checkpoint::{Editor, ScheduleLease, ScheduleVisit},
    network::Binding,
};
use crate::state::{ComposerTarget, MainView, ShellState};
use dioxus::prelude::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use super_platinum_core::drafts::{
    DraftKey, LocalDraft,
    sync::{Operation, RejectedScheduleChoice, Snapshot, Workflow},
};
#[path = "schedule_dispatch.rs"]
mod dispatch;
pub(crate) use dispatch::{
    advances_queue as protected_advances_queue, scheduled_message as protected_scheduled_message,
};
#[path = "schedule_fixture.rs"]
pub(crate) mod fixture;
#[cfg(test)]
pub(crate) use dispatch::once as test_once;

#[derive(Clone)]
enum Source {
    Network(Binding),
    Fixture(Arc<fixture::Binding>),
}
impl Source {
    fn capture(shell: &ShellState, team: &str) -> Option<Self> {
        if let Some(binding) = fixture::binding(shell) {
            return Some(Self::Fixture(binding));
        }
        if crate::fixture::is_fixture() {
            return None;
        }
        Binding::capture(shell, team).ok().map(Self::Network)
    }
    fn current(&self, shell: &ShellState) -> bool {
        match self {
            Self::Network(binding) => binding.current(shell),
            Self::Fixture(binding) => binding.current(shell),
        }
    }
    fn account(&self, account: &str) -> Result<Arc<Account>, Fault> {
        match self {
            Self::Network(_) => Account::production(account),
            Self::Fixture(binding) => {
                let owner = binding.account();
                if owner.backend.account() == account {
                    Ok(owner)
                } else {
                    Err(Fault::Stale)
                }
            }
        }
    }
    async fn execute(&self, operation: &Operation, now_ms: u64) -> super::network::Attempt {
        match self {
            Self::Network(binding) => binding.execute(operation, now_ms).await,
            Self::Fixture(binding) => binding.execute(operation, now_ms).await,
        }
    }
}

#[derive(Clone)]
enum Mode {
    Create {
        target: ComposerTarget,
        local: LocalDraft,
        field: String,
        thread: Option<u64>,
    },
    Rejected(Operation),
}
#[derive(Clone)]
pub(crate) struct Request {
    account: String,
    epoch: u64,
    shell_visit: u64,
    surface: MainView,
    key: DraftKey,
    binding: Source,
    writer: Option<Arc<Editor>>,
    before: View,
    mode: Mode,
}
impl PartialEq for Request {
    fn eq(&self, other: &Self) -> bool {
        self.account == other.account
            && self.epoch == other.epoch
            && self.shell_visit == other.shell_visit
            && self.key == other.key
            && super::dispatch_schedule::same_schedule_view(&self.before, &other.before)
    }
}
fn local(shell: &ShellState, target: ComposerTarget) -> LocalDraft {
    match target {
        ComposerTarget::Channel => LocalDraft {
            composer: shell.core.composer.clone(),
            attachments: shell.core.composer_attachments.clone(),
        },
        ComposerTarget::Thread => LocalDraft {
            composer: shell.core.thread_composer.clone(),
            attachments: shell.core.thread_composer_attachments.clone(),
        },
    }
}
fn creation(view: &View) -> bool {
    view.blocked.is_none()
        && view.uncommitted.is_none()
        && view.local.attachments.is_empty()
        && !view.local.composer.text.trim().is_empty()
        && view.stored.as_ref().is_some_and(|row| {
            row.state.workflow() == &Workflow::Idle
                && row.state.acknowledged().is_some()
                && row.state.desired().is_some()
                && row.state.consumption().is_none()
                && row.state.blocked_observation().is_none()
        })
}
impl Request {
    fn capture(shell: &ShellState, key: DraftKey, mode: Mode) -> Option<Self> {
        let account = shell.core.active_account.clone()?;
        let epoch = shell.core.account_epoch;
        if !shell.managed_drafts.ready_for(&account, epoch)
            || shell.managed_drafts.local_review_pending(&account, &key)
            || shell.draft_writes.sending.contains(&key)
        {
            return None;
        }
        let before = shell.managed_drafts.entry(&account, &key)?.clone();
        let binding = Source::capture(shell, &key.team)?;
        Some(Self {
            writer: shell.draft_writes.writers.get(&account, epoch, &key),
            account,
            epoch,
            shell_visit: shell.draft_writes.view_id,
            surface: shell.main_view,
            key,
            binding,
            before,
            mode,
        })
    }
    pub fn create(shell: &ShellState, target: ComposerTarget) -> Option<Self> {
        let key = shell.composer_key(target)?;
        let live = local(shell, target);
        let request = Self::capture(
            shell,
            key,
            Mode::Create {
                target,
                local: live.clone(),
                field: crate::view::composer::field_scope(shell, target),
                thread: (target == ComposerTarget::Thread).then_some(shell.thread_generation),
            },
        )?;
        (creation(&request.before) && super::same_local(&request.before.local, &live))
            .then_some(request)
    }
    pub fn rejected(shell: &ShellState, key: &DraftKey) -> Option<Self> {
        let account = shell.core.active_account.as_ref()?;
        let op = shell
            .managed_drafts
            .entry(account, key)?
            .stored
            .as_ref()?
            .state
            .rejected_schedule()?
            .1
            .clone();
        Self::capture(shell, key.clone(), Mode::Rejected(op))
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.binding.current(shell)
            && shell.draft_writes.view_id == self.shell_visit
            && shell.main_view == self.surface
            && shell.core.active_team.as_ref() == Some(&self.key.team)
            && shell.managed_drafts.ready_for(&self.account, self.epoch)
            && !shell
                .managed_drafts
                .local_review_pending(&self.account, &self.key)
            && match &self.mode {
                Mode::Create {
                    target,
                    local: before,
                    field,
                    thread,
                } => {
                    shell.composer_key(*target).as_ref() == Some(&self.key)
                        && crate::view::composer::field_scope(shell, *target) == *field
                        && thread.is_none_or(|value| value == shell.thread_generation)
                        && super::same_local(before, &local(shell, *target))
                }
                Mode::Rejected(_) => true,
            }
    }
}

pub(crate) struct Dialog {
    request: Request,
    editor: Arc<Editor>,
    lease: ScheduleLease,
    visit: ScheduleVisit,
    mounted: Arc<AtomicBool>,
    generation: u64,
    snapshot: Snapshot,
    post_at: i64,
    scheduled_generation: u64,
    scheduled_scoped: bool,
}
impl PartialEq for Dialog {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation
    }
}
impl Dialog {
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    pub fn post_at(&self) -> i64 {
        self.post_at
    }
    pub fn key(&self) -> &DraftKey {
        &self.request.key
    }
    pub fn rejected(&self) -> bool {
        matches!(self.request.mode, Mode::Rejected(_))
    }
    pub fn feedback_current(&self, shell: &ShellState) -> bool {
        dispatch::owner_current(self, shell)
    }
    pub fn close(&self) {
        self.mounted.store(false, Ordering::Release);
        self.visit.invalidate();
    }
    // Never lock Editor here: the final admission callback runs under its mutex.
    fn context(&self, shell: &ShellState) -> bool {
        self.mounted.load(Ordering::Acquire)
            && self.request.current(shell)
            && shell
                .draft_writes
                .writers
                .get(&self.request.account, self.request.epoch, &self.request.key)
                .is_some_and(|current| Arc::ptr_eq(&current, &self.editor))
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        self.context(shell)
            && if self.rejected() {
                self.editor
                    .rejected_schedule_review_current(&self.lease, &self.visit)
            } else {
                self.editor
                    .reviewed_schedule_current(&self.lease, &self.visit)
            }
    }
}

/// No native data or local source is reconstructed after opening this review.
pub(crate) async fn open(
    mut state: Signal<ShellState>,
    request: Request,
) -> Result<Arc<Dialog>, Fault> {
    if !state.try_peek().is_ok_and(|shell| request.current(&shell)) {
        return Err(Fault::Stale);
    }
    let account = request.binding.account(&request.account)?;
    let owner = account.owner(request.key.clone());
    let loaded = owner.load().await?;
    let editor = {
        let mut shell = state.try_write().map_err(|_| Fault::Stale)?;
        if !request.current(&shell)
            || !super::session::same_transport(
                &request.writer,
                &shell
                    .draft_writes
                    .writers
                    .get(&request.account, request.epoch, &request.key),
            )
            || !super::dispatch_schedule::same_schedule_view(&loaded, &request.before)
        {
            return Err(Fault::Stale);
        }
        shell
            .draft_writes
            .editor(&account, request.epoch, request.key.clone(), loaded)?
    };
    if matches!(request.mode, Mode::Rejected(_)) {
        // The owner persisted the rejection; refresh only that same Queue writer.
        editor.refresh_receipt().await?;
    }
    let visit = ScheduleVisit::new();
    let lease = match &request.mode {
        Mode::Create { local, .. } => {
            editor
                .prepare_reviewed_schedule(local.clone(), visit.clone())
                .await?
        }
        Mode::Rejected(op) => {
            let lease = editor
                .prepare_rejected_schedule_review(visit.clone())
                .await?;
            if lease
                .before()
                .stored
                .as_ref()
                .and_then(|row| row.state.rejected_schedule())
                .map(|(_, current)| current)
                != Some(op)
            {
                return Err(Fault::Stale);
            }
            lease
        }
    };
    let row = lease.before().stored.as_ref().ok_or(Fault::Stale)?;
    let (snapshot, post_at) = match &request.mode {
        Mode::Create { .. } => (
            row.state.desired().cloned().ok_or(Fault::Unsupported)?,
            super_platinum_core::state::now_secs() + 3600,
        ),
        Mode::Rejected(_) => {
            let (intent, _) = row.state.rejected_schedule().ok_or(Fault::Stale)?;
            let super_platinum_core::drafts::sync::ConsumeKind::Schedule { post_at } = &intent.kind
            else {
                return Err(Fault::Unsupported);
            };
            (intent.snapshot.clone(), *post_at)
        }
    };
    static GENERATIONS: AtomicU64 = AtomicU64::new(1);
    let shell = state.try_peek().map_err(|_| Fault::Stale)?;
    let scheduled_scoped = shell
        .core
        .scheduled
        .matches_scope(request.epoch, &request.key.team);
    let dialog = Arc::new(Dialog {
        request,
        editor,
        lease,
        visit,
        mounted: Arc::new(AtomicBool::new(true)),
        generation: GENERATIONS.fetch_add(1, Ordering::Relaxed),
        snapshot,
        post_at,
        scheduled_generation: shell.core.scheduled.generation,
        scheduled_scoped,
    });
    if !dialog.current(&shell) {
        dialog.close();
        return Err(Fault::Stale);
    }
    Ok(dialog)
}

#[derive(Clone)]
pub(crate) enum Decision {
    Create {
        blocks: Vec<serde_json::Value>,
        post_at: i64,
    },
    Retry,
    Replace {
        blocks: Vec<serde_json::Value>,
        post_at: i64,
    },
}

pub(crate) async fn submit(
    state: Signal<ShellState>,
    dialog: Arc<Dialog>,
    decision: Decision,
) -> Result<&'static str, Fault> {
    let now = super_platinum_core::state::now_secs();
    if !state.try_peek().is_ok_and(|shell| dialog.current(&shell)) {
        return Err(Fault::Stale);
    }
    let guarded = dialog.clone();
    let guard = move || {
        state
            .try_peek()
            .ok()
            .filter(|shell| guarded.context(shell))
            .map(|_| ())
            .ok_or(Fault::Stale)
    };
    let stored = match decision {
        Decision::Create { blocks, post_at } if !dialog.rejected() => {
            super_platinum_core::scheduled::validate_time(post_at, now)
                .map_err(|_| Fault::Unsupported)?;
            let mut raw = dialog.snapshot.clone();
            raw.blocks = blocks;
            // Validate the exact recovery-supported operation BEFORE durable consume.
            dispatch::supported(&dialog.lease, &raw, post_at)?;
            dialog
                .editor
                .consume_reviewed_schedule_guarded(
                    dialog.lease.clone(),
                    dialog.visit.clone(),
                    raw,
                    post_at,
                    guard,
                )
                .await?
        }
        Decision::Retry if dialog.rejected() => {
            dialog
                .editor
                .apply_rejected_schedule_guarded(
                    dialog.lease.clone(),
                    dialog.visit.clone(),
                    RejectedScheduleChoice::Retry,
                    now,
                    guard,
                )
                .await?
        }
        Decision::Replace { blocks, post_at } if dialog.rejected() => {
            let mut snapshot = dialog.snapshot.clone();
            snapshot.blocks = blocks;
            dialog
                .editor
                .apply_rejected_schedule_guarded(
                    dialog.lease.clone(),
                    dialog.visit.clone(),
                    RejectedScheduleChoice::Replace { snapshot, post_at },
                    now,
                    guard,
                )
                .await?
        }
        _ => return Err(Fault::Stale),
    };
    let intent = stored.state.consumption().cloned().ok_or(Fault::Pending)?;
    dispatch::project_consumed(state, &dialog);
    // The human action is now durable. Captured account dispatch survives a UI switch.
    dispatch::run(state, &dialog, &intent).await
}
