//! Personal record reminders. A sent operation is retained independently of its
//! dialog, and is never repeated merely because its response or visit was lost.
use super::*;
use super_platinum_core::{
    later::SavedItem,
    slack::transport::ListReminderFailure,
    slack_lists::{ListReminderChange, ListReminderEdit},
};
pub(crate) mod fixture;
mod lifecycle;
mod network;
#[cfg(test)]
mod tests;
use lifecycle::*;
type Owner = (Option<String>, String, String);
type Key = (Owner, String);
#[derive(Clone, Default)]
pub(crate) struct State {
    draft: Option<Draft>,
    receipts: BTreeMap<Key, Receipt>,
    fixture: fixture::State,
    cooldown_visible: bool,
    cooldowns: BTreeMap<Key, i64>,
    loads: BTreeMap<u64, Key>,
    preflights: BTreeMap<u64, Key>,
}
#[derive(Clone)]
struct Draft {
    origin: Origin,
    loaded: Option<Loaded>,
    saved: Option<SavedItem>,
    loading: bool,
    error: Option<String>,
    retry_at: i64,
}
#[derive(Clone)]
struct Origin {
    scope: Scope,
    serial: u64,
    visit: u64,
    revision: u64,
    selection: u64,
    detail: u64,
    open: u64,
    list: String,
    row: String,
    title: String,
    original: Value,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Preparing,
    Sending,
    Uncertain,
}
#[derive(Clone)]
struct Attempt {
    origin: Origin,
    intent: ListReminderEdit,
}
#[derive(Clone)]
struct Receipt {
    attempt: Attempt,
    phase: Phase,
    response_seen: bool,
    retry_at: i64,
    checking: Option<u64>,
    message: String,
}
#[derive(Clone)]
struct Read {
    key: Key,
    serial: u64,
    token: u64,
    scope: Scope,
    visit: u64,
}
#[derive(Clone, PartialEq)]
pub(crate) struct Presentation {
    pub generation: u64,
    pub title: String,
    pub due: i64,
    pub editable: bool,
    pub ready: bool,
    pub loading: bool,
    pub can_clear: bool,
    pub can_check: bool,
    pub pending: bool,
    pub message: Option<String>,
}
impl Origin {
    fn key(&self) -> Key {
        (self.scope.stable_owner(), self.list.clone())
    }
    fn visible(&self, shell: &ShellState) -> bool {
        self.scope.current(shell)
            && shell.overlay == Some(Overlay::SlackList)
            && shell
                .slack_lists
                .view
                .as_ref()
                .is_some_and(|v| v.id == self.list && v.generation == self.visit && !v.unavailable)
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.visible(shell)
            && shell.slack_lists.view.as_ref().is_some_and(|v| {
                v.revision == self.revision
                    && v.layout.selection_generation == self.selection
                    && !v.stale
                    && !v.loading
                    && v.retry_at <= now()
                    && record_detail::delete_target(v, self.detail)
                        .is_some_and(|(open, row, _)| open == self.open && row == self.row)
                    && v.loaded
                        .as_ref()
                        .and_then(|l| l.records.record(&self.row))
                        .is_some_and(|row| row.raw == self.original)
            })
    }
    fn read_current(&self, shell: &ShellState) -> bool {
        self.current(shell) && self.owns_draft(shell) && cooldown(shell, &self.key()) <= now()
    }
    fn owns_draft(&self, shell: &ShellState) -> bool {
        shell
            .slack_lists
            .reminder
            .draft
            .as_ref()
            .is_some_and(|d| d.origin.serial == self.serial)
    }
}
fn key(shell: &ShellState) -> Option<Key> {
    let scope = shell
        .slack_lists
        .scope
        .as_ref()
        .filter(|s| s.current(shell))?;
    Some((
        scope.stable_owner(),
        shell.slack_lists.view.as_ref()?.id.clone(),
    ))
}
pub(super) fn blocks_target(shell: &ShellState, scope: &Scope, list: &str) -> bool {
    let key = (scope.stable_owner(), list.to_owned());
    shell.slack_lists.reminder.receipts.contains_key(&key)
        || shell
            .slack_lists
            .reminder
            .draft
            .as_ref()
            .is_some_and(|d| d.origin.key() == key)
        || shell.slack_lists.reminder.loads.values().any(|k| k == &key)
        || shell
            .slack_lists
            .reminder
            .preflights
            .values()
            .any(|k| k == &key)
}
/// Preserve in-process receipts and pending read ownership across shell replacement.
/// Drafts and credential-bearing contexts do not migrate. Late callbacks retain
/// their serial/lease and may settle only the original receipt, never dispatch.
pub(crate) fn carry_pending(previous: &mut ShellState, loaded: &mut ShellState) {
    let old = &mut previous.slack_lists.reminder;
    let new = &mut loaded.slack_lists.reminder;
    assert!(
        old.receipts
            .keys()
            .all(|key| !new.receipts.contains_key(key))
            && old.loads.keys().all(|key| !new.loads.contains_key(key))
            && old
                .preflights
                .keys()
                .all(|key| !new.preflights.contains_key(key)),
        "duplicate reminder receipt or lease"
    );
    for (key, mut receipt) in std::mem::take(&mut old.receipts) {
        receipt.attempt.origin.scope = receipt.attempt.origin.scope.receipt_only();
        assert!(
            new.receipts.insert(key, receipt).is_none(),
            "duplicate reminder receipt"
        );
    }
    for (key, until) in std::mem::take(&mut old.cooldowns) {
        new.cooldowns
            .entry(key)
            .and_modify(|v| *v = (*v).max(until))
            .or_insert(until);
    }
    new.loads.append(&mut old.loads);
    new.preflights.append(&mut old.preflights);
    old.draft = None;
}
pub(crate) fn blocks_mutation(shell: &ShellState) -> bool {
    key(shell).is_some_and(|key| {
        shell.slack_lists.reminder.receipts.contains_key(&key)
            || shell
                .slack_lists
                .reminder
                .draft
                .as_ref()
                .is_some_and(|d| d.origin.key() == key)
    })
}
pub(super) fn blocks_read(shell: &ShellState) -> bool {
    key(shell)
        .and_then(|k| shell.slack_lists.reminder.receipts.get(&k))
        .is_some_and(|r| r.phase != Phase::Uncertain)
}
pub(crate) fn available(shell: &ShellState) -> bool {
    shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .and_then(|l| l.list_experiments.as_ref())
        .is_some_and(|e| e.lists_in_later)
}
pub(crate) fn reason(shell: &ShellState) -> Option<&'static str> {
    let Some(view) = shell.slack_lists.view.as_ref() else {
        return Some("Open an item first.");
    };
    if !available(shell) {
        return Some("Reminders are not enabled for this account.");
    }
    if key(shell)
        .and_then(|k| shell.slack_lists.reminder.receipts.get(&k))
        .is_some()
    {
        // Reopening the retained receipt does not authorize another write.
        return None;
    }
    if view.stale
        || view.loading
        || view.unavailable
        || view.retry_at > now()
        || view.editor.is_some()
        || shell.slack_lists.operations.contains_key(&view.id)
        || rename::blocks_mutation(shell)
        || create::blocks_mutation(shell)
        || delete::blocks_mutation(shell)
        || completion::blocks_mutation(shell)
        || editor::assignment::blocks_mutation(shell)
        || blocks_mutation(shell)
    {
        return Some("Finish the current change or refresh this item first.");
    }
    if key(shell).is_some_and(|key| cooldown(shell, &key) > now()) {
        return Some("Slack asked us to wait before loading this reminder again.");
    }
    if capacity_used(shell) >= 32
        || key(shell).is_some_and(|k| {
            shell
                .slack_lists
                .reminder
                .receipts
                .keys()
                .filter(|(owner, _)| owner == &k.0)
                .count()
                >= 8
        })
    {
        return Some("Check earlier reminders before changing another one.");
    }
    None
}
pub(crate) fn open(mut state: Signal<ShellState>, detail: u64) {
    let origin = begin(&mut state.write(), detail);
    if let Some(origin) = origin {
        network::load_draft(state, origin);
    }
}
fn begin(shell: &mut ShellState, detail: u64) -> Option<Origin> {
    expire(shell, now());
    if reason(shell).is_some() {
        return None;
    }
    let key = key(shell)?;
    if let Some(receipt) = shell.slack_lists.reminder.receipts.get(&key) {
        let mut origin = receipt.attempt.origin.clone();
        origin.visit = shell.slack_lists.view.as_ref()?.generation;
        origin.scope = shell.slack_lists.scope.clone()?;
        shell.slack_lists.reminder.draft = Some(Draft {
            origin,
            loaded: None,
            saved: None,
            loading: false,
            error: None,
            retry_at: 0,
        });
        return None;
    }
    let view = shell.slack_lists.view.as_ref()?;
    let (open, row, title) = record_detail::delete_target(view, detail)?;
    let loaded = view.loaded.as_ref()?;
    let origin = Origin {
        scope: shell.slack_lists.scope.clone()?,
        serial: next_id(),
        visit: view.generation,
        revision: view.revision,
        selection: view.layout.selection_generation,
        detail,
        open,
        list: view.id.clone(),
        original: loaded.records.record(&row)?.raw.clone(),
        row,
        title,
    };
    if !origin.current(shell) {
        return None;
    }
    shell
        .slack_lists
        .reminder
        .loads
        .insert(origin.serial, origin.key());
    shell.slack_lists.reminder.draft = Some(Draft {
        origin: origin.clone(),
        loaded: None,
        saved: None,
        loading: true,
        error: None,
        retry_at: 0,
    });
    Some(origin)
}
pub(crate) fn close(mut state: Signal<ShellState>, serial: u64) {
    let shell = &mut state.write();
    if shell
        .slack_lists
        .reminder
        .draft
        .as_ref()
        .is_some_and(|d| d.origin.serial == serial)
    {
        close_visit(shell);
    }
}
pub(super) fn close_visit(shell: &mut ShellState) {
    if let Some(draft) = shell.slack_lists.reminder.draft.take() {
        let key = draft.origin.key();
        if shell
            .slack_lists
            .reminder
            .receipts
            .get(&key)
            .is_some_and(|r| {
                r.attempt.origin.serial == draft.origin.serial && r.phase == Phase::Preparing
            })
        {
            shell.slack_lists.reminder.receipts.remove(&key);
        }
    }
}
pub(crate) fn save(mut state: Signal<ShellState>, serial: u64, change: ListReminderChange) {
    let attempt = begin_save(&mut state.write(), serial, change);
    if let Some(attempt) = attempt {
        network::dispatch(state, attempt);
    }
}
pub(crate) fn check(mut state: Signal<ShellState>, serial: u64) {
    let read = begin_check(&mut state.write(), serial);
    if let Some(read) = read {
        network::check(state, read);
    }
}
pub(crate) fn presentation(shell: &ShellState) -> Option<Presentation> {
    let draft = shell.slack_lists.reminder.draft.as_ref()?;
    if !draft.origin.visible(shell) {
        return None;
    }
    let receipt = shell.slack_lists.reminder.receipts.get(&draft.origin.key());
    let editable = receipt.is_none()
        && !draft.loading
        && draft.loaded.is_some()
        && draft.retry_at <= now()
        && cooldown(shell, &draft.origin.key()) <= now()
        && draft.origin.current(shell);
    let can_clear = editable
        && draft.loaded.as_ref().is_some_and(|l| {
            ListReminderEdit::capture(
                &l.document,
                &l.records,
                &draft.origin.row,
                available_loaded(l),
                draft.saved.as_ref(),
                ListReminderChange::Clear,
            )
            .is_ok()
        });
    Some(Presentation {generation:draft.origin.serial,title:draft.origin.title.clone(),
        due:draft.saved.as_ref().map(|s|if s.todo_state=="to_do"&&s.date_due==s.date_created{0}else{s.date_due}).unwrap_or(0),editable,ready:draft.loaded.is_some(),loading:draft.loading,
        can_clear,can_check:receipt.is_some_and(|r|r.phase==Phase::Uncertain&&r.checking.is_none()&&r.retry_at<=now()&&cooldown(shell,&draft.origin.key())<=now()),
        pending:receipt.is_some_and(|r|r.phase!=Phase::Uncertain||r.checking.is_some()),
        message:receipt.map(|r|if r.retry_at>now(){"Slack asked us to wait before checking again. This change will not be sent again.".into()}else{r.message.clone()})
            .or_else(||draft.error.clone()).or_else(||(!draft.origin.current(shell)).then(||"This item changed. Close and reopen it before setting a reminder.".into()))})
}
fn available_loaded(loaded: &Loaded) -> bool {
    loaded
        .list_experiments
        .as_ref()
        .is_some_and(|e| e.lists_in_later)
}
pub(super) fn poll(shell: &mut ShellState) -> bool {
    poll_at(shell, now())
}
fn poll_at(shell: &mut ShellState, time: i64) -> bool {
    expire(shell, time);
    let waiting = key(shell).is_some_and(|key| {
        cooldown(shell, &key) > time
            || shell
                .slack_lists
                .reminder
                .receipts
                .get(&key)
                .is_some_and(|r| r.retry_at > time)
    });
    let changed = waiting != shell.slack_lists.reminder.cooldown_visible;
    shell.slack_lists.reminder.cooldown_visible = waiting;
    changed
}
fn expire(shell: &mut ShellState, time: i64) {
    shell
        .slack_lists
        .reminder
        .cooldowns
        .retain(|_, deadline| *deadline > time);
}
fn cooldown(shell: &ShellState, key: &Key) -> i64 {
    shell
        .slack_lists
        .reminder
        .cooldowns
        .get(key)
        .copied()
        .unwrap_or(0)
}
fn retain_cooldown(shell: &mut ShellState, key: Key, error: &Error) {
    let deadline = retry_at(error);
    if deadline > now() {
        let slot = shell.slack_lists.reminder.cooldowns.entry(key).or_default();
        *slot = (*slot).max(deadline);
    }
}
fn capacity_used(shell: &ShellState) -> usize {
    let s = &shell.slack_lists.reminder;
    s.receipts.len() + s.loads.len() + s.preflights.len() + s.cooldowns.len()
}
