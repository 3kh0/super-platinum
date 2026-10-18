//! Explicit native projection proof. No timer, remote observation or autosave wiring.
use super::{byte_to_utf16, field, field_scope, protected_enrollment::Policy, utf16_to_byte};
use crate::{
    drafts::protected::{
        editor::Editor,
        projection::{PreservedAlternate, ProjectionTicket},
        registry::Registry,
    },
    state::{ComposerTarget, ShellState},
};
use dioxus::prelude::*;
use std::{rc::Rc, sync::Arc};
use super_platinum_core::drafts::{LocalDraft, store::protected::Error as Fault};

const MAX_UNITS: usize = 131_072;
const MAX_COUNTER: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Direction {
    None,
    Forward,
    Backward,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativeSnapshot {
    pub(super) id: String,
    pub(super) scope: String,
    pub(super) visit: String,
    pub(super) input: u64,
    pub(super) composition: u64,
    pub(super) composing: bool,
    pub(super) wire: String,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) direction: Direction,
    pub(super) display_start: usize,
    pub(super) display_end: usize,
    pub(super) display_len: usize,
}

impl NativeSnapshot {
    pub(super) fn valid(&self) -> bool {
        self.input <= MAX_COUNTER
            && self.composition <= MAX_COUNTER
            && valid_selection(&self.wire, self.start, self.end)
            && self.display_start <= self.display_end
            && self.display_end <= self.display_len
            && self.display_len <= MAX_UNITS * 4
    }
    fn belongs(&self, attempt: &ProjectionAttempt) -> bool {
        self.valid()
            && self.id == field_id(attempt.target)
            && self.scope == attempt.lease.scope
            && self.visit == attempt.lease.token
    }
}

fn valid_selection(text: &str, start: usize, end: usize) -> bool {
    let length = text.encode_utf16().count();
    length <= MAX_UNITS
        && start <= end
        && end <= length
        && byte_to_utf16(text, utf16_to_byte(text, start)) == start
        && byte_to_utf16(text, utf16_to_byte(text, end)) == end
}

#[derive(serde::Serialize)]
struct Replacement<'a> {
    wire: &'a str,
    start: usize,
    end: usize,
    direction: Direction,
}
fn replacement(local: &LocalDraft) -> Option<Replacement<'_>> {
    let composer = &local.composer;
    let selection = composer.selection;
    if !local.attachments.is_empty()
        || !composer.text.is_char_boundary(selection.start)
        || !composer.text.is_char_boundary(selection.end)
    {
        return None;
    }
    let start = byte_to_utf16(&composer.text, selection.start);
    let end = byte_to_utf16(&composer.text, selection.end);
    valid_selection(&composer.text, start, end).then_some(Replacement {
        wire: &composer.text,
        start,
        end,
        // Core stores offsets, not direction. Request an explicit portable direction:
        // Windows normalizes "none" to "forward", which would fail exact readback.
        // https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#dom-textarea/input-selectiondirection
        direction: Direction::Forward,
    })
}

#[derive(serde::Deserialize)]
#[serde(tag = "status", rename_all = "lowercase", deny_unknown_fields)]
pub(super) enum NativeReceipt {
    Captured {
        token: String,
        current: NativeSnapshot,
    },
    Applied {
        token: String,
        before: NativeSnapshot,
        after: NativeSnapshot,
    },
    Changed {
        current: NativeSnapshot,
    },
    // Empty struct variants enforce deny_unknown_fields; Serde's internally
    // tagged unit variants otherwise accept and discard additional fields.
    Composing {},
    Selectionchanged {},
    Unavailable {},
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectionDeferral {
    Stale,
    Composing,
    SelectionChanged,
    Unavailable,
}

pub(crate) enum ProjectionResult<T> {
    Ready(T),
    Changed(ProjectionChanged),
    Deferred(ProjectionDeferral),
}

/// No public fields/constructors: a capture cannot stand in for native acceptance.
pub(crate) struct ProjectionAttempt {
    policy: Policy,
    state: Signal<ShellState>,
    target: ComposerTarget,
    editor: Arc<Editor>,
    ticket: ProjectionTicket,
    registry: Arc<Registry>,
    epoch: u64,
    lease: Rc<field::MountLease>,
    live: LocalDraft,
    book: Option<LocalDraft>,
    native: Option<NativeSnapshot>,
    token: String,
}

pub(crate) struct AppliedProjection {
    attempt: ProjectionAttempt,
}

/// Send-safe proof produced only after native acceptance AND guarded paired install.
/// Keeping signals and native lease objects out allows the Editor to retain its ack.
pub(crate) struct RendererApplied {
    ticket: ProjectionTicket,
}
impl RendererApplied {
    pub(crate) fn into_editor_ticket(self) -> ProjectionTicket {
        self.ticket
    }
}

pub(crate) struct ProjectionChanged {
    ticket: ProjectionTicket,
    local: LocalDraft,
}
impl ProjectionChanged {
    pub(crate) async fn preserve(self, editor: &Arc<Editor>) -> Result<PreservedAlternate, Fault> {
        if !editor
            .snapshot()
            .projection
            .as_ref()
            .is_some_and(|barrier| {
                barrier.after().scope() == self.ticket.scope()
                    && barrier.id() == self.ticket.barrier_id()
            })
        {
            return Err(Fault::Stale);
        }
        editor
            .preserve_projection_input(
                self.ticket.barrier_id().to_owned(),
                self.ticket.serial(),
                self.local,
            )
            .await
    }
}

fn field_id(target: ComposerTarget) -> &'static str {
    match target {
        ComposerTarget::Channel => "channel-composer",
        ComposerTarget::Thread => "thread-composer",
    }
}

fn live(shell: &ShellState, target: ComposerTarget) -> LocalDraft {
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

fn same_book(policy: Policy, left: Option<&LocalDraft>, right: Option<&LocalDraft>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(a), Some(b)) => policy.same(a, b),
        _ => false,
    }
}

impl ProjectionAttempt {
    fn same_destination(&self, shell: &ShellState) -> bool {
        self.ticket.authority_current(shell)
            && self.registry.current(
                &self.ticket.scope().account_id,
                self.epoch,
                &self.ticket.scope().key,
                &self.editor,
            )
            && shell.core.account_epoch == self.epoch
            && self.lease.current()
            && field_scope(shell, self.target) == self.lease.scope
            && shell.core.active_account.as_deref() == Some(self.ticket.scope().account_id.as_str())
            && shell.composer_key(self.target).as_ref() == Some(&self.ticket.scope().key)
            && match self.target {
                ComposerTarget::Channel => {
                    shell.core.drafts.channel.as_ref() == Some(&self.ticket.scope().key)
                }
                ComposerTarget::Thread => {
                    shell.core.drafts.thread.as_ref() == Some(&self.ticket.scope().key)
                }
            }
    }
    fn same_pair(&self, shell: &ShellState) -> bool {
        self.same_destination(shell)
            && self.policy.same(&live(shell, self.target), &self.live)
            && same_book(
                self.policy,
                shell.core.drafts.items.get(&self.ticket.scope().key),
                self.book.as_ref(),
            )
    }
    fn current(&self) -> bool {
        let snapshot = self.editor.snapshot();
        snapshot.serial == self.ticket.serial()
            && snapshot.projection.as_ref().is_some_and(|barrier| {
                barrier.id() == self.ticket.barrier_id()
                    && barrier.after().scope() == self.ticket.scope()
            })
            && self.same_pair(&self.state.peek())
    }
    fn changed(self, current: NativeSnapshot) -> ProjectionResult<AppliedProjection> {
        match self.into_changed(current) {
            Some(changed) => ProjectionResult::Changed(changed),
            None => ProjectionResult::Deferred(ProjectionDeferral::Stale),
        }
    }
    fn into_changed(self, current: NativeSnapshot) -> Option<ProjectionChanged> {
        if !current.belongs(&self) || current.composing || !self.current() {
            return None;
        }
        let mut local = self.live;
        let start = utf16_to_byte(&current.wire, current.start);
        let end = utf16_to_byte(&current.wire, current.end);
        local.composer.text = current.wire;
        local.composer.set_selection(start, end);
        Some(ProjectionChanged {
            ticket: self.ticket,
            local,
        })
    }

    /// Deliberately separate from install/ack so an input or remount between
    /// these phases is rejected by the later guarded installation.
    pub(crate) async fn apply(self) -> ProjectionResult<AppliedProjection> {
        if !self.current() {
            release(&self.token);
            return ProjectionResult::Deferred(ProjectionDeferral::Stale);
        }
        let Some(replacement) = replacement(self.ticket.after_local()) else {
            release(&self.token);
            return ProjectionResult::Deferred(ProjectionDeferral::Unavailable);
        };
        // Resolve only references in the proposed text from the same captured
        // workspace. Never serialize a directory or initiate a lookup here.
        let labels = {
            let shell = self.state.peek();
            if !self.same_destination(&shell) {
                release(&self.token);
                return ProjectionResult::Deferred(ProjectionDeferral::Stale);
            }
            shell
                .core
                .workspaces
                .get(&self.ticket.scope().key.team)
                .map(|workspace| super::super::completion::labels(workspace, replacement.wire))
                .unwrap_or_default()
        };
        let args = serde_json::to_string(&(&self.token, &replacement, &labels)).unwrap();
        let result = evaluate(&format!("window.superPlatinumProjection?.apply(...{args})")).await;
        match result {
            NativeReceipt::Applied {
                token,
                before,
                after,
            } => {
                let expected = self.native.as_ref();
                let exact = token == self.token
                    && expected == Some(&before)
                    && before.belongs(&self)
                    && after.belongs(&self)
                    && !before.composing
                    && !after.composing
                    && before.input == after.input
                    && before.composition == after.composition
                    && after.wire == replacement.wire
                    && after.start == replacement.start
                    && after.end == replacement.end
                    && after.direction == replacement.direction;
                if exact {
                    ProjectionResult::Ready(AppliedProjection { attempt: self })
                } else {
                    ProjectionResult::Deferred(ProjectionDeferral::Unavailable)
                }
            }
            NativeReceipt::Changed { current } => self.changed(current),
            NativeReceipt::Composing {} => {
                ProjectionResult::Deferred(ProjectionDeferral::Composing)
            }
            NativeReceipt::Selectionchanged {} => {
                ProjectionResult::Deferred(ProjectionDeferral::SelectionChanged)
            }
            _ => ProjectionResult::Deferred(ProjectionDeferral::Unavailable),
        }
    }
}

impl AppliedProjection {
    pub(crate) fn install(self) -> Result<RendererApplied, Fault> {
        let attempt = self.attempt;
        let ProjectionAttempt {
            mut state,
            policy,
            target,
            editor,
            ticket,
            lease,
            registry,
            epoch,
            live: before,
            book,
            ..
        } = attempt;
        editor
            .clone()
            .with_current_projection(ticket, move |ticket| {
                let mut shell = state.write();
                let key = &ticket.scope().key;
                if !ticket.authority_current(&shell)
                    || !registry.current(
                        &ticket.scope().account_id,
                        epoch,
                        &ticket.scope().key,
                        &editor,
                    )
                    || shell.core.account_epoch != epoch
                    || !lease.current()
                    || field_scope(&shell, target) != lease.scope
                    || shell.core.active_account.as_deref()
                        != Some(ticket.scope().account_id.as_str())
                    || shell.composer_key(target).as_ref() != Some(key)
                    || !policy.same(&live(&shell, target), &before)
                    || !same_book(policy, shell.core.drafts.items.get(key), book.as_ref())
                    || match target {
                        ComposerTarget::Channel => shell.core.drafts.channel.as_ref() != Some(key),
                        ComposerTarget::Thread => shell.core.drafts.thread.as_ref() != Some(key),
                    }
                {
                    return Err(Fault::Stale);
                }
                install_pair(
                    &mut shell,
                    target,
                    key,
                    policy,
                    &before,
                    book.as_ref(),
                    ticket.after_local(),
                )?;
                // No checkpoint/route: this is a paired remote projection, and the
                // Editor barrier must remain until its separate strict ack succeeds.
                Ok(RendererApplied { ticket })
            })
    }
}

pub(crate) async fn capture_projection(
    state: Signal<ShellState>,
    target: ComposerTarget,
    editor: Arc<Editor>,
    ticket: ProjectionTicket,
    registry: Arc<Registry>,
) -> ProjectionResult<ProjectionAttempt> {
    capture_with_policy(state, target, editor, ticket, registry, Policy::Plain).await
}
pub(crate) async fn capture_baseline_projection(
    state: Signal<ShellState>,
    target: ComposerTarget,
    editor: Arc<Editor>,
    ticket: ProjectionTicket,
    registry: Arc<Registry>,
) -> ProjectionResult<ProjectionAttempt> {
    capture_with_policy(state, target, editor, ticket, registry, Policy::Baseline).await
}
async fn capture_with_policy(
    state: Signal<ShellState>,
    target: ComposerTarget,
    editor: Arc<Editor>,
    ticket: ProjectionTicket,
    registry: Arc<Registry>,
    policy: Policy,
) -> ProjectionResult<ProjectionAttempt> {
    let (lease, before, book, epoch) = {
        let shell = state.peek();
        let Some(lease) = field::current(&shell, target) else {
            return ProjectionResult::Deferred(ProjectionDeferral::Unavailable);
        };
        (
            lease,
            live(&shell, target),
            shell.core.drafts.items.get(&ticket.scope().key).cloned(),
            shell.core.account_epoch,
        )
    };
    if !policy.same(&before, ticket.before_local())
        || replacement(ticket.after_local()).is_none()
        || book
            .as_ref()
            .is_some_and(|book| !policy.same(book, &before))
        || (book.is_none() && !before.empty())
    {
        return ProjectionResult::Deferred(ProjectionDeferral::Stale);
    }
    let mut attempt = ProjectionAttempt {
        policy,
        state,
        target,
        editor,
        ticket,
        registry,
        epoch,
        lease,
        live: before,
        book,
        native: None,
        token: String::new(),
    };
    if !attempt.current() {
        return ProjectionResult::Deferred(ProjectionDeferral::Stale);
    }
    let args = serde_json::to_string(&(
        field_id(target),
        &attempt.lease.scope,
        &attempt.lease.token,
        &attempt.live.composer.text,
    ))
    .unwrap();
    match evaluate(&format!(
        "window.superPlatinumProjection?.capture(...{args})"
    ))
    .await
    {
        NativeReceipt::Captured { token, current } => {
            if !valid_token(&token)
                || !current.belongs(&attempt)
                || current.composing
                || current.wire != attempt.live.composer.text
                || !attempt.current()
            {
                release(&token);
                return ProjectionResult::Deferred(ProjectionDeferral::Stale);
            }
            attempt.native = Some(current);
            attempt.token = token;
            ProjectionResult::Ready(attempt)
        }
        NativeReceipt::Changed { current } => match attempt.into_changed(current) {
            Some(changed) => ProjectionResult::Changed(changed),
            None => ProjectionResult::Deferred(ProjectionDeferral::Stale),
        },
        NativeReceipt::Composing {} => ProjectionResult::Deferred(ProjectionDeferral::Composing),
        _ => ProjectionResult::Deferred(ProjectionDeferral::Unavailable),
    }
}

pub(super) fn valid_token(token: &str) -> bool {
    token.len() <= 16
        && token
            .parse::<u64>()
            .is_ok_and(|value| value > 0 && value <= MAX_COUNTER)
}

pub(super) fn mount_native(id: &str, scope: &str, visit: &str) {
    let args = serde_json::to_string(&(id, scope, visit)).unwrap();
    document::eval(&format!(
        "{}; window.superPlatinumProjection.mount(...{args});",
        include_str!("projection.js")
    ));
}
fn release(token: &str) {
    if !valid_token(token) {
        return;
    }
    let token = serde_json::to_string(token).unwrap();
    document::eval(&format!(
        "window.superPlatinumProjection?.release({token});"
    ));
}
pub(super) async fn evaluate(expression: &str) -> NativeReceipt {
    let mut eval = document::eval(&format!(
        "try{{dioxus.send(({expression})||{{status:'unavailable'}});}}catch{{dioxus.send({{status:'unavailable'}});}} await dioxus.recv();"
    ));
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        eval.recv::<NativeReceipt>(),
    )
    .await;
    let _ = eval.send(());
    match result {
        Ok(Ok(receipt)) => receipt,
        _ => NativeReceipt::Unavailable {},
    }
}

// Shared native protocol tests remain in projection_tests.rs; protected proof
// and registry interleavings receive separate opaque-worker tests.

// Called only inside with_current_projection after native Applied and all
// account/mount/serial guards. This helper owns the atomic paired state mutation.
fn install_pair(
    shell: &mut ShellState,
    target: ComposerTarget,
    key: &super_platinum_core::drafts::DraftKey,
    policy: Policy,
    before: &LocalDraft,
    book: Option<&LocalDraft>,
    after: &LocalDraft,
) -> Result<(), Fault> {
    if !after.attachments.is_empty() {
        return Err(Fault::Unsupported);
    }
    if !policy.same(&live(shell, target), before)
        || !same_book(policy, shell.core.drafts.items.get(key), book)
    {
        return Err(Fault::Stale);
    }
    shell
        .core
        .drafts
        .project_clean(key, book, after)
        .map_err(|_| Fault::Stale)?;
    match target {
        ComposerTarget::Channel => {
            shell.core.composer = after.composer.clone();
            shell.core.composer_attachments = after.attachments.clone();
        }
        ComposerTarget::Thread => {
            shell.core.thread_composer = after.composer.clone();
            shell.core.thread_composer_attachments = after.attachments.clone();
        }
    }
    Ok(())
}
#[cfg(test)]
#[path = "protected_projection/baseline_tests.rs"]
mod baseline_tests;
