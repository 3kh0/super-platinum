//! Explicit header creation, with receipts independent of the visible List visit.
//! A lost response cannot identify an indistinguishable blank row: never replay.
use super::*;
use super_platinum_core::slack_lists::{CreationIntent, ListClock, ListTimestamp, Record};
pub(crate) mod duplicate;
mod network;
#[cfg(test)]
mod tests;

type Key = ((Option<String>, String, String), String);

#[derive(Clone, Default)]
pub(crate) struct State {
    clock: ListClock,
    receipts: BTreeMap<Key, Receipt>,
    notice: Option<Notice>,
}
#[derive(Clone)]
pub(super) struct Attempt {
    pub scope: Scope,
    pub serial: u64,
    visit: u64,
    revision: u64,
    selection: u64,
    selected: Option<String>,
    origin: Option<duplicate::Origin>,
    pub intent: CreationIntent,
    pub timestamp: ListTimestamp,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Preparing,
    Sending,
    AwaitingRead,
    Uncertain,
}
#[derive(Clone)]
struct Receipt {
    attempt: Attempt,
    stage: Stage,
    response_seen: bool,
    record: Option<Record>,
}
#[derive(Clone)]
struct Notice {
    scope: Scope,
    list: String,
    visit: u64,
    message: String,
    intent: CreationIntent,
    record: Record,
}
pub(crate) struct CreatePresentation {
    pub enabled: bool,
    pub reason: Option<String>,
    pub operation: Option<CreateOperationView>,
    pub notice: Option<String>,
    pub can_open: bool,
}
pub(crate) struct CreateOperationView {
    pub serial: u64,
    pub pending: bool,
    pub can_cancel: bool,
    pub can_retire: bool,
    pub can_dismiss: bool,
    pub message: &'static str,
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
fn receipt(shell: &ShellState) -> Option<&Receipt> {
    let key = key(shell)?;
    shell
        .slack_lists
        .create
        .receipts
        .get(&key)
        .filter(|r| r.attempt.scope.stable_current(shell))
}
pub(crate) fn blocks_mutation(shell: &ShellState) -> bool {
    receipt(shell).is_some()
}
fn unavailable(shell: &ShellState) -> Option<&'static str> {
    unavailable_for(shell, false)
}
fn unavailable_for(shell: &ShellState, duplicate: bool) -> Option<&'static str> {
    let Some(view) = shell.slack_lists.view.as_ref() else {
        return Some("Open a list to add an item.");
    };
    if !checkbox_visit_current(shell, view.generation) || view.unavailable {
        return Some("This list is no longer available.");
    }
    if view.loading || view.stale {
        return Some("Refresh this list before adding an item.");
    }
    if view.retry_at > now() {
        return Some("Wait before adding another item.");
    }
    if view.editor.is_some() || (!duplicate && view.detail.is_some()) {
        return Some("Return to the list before adding an item.");
    }
    if shell.slack_lists.operations.contains_key(&view.id)
        || blocks_mutation(shell)
        || delete::blocks_mutation(shell)
    {
        return Some("Wait for the current change to be confirmed.");
    }
    let Some(loaded) = view.loaded.as_ref() else {
        return Some("Wait for this list to load.");
    };
    CreationIntent::eligible(
        &loaded.document,
        loaded.allow_lists.as_deref(),
        loaded.personal_task_lists.as_ref(),
        view.layout.selected.as_deref(),
    )
    .err()
    .map(|error| error.0)
}
pub(crate) fn presentation(shell: &ShellState) -> CreatePresentation {
    let reason = unavailable(shell).map(str::to_owned);
    let operation=receipt(shell).map(|receipt|CreateOperationView {
        serial:receipt.attempt.serial,
        pending:matches!(receipt.stage,Stage::Preparing|Stage::Sending),
        can_cancel:receipt.stage==Stage::Preparing,
        can_retire:receipt.stage==Stage::Uncertain,
        can_dismiss:receipt.stage==Stage::AwaitingRead,
        message:if receipt.attempt.origin.is_some() { match receipt.stage {
            Stage::Preparing=>"Checking this item before duplicating it...",
            Stage::Sending=>"Duplicating item...",
            Stage::AwaitingRead=>"Item duplicated. Refresh to load its fields.",
            Stage::Uncertain=>"Could not confirm whether the item was duplicated. Check this list in Slack before adding or duplicating another item.",
        }} else { match receipt.stage {
            Stage::Preparing=>"Checking access before adding an item…",
            Stage::Sending=>"Adding item…",
            Stage::AwaitingRead=>"Item added. Refresh to load its fields.",
            Stage::Uncertain=>"Could not confirm whether the item was added. Check this list in Slack before adding another item.",
        }},
    });
    let notice = shell.slack_lists.create.notice.as_ref().filter(|notice| {
        notice_current(&notice.scope, shell)
            && shell
                .slack_lists
                .view
                .as_ref()
                .is_some_and(|view| view.id == notice.list && view.generation == notice.visit)
    });
    let can_open = notice.is_some()
        && shell.slack_lists.view.as_ref().is_some_and(|view| {
            !view.loading
                && !view.stale
                && !view.unavailable
                && view.editor.is_none()
                && view.detail.is_none()
        });
    CreatePresentation {
        enabled: reason.is_none(),
        reason,
        operation,
        notice: notice.map(|notice| notice.message.clone()),
        can_open,
    }
}
pub(crate) fn add_item(mut state: Signal<ShellState>, visit: u64, layout: u64) {
    let result = begin(&mut state.write(), visit, layout);
    match result {
        Ok(attempt) => network::dispatch(state, attempt),
        Err(error) => {
            if let Some(view) = state
                .write()
                .slack_lists
                .view
                .as_mut()
                .filter(|v| v.generation == visit)
            {
                view.error = Some(error.into());
            }
        }
    }
}
pub(super) fn begin(
    shell: &mut ShellState,
    visit: u64,
    layout: u64,
) -> Result<Attempt, &'static str> {
    if !layout::current(shell, visit, layout) {
        return Err("This list changed. Review it before adding an item.");
    }
    if let Some(reason) = unavailable(shell) {
        return Err(reason);
    }
    let view = shell.slack_lists.view.as_ref().unwrap();
    let loaded = view.loaded.as_ref().unwrap();
    let intent = CreationIntent::capture(
        &loaded.document,
        loaded.allow_lists.as_deref(),
        loaded.personal_task_lists.as_ref(),
        view.layout.selected.as_deref(),
    )
    .map_err(|e| e.0)?;
    reserve(shell, intent, None)
}
fn reserve(
    shell: &mut ShellState,
    intent: CreationIntent,
    origin: Option<duplicate::Origin>,
) -> Result<Attempt, &'static str> {
    let view = shell.slack_lists.view.as_ref().unwrap();
    let visit = view.generation;
    let scope = shell
        .slack_lists
        .scope
        .clone()
        .ok_or("Open this list again.")?;
    let milliseconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "The system clock is unavailable.")?
        .as_millis();
    let timestamp = shell
        .slack_lists
        .create
        .clock
        .tick(u64::try_from(milliseconds).map_err(|_| "The system clock is unavailable.")?)
        .map_err(|e| e.0)?;
    let attempt = Attempt {
        scope,
        serial: next_id(),
        visit,
        revision: view.revision,
        selection: view.layout.selection_generation,
        selected: view.layout.selected.clone(),
        origin,
        intent,
        timestamp,
    };
    shell.slack_lists.create.notice = None;
    shell.slack_lists.create.receipts.insert(
        attempt.key(),
        Receipt {
            attempt: attempt.clone(),
            stage: Stage::Preparing,
            response_seen: false,
            record: None,
        },
    );
    layout::protect_selection(shell.slack_lists.view.as_mut().unwrap());
    Ok(attempt)
}
impl Attempt {
    fn key(&self) -> Key {
        (self.scope.stable_owner(), self.intent.list_id().into())
    }
    fn owns(&self, shell: &ShellState) -> bool {
        shell
            .slack_lists
            .create
            .receipts
            .get(&self.key())
            .is_some_and(|receipt| receipt.attempt.serial == self.serial)
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.owns(shell)
            && self.scope.current(shell)
            && shell.overlay == Some(Overlay::SlackList)
            && shell.slack_lists.view.as_ref().is_some_and(|view| {
                view.id == self.intent.list_id()
                    && view.generation == self.visit
                    && view.revision == self.revision
                    && view.layout.selection_generation == self.selection
                    && !view.unavailable
                    && !view.stale
                    && !view.loading
                    && view.retry_at <= now()
                    && view.editor.is_none()
                    && self
                        .origin
                        .as_ref()
                        .map_or_else(|| view.detail.is_none(), |origin| origin.current(view))
            })
            && !self.scope.deleted(shell, self.intent.list_id())
    }
}
pub(super) fn validate(
    shell: &mut ShellState,
    attempt: &Attempt,
    loaded: &Loaded,
) -> Result<(), &'static str> {
    if !attempt.current(shell)
        || delete::blocks_mutation(shell)
        || shell
            .slack_lists
            .operations
            .contains_key(attempt.intent.list_id())
        || shell
            .slack_lists
            .create
            .receipts
            .get(&attempt.key())
            .is_none_or(|r| r.stage != Stage::Preparing)
    {
        return Err("This creation is out of date. Review the list before trying again.");
    }
    attempt
        .intent
        .revalidate_with_records(
            &loaded.document,
            &loaded.records,
            loaded.allow_lists.as_deref(),
            loaded.personal_task_lists.as_ref(),
            attempt.selected.as_deref(),
        )
        .map_err(|e| e.0)?;
    shell
        .slack_lists
        .create
        .receipts
        .get_mut(&attempt.key())
        .unwrap()
        .stage = Stage::Sending;
    Ok(())
}
pub(super) fn rejected(
    shell: &mut ShellState,
    attempt: &Attempt,
    message: &str,
    error: Option<&Error>,
) {
    if !attempt.owns(shell) {
        return;
    }
    shell.slack_lists.create.receipts.remove(&attempt.key());
    if attempt.scope.current(shell)
        && let Some(view) =
            shell.slack_lists.view.as_mut().filter(|view| {
                view.id == attempt.intent.list_id() && view.generation == attempt.visit
            })
    {
        view.error = Some(message.into());
        view.retry_at = error.map(retry_at).unwrap_or(0);
    }
}
pub(super) fn finish(
    shell: &mut ShellState,
    attempt: &Attempt,
    result: Result<Value, Error>,
) -> bool {
    if !attempt.owns(shell)
        || shell
            .slack_lists
            .create
            .receipts
            .get(&attempt.key())
            .is_none_or(|r| {
                r.response_seen || !matches!(r.stage, Stage::Sending | Stage::Uncertain)
            })
    {
        return false;
    }
    let receipt = shell
        .slack_lists
        .create
        .receipts
        .get_mut(&attempt.key())
        .unwrap();
    receipt.response_seen = true;
    receipt.attempt.scope = receipt.attempt.scope.receipt_only();
    let record = result
        .as_ref()
        .ok()
        .and_then(|response| attempt.intent.validate_response(response).ok());
    let uncertain = result.is_ok()
        || result
            .as_ref()
            .err()
            .is_some_and(|error| !matches!(error, Error::Offline(_)) && outcome_uncertain(error));
    if let Some(record) = record {
        let receipt = shell
            .slack_lists
            .create
            .receipts
            .get_mut(&attempt.key())
            .unwrap();
        receipt.stage = Stage::AwaitingRead;
        receipt.record = Some(record);
    } else if uncertain {
        shell
            .slack_lists
            .create
            .receipts
            .get_mut(&attempt.key())
            .unwrap()
            .stage = Stage::Uncertain;
    } else {
        rejected(
            shell,
            attempt,
            "The item was not added. Review the list before trying again.",
            result.as_ref().err(),
        );
        return false;
    }
    let visible = attempt.scope.current(shell)
        && shell
            .slack_lists
            .view
            .as_ref()
            .is_some_and(|v| v.id == attempt.intent.list_id());
    if visible {
        let view = shell.slack_lists.view.as_mut().unwrap();
        view.stale = true;
        view.revision = next_id();
        view.retry_at = result.as_ref().err().map(retry_at).unwrap_or(0);
    }
    visible
}
pub(crate) fn cancel_create(mut state: Signal<ShellState>, serial: u64) {
    cancel(&mut state.write(), serial);
}
pub(crate) fn retire_uncertain(mut state: Signal<ShellState>, serial: u64) {
    retire(&mut state.write(), serial);
}
pub(crate) fn dismiss_confirmed(mut state: Signal<ShellState>, serial: u64) {
    dismiss(&mut state.write(), serial);
}
fn dismiss(shell: &mut ShellState, serial: u64) -> bool {
    let Some(key) = key(shell) else {
        return false;
    };
    if receipt(shell).is_none_or(|receipt| {
        receipt.attempt.serial != serial || receipt.stage != Stage::AwaitingRead
    }) {
        return false;
    }
    shell.slack_lists.create.receipts.remove(&key);
    true
}
fn retire(shell: &mut ShellState, serial: u64) -> bool {
    let Some(key) = key(shell) else {
        return false;
    };
    if receipt(shell)
        .is_none_or(|receipt| receipt.attempt.serial != serial || receipt.stage != Stage::Uncertain)
    {
        return false;
    }
    shell.slack_lists.create.receipts.remove(&key);
    true
}
pub(crate) fn open_result(mut state: Signal<ShellState>, visit: u64) {
    open_result_shell(&mut state.write(), visit);
}
fn open_result_shell(shell: &mut ShellState, visit: u64) -> bool {
    let Some(notice) = shell
        .slack_lists
        .create
        .notice
        .as_ref()
        .filter(|notice| notice.visit == visit && notice_current(&notice.scope, shell))
        .cloned()
    else {
        return false;
    };
    let Some(view) = shell.slack_lists.view.as_ref().filter(|view| {
        view.generation == visit
            && view.id == notice.list
            && !view.stale
            && !view.loading
            && !view.unavailable
    }) else {
        return false;
    };
    let Some(loaded) = view.loaded.as_ref() else {
        return false;
    };
    if notice
        .intent
        .readback_record(&loaded.document, &loaded.records, &notice.record)
        .is_some()
    {
        return record_detail::open_created(shell, &notice.record.id);
    }
    false
}

fn cancel(shell: &mut ShellState, serial: u64) -> bool {
    let Some(key) = key(shell) else {
        return false;
    };
    if receipt(shell)
        .is_none_or(|receipt| receipt.attempt.serial != serial || receipt.stage != Stage::Preparing)
    {
        return false;
    }
    shell.slack_lists.create.receipts.remove(&key);
    true
}
pub(crate) fn close_visit(shell: &mut ShellState) {
    let visit = shell.slack_lists.view.as_ref().map(|view| view.generation);
    shell.slack_lists.create.receipts.retain(|_, receipt| {
        receipt.stage != Stage::Preparing || Some(receipt.attempt.visit) != visit
    });
}
pub(crate) fn poll(shell: &mut ShellState) -> bool {
    let stale = shell
        .slack_lists
        .create
        .receipts
        .iter()
        .filter(|(_, receipt)| receipt.stage == Stage::Preparing && !receipt.attempt.current(shell))
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    let mut changed = !stale.is_empty();
    for key in stale {
        shell.slack_lists.create.receipts.remove(&key);
    }
    let disconnected = shell
        .slack_lists
        .create
        .receipts
        .iter()
        .filter(|(_, receipt)| {
            receipt.stage == Stage::Sending && !receipt.attempt.scope.account_current(shell)
        })
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    for key in disconnected {
        let receipt = shell.slack_lists.create.receipts.get_mut(&key).unwrap();
        receipt.stage = Stage::Uncertain;
        receipt.attempt.scope = receipt.attempt.scope.receipt_only();
        changed = true;
    }
    changed
}
fn notice_current(scope: &Scope, shell: &ShellState) -> bool {
    scope.stable_current(shell)
        && shell.slack_lists.scope.as_ref().is_some_and(|current| {
            current.current(shell) && current.stable_owner() == scope.stable_owner()
        })
}
pub(crate) fn reconcile(shell: &mut ShellState) {
    let Some(receipt) = receipt(shell)
        .cloned()
        .filter(|receipt| receipt.stage == Stage::AwaitingRead)
    else {
        return;
    };
    let Some(expected) = receipt.record.as_ref() else {
        return;
    };
    let Some(view) = shell
        .slack_lists
        .view
        .as_ref()
        .filter(|view| !view.loading && !view.stale && !view.unavailable)
    else {
        return;
    };
    let Some(loaded) = view.loaded.as_ref() else {
        return;
    };
    let Some(record) =
        receipt
            .attempt
            .intent
            .readback_record(&loaded.document, &loaded.records, expected)
    else {
        return;
    };
    let hidden = !view.rows.iter().any(|row| row.id == record.id);
    let open = view.generation == receipt.attempt.visit
        && view.layout.selection_generation == receipt.attempt.selection
        && receipt
            .attempt
            .origin
            .as_ref()
            .map_or_else(|| view.detail.is_none(), |origin| origin.still_open(view))
        && view.editor.is_none();
    let visit = view.generation;
    shell
        .slack_lists
        .create
        .receipts
        .remove(&receipt.attempt.key());
    shell.slack_lists.create.notice = Some(Notice {
        scope: receipt.attempt.scope.clone(),
        list: receipt.attempt.intent.list_id().into(),
        visit,
        message: match (receipt.attempt.origin.is_some(), hidden) {
            (true, true) => "Item duplicated. It is not shown in the current view.",
            (true, false) => "Item duplicated.",
            (false, true) => "Item added. It is not shown in the current view.",
            (false, false) => "Item added.",
        }
        .into(),
        intent: receipt.attempt.intent.clone(),
        record: expected.clone(),
    });
    if open {
        if receipt.attempt.origin.is_some() {
            record_detail::dismiss_shell(shell);
        }
        record_detail::open_created(shell, &record.id);
    }
}

/// Retire only a known created identity after a current, request-associated
/// records.info tombstone. Page absence and a delete ACK alone are insufficient.
pub(super) fn reconcile_deleted(
    shell: &mut ShellState,
    scope: &Scope,
    readback: &super_platinum_core::slack_lists::ListRecordReadback,
) {
    if !scope.current(shell) || !readback.is_deleted() {
        return;
    }
    let owner = scope.stable_owner();
    shell.slack_lists.create.receipts.retain(|_, receipt| {
        !(receipt.stage == Stage::AwaitingRead
            && receipt.attempt.scope.stable_owner() == owner
            && receipt.attempt.intent.list_id() == readback.list_id()
            && receipt
                .record
                .as_ref()
                .is_some_and(|record| record.id == readback.record_id()))
    });
    if shell
        .slack_lists
        .create
        .notice
        .as_ref()
        .is_some_and(|notice| {
            notice.scope.stable_owner() == owner
                && notice.list == readback.list_id()
                && notice.record.id == readback.record_id()
        })
    {
        shell.slack_lists.create.notice = None;
    }
}
