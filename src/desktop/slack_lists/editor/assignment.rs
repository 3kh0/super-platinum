//! Special assignees retain a closed TaskAssignment and separate share receipts.
//! The generic Users capture stays closed. No stored receipt contains credentials.
use super::*;
use super_platinum_core::{
    slack::api::{self, AssigneeGrant},
    slack_lists::{TaskAssignment, TaskAssignmentDecision},
};
mod custody;
pub(crate) mod fixture;
pub(crate) use custody::{abandon, recovery};
mod owner;
mod worker;
use owner::Guard;
use std::time::{Duration, Instant};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    owner: (Option<String>, String, String),
    list: String,
}
impl Key {
    fn current(shell: &ShellState) -> Option<Self> {
        let scope = shell
            .slack_lists
            .scope
            .as_ref()
            .filter(|s| s.current(shell))?;
        if shell.overlay != Some(Overlay::SlackList) {
            return None;
        }
        Some(Self {
            owner: scope.stable_owner(),
            list: shell.slack_lists.view.as_ref()?.id.clone(),
        })
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Stage {
    Checking,
    Review,
    Sharing,
    Uncertain,
    Blocked,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ShareResult {
    NotAttempted,
    Sending,
    Acknowledged,
    Uncertain,
    NotRequested,
    Blocked,
}
#[derive(Clone)]
struct Recipient {
    id: String,
    label: String,
    grant: AssigneeGrant,
    result: ShareResult,
    dm_attempted: bool,
    channel: Option<Value>,
}
#[derive(Clone)]
struct Entry {
    serial: u64,
    row: String,
    column: String,
    intent: TaskAssignment,
    records: Arc<RecordsPage>,
    selected: Vec<String>,
    recipients: Vec<Recipient>,
    stage: Stage,
    job: Option<u64>,
    message: String,
    deadline: Option<Instant>,
    cooldown_unknown: bool,
}
#[derive(Clone)]
struct ShareReceipt {
    key: Key,
    serial: u64,
    person: Recipient,
    observed: bool,
}
#[derive(Default)]
struct Registry {
    entries: BTreeMap<Key, Entry>,
    shares: Vec<ShareReceipt>,
    deadlines: BTreeMap<(Option<String>, String, String), Option<Instant>>,
    cooldown_overflow: bool,
}
#[cfg(not(test))]
fn registry<T>(f: impl FnOnce(&mut Registry) -> T) -> T {
    static REGISTRY: std::sync::OnceLock<std::sync::Mutex<Registry>> = std::sync::OnceLock::new();
    f(&mut REGISTRY
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner()))
}
#[cfg(test)]
thread_local! {static REGISTRY:std::cell::RefCell<Registry>=Default::default();}
#[cfg(test)]
fn registry<T>(f: impl FnOnce(&mut Registry) -> T) -> T {
    REGISTRY.with(|r| f(&mut r.borrow_mut()))
}
fn entry(shell: &ShellState) -> Option<Entry> {
    let key = Key::current(shell)?;
    registry(|r| r.entries.get(&key).cloned())
}
pub(in crate::slack_lists) fn blocks_target(scope: &Scope, list: &str) -> bool {
    let key = Key {
        owner: scope.stable_owner(),
        list: list.into(),
    };
    registry(|r| r.entries.contains_key(&key))
}
pub(crate) fn blocks_mutation(shell: &ShellState) -> bool {
    entry(shell).is_some()
}
pub(super) fn is_assignee(shell: &ShellState, generation: u64) -> bool {
    shell.slack_lists.view.as_ref().is_some_and(|v| {
        v.editor.as_ref().is_some_and(|d| {
            d.generation == generation
                && v.loaded.as_ref().is_some_and(|l| {
                    l.document
                        .column(&d.column_id)
                        .is_some_and(|c| matches!(c.kind.as_str(), "assignee" | "todo_assignee"))
                })
        })
    })
}
pub(super) fn can_open(shell: &ShellState, row: &str, column: &str) -> bool {
    entry(shell).is_none_or(|e| e.row == row && e.column == column)
}
#[derive(Clone, PartialEq)]
pub(crate) struct Person {
    pub id: String,
    pub label: String,
    pub grant: AssigneeGrant,
    pub status: &'static str,
}
#[derive(Clone, PartialEq)]
pub(crate) struct Presentation {
    pub serial: u64,
    pub stage: Stage,
    pub people: Vec<Person>,
    pub selected: String,
    pub message: String,
    pub busy: bool,
    pub can_share: bool,
    pub can_not_now: bool,
    pub can_check: bool,
}
pub(super) fn presentation(shell: &ShellState) -> Option<Presentation> {
    let e = entry(shell)?;
    let draft = shell.slack_lists.view.as_ref()?.editor.as_ref()?;
    if draft.row_id != e.row || draft.column_id != e.column {
        return None;
    }
    let current = Guard::capture(shell, &e).is_ok();
    let idle = e.job.is_none() && e.deadline.is_none_or(|d| Instant::now() >= d);
    let checked = e.intent.sharing_needed().is_ok();
    let missing = e.intent.sharing_needed().unwrap_or_default();
    Some(Presentation {
        serial: e.serial,
        stage: e.stage,
        selected: users::selected_labels(shell, &e.selected)
            .iter()
            .map(|p| p.label.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        people: e
            .recipients
            .iter()
            .map(|p| Person {
                id: p.id.clone(),
                label: p.label.clone(),
                grant: p.grant,
                status: match p.result {
                    _ if checked && !missing.contains(&p.id) => "Access verified",
                    ShareResult::NotAttempted if p.dm_attempted && p.channel.is_none() => {
                        "Recipient DM unconfirmed"
                    }
                    ShareResult::NotAttempted => "Not shared",
                    ShareResult::Sending => "Sharing…",
                    ShareResult::Acknowledged => "Share acknowledged; checking access",
                    ShareResult::Uncertain => "Share unconfirmed",
                    ShareResult::NotRequested => "Share not sent",
                    ShareResult::Blocked => "Sharing unavailable",
                },
            })
            .collect(),
        message: e.message,
        busy: e.job.is_some(),
        can_share: current
            && checked
            && idle
            && e.stage == Stage::Review
            && e.recipients.iter().any(|p| {
                missing.contains(&p.id)
                    && p.result == ShareResult::NotAttempted
                    && (!p.dm_attempted || p.channel.is_some())
            }),
        can_not_now: current && idle && checked,
        can_check: current && idle,
    })
}
pub(super) fn selected(shell: &ShellState, row: &str, column: &str) -> Option<Vec<String>> {
    entry(shell)
        .filter(|e| e.row == row && e.column == column)
        .map(|e| e.selected)
}
pub(super) fn start(mut state: Signal<ShellState>, generation: u64, value: CellEditValue) {
    let result = begin(&state.peek(), generation, value);
    match result {
        Ok(guard) => worker::start(state, guard, worker::Action::Access),
        Err(message) => set_error(&mut state.write(), generation, message),
    }
}
fn set_error(shell: &mut ShellState, generation: u64, message: &str) {
    if let Some(d) = shell
        .slack_lists
        .view
        .as_mut()
        .and_then(|v| v.editor.as_mut())
        .filter(|d| d.generation == generation)
    {
        d.error = Some(message.into());
    }
}
fn begin(shell: &ShellState, generation: u64, value: CellEditValue) -> Result<Guard, &'static str> {
    let key = Key::current(shell).ok_or("Open this List again.")?;
    if registry(|r| r.entries.contains_key(&key) || r.entries.len() >= 32 || r.shares.len() >= 256)
    {
        return Err("Check the retained assignment before starting another one.");
    }
    let view = shell
        .slack_lists
        .view
        .as_ref()
        .ok_or("Open this List again.")?;
    let draft = view
        .editor
        .as_ref()
        .filter(|d| d.generation == generation)
        .ok_or("Open the cell again.")?;
    let loaded = view.loaded.as_ref().ok_or("Wait for this List to load.")?;
    let CellEditValue::Users(selected) = value else {
        return Err("Choose people for this assignee field.");
    };
    if selected.len() > 64 || !custody::can_reserve(selected.len()) {
        return Err("This assignment exceeds the retained sharing receipt limit.");
    }
    if serde_json::to_vec(&loaded.records.raw).map_or(true, |b| b.len() > 2 * 1024 * 1024)
        || serde_json::to_vec(&loaded.document.raw).map_or(true, |b| b.len() > 2 * 1024 * 1024)
    {
        return Err("This List is too large for a retained assignment review.");
    }
    let personal = loaded
        .personal_task_lists
        .as_ref()
        .ok_or("Refresh your task preferences before assigning.")?;
    let intent = TaskAssignment::capture(
        &loaded.document,
        &loaded.records,
        &draft.row_id,
        &draft.column_id,
        loaded.allow_lists.as_deref(),
        personal,
        selected.clone(),
    )
    .map_err(|_| "This assignment cannot be safely saved.")?;
    let e = Entry {
        serial: next_id(),
        row: draft.row_id.clone(),
        column: draft.column_id.clone(),
        intent,
        records: Arc::new(loaded.records.clone()),
        selected,
        recipients: Vec::new(),
        stage: Stage::Checking,
        job: None,
        message: "Checking the selected people’s access…".into(),
        deadline: None,
        cooldown_unknown: false,
    };
    let guard = Guard::capture(shell, &e)?;
    custody::insert(key, e)?;
    Ok(guard)
}
pub(crate) fn grant(mut state: Signal<ShellState>, serial: u64, user: String, value: String) {
    let Some(e) = entry(&state.peek())
        .filter(|e| e.serial == serial && e.job.is_none() && e.stage == Stage::Review)
    else {
        return;
    };
    if Guard::capture(&state.peek(), &e).is_err()
        || !e
            .intent
            .sharing_needed()
            .is_ok_and(|ids| ids.contains(&user))
    {
        return;
    }
    let grant = match value.as_str() {
        "read" => AssigneeGrant::Read,
        "write" => AssigneeGrant::Write,
        _ => return,
    };
    let key = Key::current(&state.peek()).unwrap();
    registry(|r| {
        if let Some(p) = r.entries.get_mut(&key).and_then(|e| {
            e.recipients
                .iter_mut()
                .find(|p| p.id == user && p.result == ShareResult::NotAttempted)
        }) {
            p.grant = grant;
        }
    });
    let _ = state.write();
}
pub(crate) fn share(state: Signal<ShellState>, serial: u64) {
    action(state, serial, worker::Action::Share);
}
pub(crate) fn check(state: Signal<ShellState>, serial: u64) {
    action(state, serial, worker::Action::Access);
}
fn action(state: Signal<ShellState>, serial: u64, action: worker::Action) {
    let Some(e) = entry(&state.peek()).filter(|e| e.serial == serial && e.job.is_none()) else {
        return;
    };
    let Some(p) = presentation(&state.peek()) else {
        return;
    };
    if (action == worker::Action::Share && !p.can_share)
        || (action == worker::Action::Access && !p.can_check)
    {
        return;
    }
    if let Ok(guard) = Guard::capture(&state.peek(), &e) {
        worker::start(state, guard, action);
    }
}
pub(crate) fn not_now(mut state: Signal<ShellState>, serial: u64) {
    let Some(e) = entry(&state.peek()).filter(|e| e.serial == serial && e.job.is_none()) else {
        return;
    };
    if !presentation(&state.peek()).is_some_and(|p| p.can_not_now) {
        return;
    }
    let Ok(guard) = Guard::capture(&state.peek(), &e) else {
        return;
    };
    let result = admit(
        &mut state.write(),
        &guard,
        TaskAssignmentDecision::AssignWithoutSharing,
    );
    match result {
        Ok(attempt) => network::dispatch(state, attempt),
        Err(message) => set_error(&mut state.write(), guard.draft, message),
    }
}
fn admit(
    shell: &mut ShellState,
    guard: &Guard,
    decision: TaskAssignmentDecision,
) -> Result<EditAttempt, &'static str> {
    let e = guard.entry().ok_or("This assignment was replaced.")?;
    if !guard.current(shell) {
        return Err("The List changed. The assignment was not sent.");
    }
    let edit =
        e.intent.clone().into_edit(decision).map_err(
            |_| "Access is still unconfirmed. Choose Not Now to assign without sharing.",
        )?;
    custody::can_archive(&guard.key, &e)?;
    let attempt = super::begin_save_verified(
        shell,
        guard.draft,
        CellEditValue::Users(e.selected.clone()),
        Some(edit),
    )?;
    // Capacity is reserved at begin and checked before cell admission. This
    // synchronous transfer cannot race another event or drop a share receipt.
    custody::archive(&guard.key, &e).expect("reserved assignment receipt capacity");
    Ok(attempt)
}
/// Cancel is zero-assignment. Once any share/open was attempted its receipt is
/// retained across editor closure and account/visit replacement, never replayed.
pub(super) fn cancel(shell: &ShellState) {
    let Some(key) = Key::current(shell) else {
        return;
    };
    registry(|r| {
        if r.entries.get(&key).is_some_and(|e| {
            e.job.is_none()
                && e.recipients
                    .iter()
                    .all(|p| !p.dm_attempted && p.result == ShareResult::NotAttempted)
        }) {
            r.entries.remove(&key);
        }
    });
}
fn cool_down(entry: &mut Entry, error: &Error) {
    let seconds = match error {
        Error::RateLimited { retry_after_secs } => Some(retry_after_secs.unwrap_or(60)),
        Error::HttpStatus {
            retry_after_secs, ..
        } => *retry_after_secs,
        _ => None,
    };
    if let Some(seconds) = seconds {
        entry.deadline = Instant::now().checked_add(Duration::from_secs(seconds.max(1)));
        if entry.deadline.is_none() {
            entry.stage = Stage::Blocked;
            entry.cooldown_unknown = true;
        }
    }
}
#[cfg(test)]
mod tests;

fn previous_share(key: &Key, user: &str) -> Option<Recipient> {
    registry(|r| {
        r.shares
            .iter()
            .rev()
            .find(|receipt| {
                receipt.key == *key
                    && receipt.person.id == user
                    && !receipt.observed
                    && (receipt.person.dm_attempted
                        || matches!(
                            receipt.person.result,
                            ShareResult::Sending
                                | ShareResult::Acknowledged
                                | ShareResult::Uncertain
                        ))
            })
            .map(|receipt| receipt.person.clone())
    })
}
fn observe_previous_share(key: &Key, user: &str) {
    registry(|r| {
        for receipt in &mut r.shares {
            if receipt.key == *key && receipt.person.id == user {
                receipt.observed = true;
            }
        }
    });
}

fn owner_blocked(key: &Key) -> bool {
    registry(|r| {
        r.cooldown_overflow
            || r.deadlines
                .get(&key.owner)
                .is_some_and(|deadline| deadline.is_none_or(|at| Instant::now() < at))
    })
}
fn record_cooldown(key: &Key, error: &Error) {
    let seconds = match error {
        Error::RateLimited { retry_after_secs } => Some(retry_after_secs.unwrap_or(60)),
        Error::HttpStatus {
            retry_after_secs, ..
        } => *retry_after_secs,
        _ => None,
    };
    let Some(seconds) = seconds else {
        return;
    };
    let until = Instant::now().checked_add(Duration::from_secs(seconds.max(1)));
    registry(|r| {
        if r.deadlines.len() >= 128 && !r.deadlines.contains_key(&key.owner) {
            r.cooldown_overflow = true;
            return;
        }
        r.deadlines
            .entry(key.owner.clone())
            .and_modify(|old| {
                *old = match (*old, until) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    _ => None,
                };
            })
            .or_insert(until);
    });
}
