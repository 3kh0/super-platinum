//! Personal To-do completion coordinates distinct existing cell writes. A
//! retained step is never inferred successful from its transport acknowledgement.
use super::*;
use super_platinum_core::slack_lists::{TaskCompletion, TaskCompletionCell};
#[cfg(test)]
mod custody_tests;
pub(crate) mod fixture;
mod lifecycle;
mod network;
#[cfg(test)]
mod tests;
use lifecycle::*;
type Owner = (Option<String>, String, String);

#[derive(Clone, Default)]
pub(crate) struct State {
    entries: BTreeMap<u64, Entry>,
    waiting: bool,
    fixture: fixture::State,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Preparing,
    PrimarySending,
    PrimaryUnknown,
    ReferenceReady,
    ReferenceSending,
    ReferenceUnknown,
    Stopped,
}
#[derive(Clone)]
struct Entry {
    owner: Owner,
    serial: u64,
    intent: TaskCompletion,
    reference: Option<TaskCompletionCell>,
    phase: Phase,
    running: Option<u64>,
    retry_at: i64,
    primary_observed: bool,
    message: String,
}
#[derive(Clone)]
struct Task {
    serial: u64,
    token: u64,
    scope: Scope,
    visit: u64,
    revision: u64,
    selection: u64,
    authority: Option<EditAuthority>,
}
#[derive(Clone, PartialEq)]
pub(crate) struct Presentation {
    pub serial: u64,
    pub message: String,
    pub pending: bool,
    pub can_check: bool,
    pub can_continue: bool,
    pub can_dismiss: bool,
}
/// In-process retention only. A different account keeps these entries inert;
/// exact owner and fresh dispatch authority are checked before any new action.
pub(crate) fn carry_pending(previous: &mut ShellState, loaded: &mut ShellState) {
    // Environment loading constructs a fresh shell. Check every key before
    // moving anything, so an invalid caller cannot overwrite either receipt.
    assert!(
        previous
            .slack_lists
            .completion
            .entries
            .keys()
            .all(|serial| !loaded.slack_lists.completion.entries.contains_key(serial)),
        "duplicate completion receipt"
    );
    for (serial, entry) in std::mem::take(&mut previous.slack_lists.completion.entries) {
        assert!(
            loaded
                .slack_lists
                .completion
                .entries
                .insert(serial, entry)
                .is_none(),
            "duplicate completion receipt"
        );
    }
}
fn conflicts(shell: &ShellState, scope: &Scope, list: &str) -> bool {
    cell_receipts::blocks_target(shell, scope, list)
        || rename::blocks_target(scope, list)
        || create::blocks_target(shell, scope, list)
        || delete::blocks_target(shell, scope, list)
        || reminder::blocks_target(shell, scope, list)
        || editor::assignment::blocks_target(scope, list)
}
fn list(entry: &Entry) -> &str {
    entry.intent.primary().edit().list_id()
}
fn current_entry(shell: &ShellState) -> Option<&Entry> {
    let scope = shell
        .slack_lists
        .scope
        .as_ref()
        .filter(|s| s.current(shell))?;
    let view = shell.slack_lists.view.as_ref()?;
    shell
        .slack_lists
        .completion
        .entries
        .values()
        .find(|e| e.owner == scope.stable_owner() && list(e) == view.id)
}
pub(crate) fn blocks_mutation(shell: &ShellState) -> bool {
    let Some(scope) = shell
        .slack_lists
        .scope
        .as_ref()
        .filter(|s| s.current(shell))
    else {
        return false;
    };
    let Some(view) = shell.slack_lists.view.as_ref() else {
        return false;
    };
    shell.slack_lists.completion.entries.values().any(|e| {
        e.owner == scope.stable_owner()
            && (list(e) == view.id || e.intent.reference().is_some_and(|r| r.list_id == view.id))
    })
}
pub(super) fn blocks_read(shell: &ShellState) -> bool {
    current_entry(shell).is_some_and(|e| e.running.is_some())
}
impl Task {
    fn owned(&self, shell: &ShellState) -> bool {
        shell
            .slack_lists
            .completion
            .entries
            .get(&self.serial)
            .is_some_and(|e| e.owner == self.scope.stable_owner() && e.running == Some(self.token))
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.owned(shell)
            && self.scope.current(shell)
            && shell
                .slack_lists
                .scope
                .as_ref()
                .is_some_and(|s| std::sync::Arc::ptr_eq(&s.identity, &self.scope.identity))
            && shell.overlay == Some(Overlay::SlackList)
            && shell
                .slack_lists
                .completion
                .entries
                .get(&self.serial)
                .is_some_and(|e| {
                    e.retry_at <= now()
                        && !conflicts(shell, &self.scope, list(e))
                        && !e
                            .intent
                            .reference()
                            .is_some_and(|r| conflicts(shell, &self.scope, &r.list_id))
                        && !self.scope.deleted(shell, list(e))
                        && !e
                            .intent
                            .reference()
                            .is_some_and(|r| self.scope.deleted(shell, &r.list_id))
                        && shell.slack_lists.view.as_ref().is_some_and(|v| {
                            v.id == list(e)
                                && v.generation == self.visit
                                && v.revision == self.revision
                                && v.layout.selection_generation == self.selection
                                && !v.unavailable
                                && self.authority.is_none_or(|a| {
                                    a.permits_dispatch(
                                        v,
                                        e.intent.primary().edit().record_id(),
                                        e.intent.primary().edit().column_id(),
                                    )
                                })
                        })
                })
    }
}
fn task(shell: &mut ShellState, serial: u64) -> Option<Task> {
    let scope = shell
        .slack_lists
        .scope
        .clone()
        .filter(|s| s.current(shell))?;
    let view = shell.slack_lists.view.as_ref()?;
    let entry = shell.slack_lists.completion.entries.get_mut(&serial)?;
    if entry.owner != scope.stable_owner()
        || list(entry) != view.id
        || entry.running.is_some()
        || entry.retry_at > now()
        || view.loading
        || shell.overlay != Some(Overlay::SlackList)
    {
        return None;
    }
    let task = Task {
        serial,
        token: next_id(),
        scope,
        visit: view.generation,
        revision: view.revision,
        selection: view.layout.selection_generation,
        authority: None,
    };
    entry.running = Some(task.token);
    Some(task)
}
fn dispatch_task(shell: &mut ShellState, serial: u64) -> Option<Task> {
    let authority = record_detail::presentation(shell.slack_lists.view.as_ref()?)
        .map_or(EditAuthority::Layout, |d| {
            EditAuthority::RecordDetail(d.generation)
        });
    let mut task = task(shell, serial)?;
    task.authority = Some(authority);
    Some(task)
}
fn continuation_task(shell: &mut ShellState, previous: &Task) -> Option<Task> {
    // Automatic continuation retains the original gesture's authority; it may
    // never adopt a replacement detail. Explicit Continue captures its own owner.
    let authority = previous.authority?;
    let mut next = task(shell, previous.serial)?;
    next.authority = Some(authority);
    Some(next)
}
pub(super) fn is_personal_cell(shell: &ShellState, column: &str) -> bool {
    shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .is_some_and(|l| {
            l.document
                .column(column)
                .is_some_and(|c| c.kind == "todo_completed")
                && l.personal_task_lists
                    .as_ref()
                    .is_some_and(|p| p.contains(&l.document.id))
        })
}
pub(super) fn toggle(
    mut state: Signal<ShellState>,
    row: String,
    column: String,
    authority: EditAuthority,
) {
    let result = begin(&mut state.write(), &row, &column, authority);
    match result {
        Ok(task) => network::start(state, task),
        Err(error) => {
            if let Some(view) = state.write().slack_lists.view.as_mut() {
                view.error = Some(error.into());
            }
        }
    }
}
fn begin(
    shell: &mut ShellState,
    row: &str,
    column: &str,
    authority: EditAuthority,
) -> Result<Task, &'static str> {
    let scope = shell
        .slack_lists
        .scope
        .as_ref()
        .filter(|s| s.current(shell))
        .ok_or("Open this List again.")?;
    let view = shell
        .slack_lists
        .view
        .as_ref()
        .ok_or("Open this List again.")?;
    if shell.overlay != Some(Overlay::SlackList)
        || view.stale
        || view.loading
        || view.unavailable
        || view.retry_at > now()
        || view.editor.is_some()
        || !authority.admits(view, row, column)
        || matches!(authority, EditAuthority::BoardMove(_))
        || rename::blocks_mutation(shell)
        || create::blocks_mutation(shell)
        || delete::blocks_mutation(shell)
        || reminder::blocks_mutation(shell)
        || editor::assignment::blocks_mutation(shell)
        || blocks_mutation(shell)
        || shell.slack_lists.operations.contains_key(&view.id)
    {
        return Err("Finish the current change or refresh this To-do first.");
    }
    let owner = scope.stable_owner();
    if shell.slack_lists.completion.entries.len() >= 32
        || shell
            .slack_lists
            .completion
            .entries
            .values()
            .filter(|e| e.owner == owner)
            .count()
            >= 8
    {
        return Err("Check earlier To-do changes before starting another.");
    }
    let loaded = view.loaded.as_ref().ok_or("Wait for this To-do to load.")?;
    let personal = loaded
        .personal_task_lists
        .as_ref()
        .ok_or("Refresh your To-do access first.")?;
    let record = loaded
        .records
        .record(row)
        .ok_or("This To-do is unavailable.")?;
    let field = loaded
        .document
        .column(column)
        .ok_or("This field is unavailable.")?;
    let initial = field
        .personal_completion_value(record, &loaded.document.id, personal)
        .map_err(|e| e.0)?;
    let intent = TaskCompletion::capture(
        &loaded.document,
        &loaded.records,
        row,
        column,
        loaded.allow_lists.as_deref(),
        personal,
        !initial,
    )
    .map_err(|e| e.0)?;
    if intent.reference().is_some_and(|r| {
        conflicts(shell, scope, &r.list_id)
            || shell.slack_lists.completion.entries.values().any(|e| {
                e.owner == owner
                    && (list(e) == r.list_id
                        || e.intent
                            .reference()
                            .is_some_and(|other| other.list_id == r.list_id))
            })
    }) {
        return Err("Check the pending referenced List change first.");
    }
    let serial = next_id();
    shell.slack_lists.completion.entries.insert(
        serial,
        Entry {
            owner,
            serial,
            intent,
            reference: None,
            phase: Phase::Preparing,
            running: None,
            retry_at: 0,
            primary_observed: false,
            message: "Checking the To-do and its referenced item…".into(),
        },
    );
    let mut task = task(shell, serial).ok_or("This To-do visit changed.")?;
    task.authority = Some(authority);
    Ok(task)
}
pub(crate) fn check(mut state: Signal<ShellState>, serial: u64) {
    if !current_entry(&state.peek()).is_some_and(|e| {
        e.serial == serial && matches!(e.phase, Phase::PrimaryUnknown | Phase::ReferenceUnknown)
    }) {
        return;
    }
    let next = task(&mut state.write(), serial);
    if let Some(task) = next {
        network::check(state, task, false);
    }
}
pub(crate) fn continue_reference(mut state: Signal<ShellState>, serial: u64) {
    if !current_entry(&state.peek())
        .is_some_and(|e| e.serial == serial && e.phase == Phase::ReferenceReady)
    {
        return;
    }
    let next = dispatch_task(&mut state.write(), serial);
    if let Some(task) = next {
        network::reference(state, task);
    }
}
pub(crate) fn dismiss(mut state: Signal<ShellState>, serial: u64) {
    let shell = &mut state.write();
    if current_entry(shell).is_some_and(|e| {
        e.serial == serial
            && e.phase == Phase::Stopped
            && e.running.is_none()
            && e.retry_at <= now()
    }) {
        shell.slack_lists.completion.entries.remove(&serial);
    }
}
pub(crate) fn presentation(shell: &ShellState) -> Option<Presentation> {
    let entry = current_entry(shell)?;
    let available = entry.running.is_none() && entry.retry_at <= now();
    Some(Presentation {
        serial: entry.serial,
        message: if entry.retry_at > now() {
            "Slack asked us to wait. This To-do change will not be sent again.".into()
        } else {
            entry.message.clone()
        },
        pending: entry.running.is_some(),
        can_check: available
            && matches!(entry.phase, Phase::PrimaryUnknown | Phase::ReferenceUnknown),
        can_continue: available && entry.phase == Phase::ReferenceReady,
        can_dismiss: available && entry.phase == Phase::Stopped,
    })
}
pub(super) fn poll(shell: &mut ShellState) -> bool {
    let waiting = current_entry(shell).is_some_and(|e| e.retry_at > now());
    let changed = waiting != shell.slack_lists.completion.waiting;
    shell.slack_lists.completion.waiting = waiting;
    changed
}
