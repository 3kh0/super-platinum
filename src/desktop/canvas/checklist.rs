//! Visit-owned canonical reads and retained one-attempt checklist operations.
//! Protocol eligibility, partial building and readback proof remain core-owned.
#![allow(dead_code)] // Network/UI wiring follows verified protocol integration.
#[cfg(test)]
use crate::state::Overlay;
use crate::state::ShellState;
use std::sync::Arc;
use super_platinum_core::{
    canvas::edit::{AcceptedCheckbox, EditSession, PreparedCheckboxEdit},
    slack::api::CanvasCollabSnapshot,
};
pub(crate) mod check;
pub(super) mod event_reads;
pub(super) mod fixture;
pub(super) mod reader;
pub(super) mod recovery;
pub(super) mod replacement_ops;
pub(super) mod save;
pub(in crate::canvas) mod scope;
pub(super) mod structure_ops;
pub(super) mod text_ops;
use scope::Scope;
#[cfg(test)]
mod tests;

const MAX_RECEIPTS: usize = 8;
const MAX_TOTAL_RECEIPTS: usize = 32;
#[derive(Default)]
pub(crate) struct State {
    pub(in crate::canvas) local_editor: Option<u64>,
    pub(in crate::canvas) comment_annotations: super::comments::annotation::Root,
    pub(in crate::canvas) comment_replies: super::comments::reply::Root,
    pub(in crate::canvas) comment_operations: super::comments::mutation::State,
    serial: u64,
    view: Option<Visit>,
    operations: Vec<Operation>,
    recovery: recovery::State,
    sessions: Vec<OwnedSession>,
    fixture: Option<fixture::State>,
    pub(in crate::canvas) structure_fixture: Option<super::text::structure_fixture::State>,
    pub(in crate::canvas) text_fixture: Option<super::text::fixture::State>,
}
struct OwnedSession {
    scope: Scope,
    session: EditSession,
}
struct Visit {
    scope: Scope,
    file: String,
    generation: u64,
    revision: u64,
    event_read_revision: u64,
    event_reads_blocked: bool,
    settled_text: Option<u64>,
    read: u64,
    loading: bool,
    stale: bool,
    snapshot: Option<Arc<CanvasCollabSnapshot>>,
    error: Option<&'static str>,
}
#[derive(Clone)]
pub(super) struct Read {
    scope: Scope,
    pub file: String,
    visit: u64,
    serial: u64,
    revision: u64,
}
#[derive(Clone)]
pub(super) struct Attempt {
    scope: Scope,
    pub serial: u64,
    visit: u64,
    read: u64,
    revision: u64,
    pub snapshot: Arc<CanvasCollabSnapshot>,
    pub section: String,
    mutation: Mutation,
}
#[derive(Clone)]
enum Mutation {
    Replacement { replacement: String },
    Checkbox(bool),
    Structure(Arc<super_platinum_core::canvas::edit::text::TextDraft>),
    Text(Arc<super_platinum_core::canvas::edit::text::TextDraft>),
}
enum Accepted {
    Replacement(super_platinum_core::canvas::edit::structure::AcceptedReplacement),
    Checkbox(AcceptedCheckbox),
    Structure(super_platinum_core::canvas::edit::structure::AcceptedStructure),
    Text(super_platinum_core::canvas::edit::text::AcceptedText),
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Stage {
    Preparing,
    Saving,
    AwaitingRead,
    Uncertain,
}
impl Stage {
    pub(super) fn blocks_refresh(self) -> bool {
        matches!(self, Self::Preparing | Self::Saving)
    }
}
struct Operation {
    attempt: Attempt,
    stage: Stage,
    dispatched_after_read: u64,
    dispatched: Option<(String, u64)>,
    structural_request: Option<super_platinum_core::canvas::wire::RawMessage>,
    accepted: Option<Accepted>,
    durable: Option<recovery::Ticket>,
    durable_verified: Option<u64>,
    durable_finishing: bool,
}
/// Only the canvas integration consumes these outcomes. An accepted HTTP result
/// is not enough to unlock another edit: fresh canonical proof is still required.
pub(super) enum Outcome {
    Rejected(&'static str),
    Accepted(AcceptedCheckbox),
    Uncertain,
}
pub(super) struct Presentation {
    pub visit: u64,
    pub read: u64,
    pub loading: bool,
    pub stale: bool,
    pub snapshot: Option<Arc<CanvasCollabSnapshot>>,
    pub operation: Option<(u64, String, Stage)>,
    pub error: Option<&'static str>,
    pub outstanding_total: usize,
    pub capacity_reason: Option<&'static str>,
}
impl State {
    fn next(&mut self) -> u64 {
        self.serial = self.serial.wrapping_add(1);
        self.serial
    }
}
fn view_current(shell: &ShellState, view: &Visit) -> bool {
    view.scope.current(shell)
        && super::surface::visible(shell, &view.file)
        && shell.shortcut_generation == view.generation
        && shell
            .canvas
            .as_ref()
            .is_some_and(|target| target.id == view.file)
        && !view.scope.deleted(shell, &view.file)
}
fn operation_index(shell: &ShellState, file: &str) -> Option<usize> {
    shell
        .canvas_checklist
        .operations
        .iter()
        .position(|operation| {
            operation.attempt.snapshot.file_id() == file
                && operation.attempt.scope.stable_current(shell)
        })
}
/// Opening a different view cancels only unsent work. Sent/uncertain receipts
/// remain owned by their original account/session and cannot attach to another.
pub(crate) fn open_visit(shell: &mut ShellState) {
    close_visit(shell);
    let Some(scope) = Scope::capture(shell) else {
        return;
    };
    let Some(file) = shell.canvas.as_ref().map(|target| target.id.clone()) else {
        return;
    };
    if !super::surface::visible(shell, &file) || scope.deleted(shell, &file) {
        return;
    }
    shell.canvas_checklist.view = Some(Visit {
        scope,
        file,
        generation: shell.shortcut_generation,
        revision: 0,
        event_read_revision: 0,
        event_reads_blocked: false,
        settled_text: None,
        read: 0,
        loading: false,
        stale: true,
        snapshot: None,
        error: None,
    });
}
pub(crate) fn close_visit(shell: &mut ShellState) {
    let state = &mut shell.canvas_checklist;
    if let Some(view) = state.view.take() {
        state.operations.retain(|operation| {
            !(operation.stage == Stage::Preparing && operation.attempt.visit == view.generation)
        });
    }
}
pub(super) fn begin_read(shell: &mut ShellState) -> Option<Read> {
    let view = shell
        .canvas_checklist
        .view
        .as_ref()
        .filter(|view| view_current(shell, view))?;
    if view.loading {
        return None;
    }
    let read = Read {
        scope: view.scope.clone(),
        file: view.file.clone(),
        visit: view.generation,
        serial: 0,
        revision: view.revision,
    };
    let serial = shell.canvas_checklist.next();
    let view = shell.canvas_checklist.view.as_mut()?;
    view.read = serial;
    view.event_read_revision = view.revision;
    view.event_reads_blocked = false;
    view.loading = true;
    view.stale = true;
    view.error = None;
    Some(Read { serial, ..read })
}
/// Replace one successfully completed but event-invalidated canonical read.
/// This is a bounded readback recovery, never a retry of a checklist write.
fn retry_invalidated_read(shell: &mut ShellState, read: &Read, retries: &mut u8) -> Option<Read> {
    if *retries == 0
        || !read.scope.current(shell)
        || operation_index(shell, &read.file).is_none_or(|index| {
            shell.canvas_checklist.operations[index].stage != Stage::AwaitingRead
        })
    {
        return None;
    }
    let eligible = shell.canvas_checklist.view.as_ref().is_some_and(|view| {
        view_current(shell, view)
            && view.generation == read.visit
            && view.file == read.file
            && view.read == read.serial
            && view.revision != read.revision
            && !view.loading
    });
    if !eligible {
        return None;
    }
    let next = begin_read(shell)?;
    *retries -= 1;
    Some(next)
}
impl Read {
    pub(super) fn owns_view(&self, shell: &ShellState) -> bool {
        self.scope.current(shell)
            && shell.canvas_checklist.view.as_ref().is_some_and(|view| {
                view_current(shell, view)
                    && view.generation == self.visit
                    && view.file == self.file
                    && view.read == self.serial
            })
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.owns_view(shell)
            && shell
                .canvas_checklist
                .view
                .as_ref()
                .is_some_and(|view| view.revision == self.revision)
    }
    pub(super) fn request(&self, shell: &ShellState) -> Option<crate::scheduled::Context> {
        self.current(shell)
            .then(|| self.scope.request(shell))
            .flatten()
    }
}
pub(super) fn finish_read(
    shell: &mut ShellState,
    read: &Read,
    result: Result<Arc<CanvasCollabSnapshot>, &'static str>,
) -> bool {
    if !read.current(shell) {
        return false;
    }
    let view = shell.canvas_checklist.view.as_mut().unwrap();
    view.loading = false;
    match result {
        Ok(snapshot) if snapshot.file_id() == read.file => {
            view.snapshot = Some(snapshot);
            view.stale = false;
            view.error = None;
        }
        Ok(_) => {
            view.error = Some("The canvas identity changed. Refresh to check it.");
        }
        Err(error) => {
            view.event_reads_blocked = true;
            view.error = Some(error);
        }
    }
    true
}
/// Retire the read token before a relevant known canonical/file event is applied.
pub(crate) fn invalidate(shell: &mut ShellState, team: &str, file: &str) -> bool {
    let Some(view) =
        shell.canvas_checklist.view.as_ref().filter(|view| {
            view_current(shell, view) && view.scope.team == team && view.file == file
        })
    else {
        return false;
    };
    let revision = view.revision.wrapping_add(1);
    let view = shell.canvas_checklist.view.as_mut().unwrap();
    view.revision = revision;
    view.loading = false;
    view.stale = true;
    true
}
pub(super) fn presentation(shell: &ShellState) -> Option<Presentation> {
    let view = shell
        .canvas_checklist
        .view
        .as_ref()
        .filter(|view| view_current(shell, view))?;
    let operation = operation_index(shell, &view.file).map(|index| {
        let operation = &shell.canvas_checklist.operations[index];
        (
            operation.attempt.serial,
            operation.attempt.section.clone(),
            operation.stage,
        )
    });
    Some(Presentation {
        visit: view.generation,
        read: view.read,
        loading: view.loading,
        stale: view.stale,
        snapshot: view.snapshot.clone(),
        operation,
        error: view.error,
        outstanding_total: shell.canvas_checklist.operations.len(),
        capacity_reason: capacity_reason(shell),
    })
}
fn capacity_reason(shell: &ShellState) -> Option<&'static str> {
    if let Some(reason) = recovery::reason(shell) {
        return Some(reason);
    }
    // Never discard an uncertain operation to make room. The per-owner limit
    // survives credential rotation; the global ceiling bounds retained snapshots
    // across accounts and is surfaced explicitly, not attributed to this canvas.
    if shell
        .canvas_checklist
        .operations
        .iter()
        .filter(|operation| operation.attempt.scope.stable_current(shell))
        .count()
        + super::comments::mutation::retained_count(shell).0
        >= MAX_RECEIPTS
    {
        Some("Eight canvas changes in this workspace still need review before another edit.")
    } else if shell.canvas_checklist.operations.len()
        + super::comments::mutation::retained_count(shell).1
        >= MAX_TOTAL_RECEIPTS
    {
        Some(
            "32 canvas changes are retained across accounts. Review those changes before starting another edit.",
        )
    } else {
        None
    }
}

/// Called only after the core builder has identified an eligible existing item.
/// This function enforces ownership; it deliberately grants no protocol permission.
pub(super) fn begin(
    shell: &mut ShellState,
    visit: u64,
    read: u64,
    section: &str,
    checked: bool,
) -> Result<Attempt, &'static str> {
    let view = shell
        .canvas_checklist
        .view
        .as_ref()
        .filter(|view| view_current(shell, view) && view.generation == visit && view.read == read)
        .ok_or("This canvas view changed. Refresh before editing.")?;
    if view.loading || view.stale {
        return Err("Refresh this canvas before editing.");
    }
    if operation_index(shell, &view.file).is_some()
        || super::comments::mutation::pending_for_current_file(shell)
        || super::comments::reply::pending_for_current_file(shell)
        || super::comments::annotation::pending_for_current_file(shell)
    {
        return Err("Wait for the previous change to be checked.");
    }
    if let Some(reason) = capacity_reason(shell) {
        return Err(reason);
    }
    let snapshot = view
        .snapshot
        .clone()
        .ok_or("Load this canvas before editing.")?;
    if snapshot.section(section).is_none() {
        return Err("This checklist item is no longer available.");
    }
    let attempt = Attempt {
        scope: view.scope.clone(),
        serial: 0,
        visit,
        read,
        revision: view.revision,
        snapshot,
        section: section.into(),
        mutation: Mutation::Checkbox(checked),
    };
    let serial = shell.canvas_checklist.next();
    let attempt = Attempt { serial, ..attempt };
    shell.canvas_checklist.operations.push(Operation {
        attempt: attempt.clone(),
        stage: Stage::Preparing,
        dispatched_after_read: read,
        dispatched: None,
        structural_request: None,
        accepted: None,
        durable: None,
        durable_verified: None,
        durable_finishing: false,
    });
    Ok(attempt)
}
impl Attempt {
    pub(super) fn checked(&self) -> Option<bool> {
        match self.mutation {
            Mutation::Checkbox(value) => Some(value),
            _ => None,
        }
    }
    pub(super) fn visit(&self) -> u64 {
        self.visit
    }
    pub(super) fn request(&self, shell: &ShellState) -> Option<crate::scheduled::Context> {
        dispatchable(shell, self)
            .then(|| self.scope.request(shell))
            .flatten()
    }
}
pub(super) fn dispatchable(shell: &ShellState, attempt: &Attempt) -> bool {
    attempt.scope.current(shell)
        && shell.canvas_checklist.operations.iter().any(|operation| {
            operation.attempt.serial == attempt.serial && operation.stage == Stage::Preparing
        })
        && shell.canvas_checklist.view.as_ref().is_some_and(|view| {
            view_current(shell, view)
                && view.generation == attempt.visit
                && view.read == attempt.read
                && view.revision == attempt.revision
                && !view.stale
                && !view.loading
                && view.file == attempt.snapshot.file_id()
                && view
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| Arc::ptr_eq(snapshot, &attempt.snapshot))
        })
}
/// Fresh core revalidation must succeed immediately before this transition and
/// no await may intervene between this guard and a no-hold one-attempt send.
pub(super) fn mark_dispatched(
    shell: &mut ShellState,
    attempt: &Attempt,
    prepared: &PreparedCheckboxEdit,
) -> bool {
    if !dispatchable(shell, attempt)
        || prepared.file_id() != attempt.snapshot.file_id()
        || prepared.thread_id() != attempt.snapshot.thread_id()
        || prepared.document_id() != attempt.snapshot.document_id()
        || prepared.target_section() != attempt.section
        || Some(prepared.desired()) != attempt.checked()
    {
        return false;
    }
    let operation = shell
        .canvas_checklist
        .operations
        .iter_mut()
        .find(|operation| operation.attempt.serial == attempt.serial)
        .unwrap();
    operation.stage = Stage::Saving;
    operation.dispatched = Some((prepared.session_id().into(), prepared.sequence()));
    if let Some(view) = shell.canvas_checklist.view.as_mut() {
        view.stale = true;
    }
    true
}
pub(super) fn cancel_preflight(shell: &mut ShellState, attempt: &Attempt) -> bool {
    let Some(index) = shell
        .canvas_checklist
        .operations
        .iter()
        .position(|operation| {
            operation.attempt.serial == attempt.serial && operation.stage == Stage::Preparing
        })
    else {
        return false;
    };
    shell.canvas_checklist.operations.remove(index);
    true
}
/// Sent results settle their retained receipt even after navigation. They never
/// install a document, show a toast in another account, or open a replacement UI.
pub(super) fn finish(shell: &mut ShellState, attempt: &Attempt, outcome: Outcome) -> bool {
    let Some(index) = shell
        .canvas_checklist
        .operations
        .iter()
        .position(|operation| {
            operation.attempt.serial == attempt.serial && operation.stage == Stage::Saving
        })
    else {
        return false;
    };
    match outcome {
        Outcome::Accepted(accepted) => {
            let operation = &shell.canvas_checklist.operations[index];
            let section = accepted.section();
            let matches = operation
                .dispatched
                .as_ref()
                .is_some_and(|(session, sequence)| {
                    session == accepted.session_id() && *sequence == accepted.sequence()
                })
                && section.id() == attempt.section
                && section.thread_id().ok().flatten() == Some(attempt.snapshot.thread_id())
                && section.document_id().ok().flatten() == Some(attempt.snapshot.document_id())
                && section.checked().ok().flatten() == attempt.checked();
            if matches && attempt.scope.account_current(shell) {
                // ACKs may settle after navigation, but only the original full
                // authenticated scope may retain its CRDT in an existing session.
                // Never create/adopt a session while processing a late receipt.
                let owned = shell.canvas_checklist.sessions.iter().position(|owned| {
                    owned.scope.team == attempt.scope.team && owned.scope.account_current(shell)
                });
                if let Some(owned) = owned {
                    let _ = shell.canvas_checklist.sessions[owned]
                        .session
                        .remember_accepted(&accepted);
                }
            }
            let operation = &mut shell.canvas_checklist.operations[index];
            operation.stage = if matches {
                Stage::AwaitingRead
            } else {
                Stage::Uncertain
            };
            if matches {
                operation.accepted = Some(Accepted::Checkbox(accepted));
            }
        }
        Outcome::Uncertain => shell.canvas_checklist.operations[index].stage = Stage::Uncertain,
        Outcome::Rejected(error) => {
            shell.canvas_checklist.operations.remove(index);
            if let Some(view) = shell.canvas_checklist.view.as_ref().filter(|view| {
                attempt.scope.stable_current(shell)
                    && view_current(shell, view)
                    && view.generation == attempt.visit
                    && view.file == attempt.snapshot.file_id()
            }) {
                let _ = view;
                shell.canvas_checklist.view.as_mut().unwrap().error = Some(error);
            }
        }
    }
    true
}
pub(super) fn pending_attempt(shell: &ShellState) -> Option<Attempt> {
    let view = shell
        .canvas_checklist
        .view
        .as_ref()
        .filter(|view| view_current(shell, view))?;
    let operation = &shell.canvas_checklist.operations[operation_index(shell, &view.file)?];
    matches!(operation.stage, Stage::AwaitingRead | Stage::Uncertain)
        .then(|| operation.attempt.clone())
}
/// Narrow integration point for a core-verified identity/sequence/state proof.
/// A boolean match or an HTML export is NOT a proof. This is intentionally only
/// visible inside canvas, not a UI-dispatchable acceptance API.
pub(super) fn confirm_readback(shell: &mut ShellState, attempt: &Attempt, read: &Read) -> bool {
    if !read.current(shell)
        || !attempt.scope.stable_current(shell)
        || read.file != attempt.snapshot.file_id()
    {
        return false;
    }
    let Some(view) = shell
        .canvas_checklist
        .view
        .as_ref()
        .filter(|view| !view.stale && !view.loading)
    else {
        return false;
    };
    let Some(snapshot) = view.snapshot.as_ref() else {
        return false;
    };
    if snapshot.thread_id() != attempt.snapshot.thread_id()
        || snapshot.document_id() != attempt.snapshot.document_id()
    {
        return false;
    }
    let Some(index) = shell
        .canvas_checklist
        .operations
        .iter()
        .position(|operation| {
            operation.attempt.serial == attempt.serial
                && read.serial > operation.dispatched_after_read
                && operation.stage == Stage::AwaitingRead
        })
    else {
        return false;
    };
    let Some(accepted) = shell.canvas_checklist.operations[index].accepted.as_ref() else {
        return false;
    };
    if let Accepted::Replacement(accepted) = accepted {
        if !accepted.confirms_readback(snapshot).unwrap_or(false) {
            return false;
        }
        if shell.canvas_checklist.operations[index].durable.is_some() {
            shell.canvas_checklist.operations[index].durable_verified = Some(read.serial);
            return false;
        }
        shell.canvas_checklist.view.as_mut().unwrap().settled_text = Some(attempt.serial);
        shell.canvas_checklist.operations.remove(index);
        return true;
    }
    if let Accepted::Structure(accepted) = accepted {
        if !accepted.confirms_readback(snapshot).unwrap_or(false) {
            return false;
        }
        // The source proof alone must not unlock before its durable receipt is
        // removed. Only recovery::settle consumes this exact verified read.
        if shell.canvas_checklist.operations[index].durable.is_some() {
            shell.canvas_checklist.operations[index].durable_verified = Some(read.serial);
            return false;
        }
        shell.canvas_checklist.view.as_mut().unwrap().settled_text = Some(attempt.serial);
        shell.canvas_checklist.operations.remove(index);
        return true;
    }
    if let Accepted::Text(accepted) = accepted {
        if !accepted.confirms_readback(snapshot).unwrap_or(false) {
            return false;
        }
        shell.canvas_checklist.view.as_mut().unwrap().settled_text = Some(attempt.serial);
        shell.canvas_checklist.operations.remove(index);
        return true;
    }
    let Accepted::Checkbox(accepted) = accepted else {
        unreachable!()
    };
    let Some(current) = snapshot.section(&attempt.section) else {
        return false;
    };
    let Some(accepted_sequence) = accepted.section().sequence().ok().flatten() else {
        return false;
    };
    let Some(current_sequence) = current.sequence().ok().flatten() else {
        return false;
    };
    if current.deleted().ok().flatten() == Some(true)
        || current_sequence < accepted_sequence
        || (current_sequence == accepted_sequence
            && current.checked().ok().flatten() != accepted.section().checked().ok().flatten())
    {
        return false;
    }
    shell.canvas_checklist.operations.remove(index);
    true
}

/// The collaboration session belongs to the full authenticated owner and is
/// shared across its canvas visits. The callback reserves no counters unless
/// the core intent accepts its fresh snapshot.
pub(super) fn with_edit_session<T>(
    shell: &mut ShellState,
    attempt: &Attempt,
    prepare: impl FnOnce(&mut EditSession) -> T,
) -> Option<T> {
    if !dispatchable(shell, attempt) {
        return None;
    }
    let retained = std::mem::take(&mut shell.canvas_checklist.sessions);
    shell.canvas_checklist.sessions = retained
        .into_iter()
        .filter(|owned| owned.scope.account_current(shell))
        .collect();
    let index = shell
        .canvas_checklist
        .sessions
        .iter()
        .position(|owned| owned.scope.current(shell));
    let index = index.unwrap_or_else(|| {
        shell.canvas_checklist.sessions.push(OwnedSession {
            scope: attempt.scope.clone(),
            session: EditSession::new(),
        });
        shell.canvas_checklist.sessions.len() - 1
    });
    Some(prepare(&mut shell.canvas_checklist.sessions[index].session))
}

/// Shared file-wide custody; comments cannot overlap a document edit.
pub(in crate::canvas) fn has_pending_write(shell: &ShellState) -> bool {
    shell
        .canvas
        .as_ref()
        .is_some_and(|canvas| operation_index(shell, &canvas.id).is_some())
}

pub(in crate::canvas) fn document_operation_count(shell: &ShellState) -> (usize, usize) {
    let operations = &shell.canvas_checklist.operations;
    (
        operations
            .iter()
            .filter(|operation| operation.attempt.scope.stable_current(shell))
            .count(),
        operations.len(),
    )
}
