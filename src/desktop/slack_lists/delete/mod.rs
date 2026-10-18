//! Explicit item-and-subtasks deletion. Receipts outlive the rendered visit;
//! only an identity-bound records.info tombstone settles a potentially sent write.
use super::*;
use std::sync::Arc;
use super_platinum_core::{
    slack::transport::ListDeleteFailure,
    slack_lists::{
        AcceptedDeletion, AcceptedDeletionState, DeletionIntent, DeletionStateIntent,
        ListRecordReadback,
    },
};
pub(super) mod fixture;
mod lifecycle;
mod network;
mod undo;
pub(crate) use undo::{dismiss_undo, undo_delete};
#[cfg(test)]
mod tests;
use lifecycle::{begin_check, finish, finish_check, rejected, validate};

type Owner = (Option<String>, String, String);
type Key = (Owner, String);
const MAX_PER_OWNER: usize = 8;
const MAX_TOTAL: usize = 32;

#[derive(Clone, Default)]
pub(crate) struct State {
    fixture: fixture::Fixture,
    receipts: BTreeMap<Key, Receipt>,
    confirmation: Option<Confirmation>,
    notice: Option<(Key, u64, String)>,
    undo: BTreeMap<Key, undo::Capability>,
}
#[derive(Clone)]
struct Confirmation {
    attempt: Attempt,
    title: String,
}
#[derive(Clone)]
pub(super) struct Attempt {
    pub scope: Scope,
    pub serial: u64,
    pub visit: u64,
    pub revision: u64,
    pub detail_generation: u64,
    pub detail_open: u64,
    pub intent: Arc<DeletionIntent>,
    pub restoring: bool,
    pub redo: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    Preparing,
    Sending,
    AwaitingRead,
    Uncertain,
}
#[derive(Clone)]
struct Receipt {
    attempt: Attempt,
    stage: Stage,
    retry_at: i64,
    read: Option<ReadbackAttempt>,
    message: Option<String>,
    response_seen: bool,
    restore_intent: Option<Arc<DeletionStateIntent>>,
}
#[derive(Clone)]
pub(super) struct ReadbackAttempt {
    key: Key,
    pub serial: u64,
    pub read: u64,
    pub scope: Scope,
    pub list_id: String,
    pub record_id: String,
    visit: u64,
}
pub(crate) struct Presentation {
    pub enabled: bool,
    pub reason: Option<String>,
    pub confirmation: Option<ConfirmationView>,
    pub operation: Option<OperationView>,
    pub notice: Option<String>,
    pub undo: Option<u64>,
    pub redo: bool,
}
pub(crate) struct ConfirmationView {
    pub generation: u64,
    pub title: String,
    pub can_confirm: bool,
    pub reason: Option<String>,
}
pub(crate) struct OperationView {
    pub serial: u64,
    pub stage: Stage,
    pub pending: bool,
    pub checking: bool,
    pub can_cancel: bool,
    pub can_check: bool,
    pub message: String,
}
fn key(shell: &ShellState) -> Option<Key> {
    let scope = shell
        .slack_lists
        .scope
        .as_ref()
        .filter(|scope| scope.current(shell))?;
    Some((
        scope.stable_owner(),
        shell.slack_lists.view.as_ref()?.id.clone(),
    ))
}
fn receipt(shell: &ShellState) -> Option<&Receipt> {
    shell.slack_lists.delete.receipts.get(&key(shell)?)
}
pub(super) fn blocks_target(shell: &ShellState, scope: &Scope, list: &str) -> bool {
    let key = (scope.stable_owner(), list.to_owned());
    shell.slack_lists.delete.receipts.contains_key(&key)
        || shell
            .slack_lists
            .delete
            .confirmation
            .as_ref()
            .is_some_and(|c| c.attempt.key() == key)
}
pub(crate) fn blocks_mutation(shell: &ShellState) -> bool {
    receipt(shell).is_some()
        || shell
            .slack_lists
            .delete
            .confirmation
            .as_ref()
            .is_some_and(|confirmation| Some(confirmation.attempt.key()) == key(shell))
}
pub(super) fn blocks_read(shell: &ShellState) -> bool {
    receipt(shell).is_some_and(|receipt| matches!(receipt.stage, Stage::Preparing | Stage::Sending))
}
fn unavailable(shell: &ShellState) -> Option<&'static str> {
    let view = shell.slack_lists.view.as_ref()?;
    if !checkbox_visit_current(shell, view.generation) || view.unavailable {
        return Some("This list is no longer available.");
    }
    if view.loading || view.stale || view.retry_at > now() {
        return Some("Refresh this list before deleting an item.");
    }
    if view.editor.is_some()
        || shell.slack_lists.operations.contains_key(&view.id)
        || create::blocks_mutation(shell)
        || rename::blocks_mutation(shell)
        || reminder::blocks_mutation(shell)
        || completion::blocks_mutation(shell)
        || editor::assignment::blocks_mutation(shell)
        || blocks_mutation(shell)
    {
        return Some("Finish the current change before deleting an item.");
    }
    let Some(detail) = record_detail::presentation(view) else {
        return Some("Open the item details to delete it.");
    };
    let Some((_, record, _)) = record_detail::delete_target(view, detail.generation) else {
        return Some("This item changed. Reopen its details.");
    };
    let Some(loaded) = &view.loaded else {
        return Some("Wait for the list to load.");
    };
    if let Err(error) = DeletionIntent::eligible(
        &loaded.document,
        &loaded.records,
        &record,
        loaded.allow_lists.as_deref(),
        loaded.personal_task_lists.as_ref(),
    ) {
        return Some(error.0);
    }
    let owner = shell.slack_lists.scope.as_ref()?.stable_owner();
    if shell.slack_lists.delete.receipts.len() >= MAX_TOTAL
        || shell
            .slack_lists
            .delete
            .receipts
            .keys()
            .filter(|(key, _)| key == &owner)
            .count()
            >= MAX_PER_OWNER
    {
        return Some("Check earlier item deletions before deleting another item.");
    }
    None
}
pub(crate) fn presentation(shell: &ShellState) -> Presentation {
    let reason = if shell.slack_lists.view.is_none() {
        Some("Open an item to delete it.")
    } else {
        unavailable(shell)
    }
    .map(str::to_owned);
    let confirmation = shell
        .slack_lists
        .delete
        .confirmation
        .as_ref()
        .filter(|confirm| Some(confirm.attempt.key()) == key(shell))
        .map(|confirm| {
            let current = confirm.attempt.visible(shell);
            ConfirmationView {
                generation: confirm.attempt.serial,
                title: confirm.title.clone(),
                can_confirm: current,
                reason: (!current)
                    .then(|| "This item changed. Cancel and review it before deleting.".into()),
            }
        });
    let operation = receipt(shell).map(|receipt| OperationView {
        serial:receipt.attempt.serial,stage:receipt.stage,
        pending:matches!(receipt.stage, Stage::Preparing | Stage::Sending),checking:receipt.read.is_some(),
        can_cancel:receipt.stage == Stage::Preparing,
        can_check:matches!(receipt.stage,Stage::AwaitingRead | Stage::Uncertain) && receipt.read.is_none() && receipt.retry_at <= now(),
        message:if receipt.read.is_none() && receipt.retry_at > now() {
            "Slack asked us to wait before checking again. Check result will become available automatically; the change will not be sent again.".into()
        } else { receipt.message.clone().unwrap_or_else(|| if receipt.attempt.restoring && !receipt.attempt.redo { match receipt.stage {
            Stage::Preparing=>"Checking access before restoring this item…",
            Stage::Sending=>"Restoring item…",
            Stage::AwaitingRead=>"Slack accepted Undo. Checking that the item is restored…",
            Stage::Uncertain=>"Restore is not confirmed. Check the result before changing this list. Undo will not be sent again.",
        }.into() } else { match receipt.stage {
            Stage::Preparing=>"Checking this item and your access…",
            Stage::Sending=>"Deleting item…",
            Stage::AwaitingRead=>"Slack accepted the deletion. Checking that the item is deleted…",
            Stage::Uncertain=>"Deletion is not confirmed. Check the result before changing this list. The deletion will not be sent again.",
        }.into() }) },
    });
    let notice = shell
        .slack_lists
        .delete
        .notice
        .as_ref()
        .filter(|(owner, visit, _)| {
            Some(owner.clone()) == key(shell)
                && shell
                    .slack_lists
                    .view
                    .as_ref()
                    .is_some_and(|view| view.generation == *visit)
        })
        .map(|(_, _, message)| message.clone());
    Presentation {
        enabled: reason.is_none(),
        reason,
        confirmation,
        operation,
        notice,
        undo: undo::available(shell),
        redo: key(shell)
            .and_then(|key| shell.slack_lists.delete.undo.get(&key))
            .is_some_and(|cap| cap.deleted),
    }
}
pub(crate) fn begin_delete(mut state: Signal<ShellState>, detail_generation: u64) {
    if !begin_callback_current(&state.peek(), detail_generation) {
        return;
    }
    let result = begin_shell(&mut state.write(), detail_generation);
    if let Ok(serial) = result {
        let immediate = state
            .peek()
            .slack_lists
            .delete
            .confirmation
            .as_ref()
            .is_some_and(|confirm| confirm.attempt.intent.can_skip_delete_confirmation());
        if immediate {
            confirm_delete(state, serial);
        }
        return;
    }
    let report_current = begin_callback_current(&state.peek(), detail_generation);
    if let Err(error) = result
        && report_current
    {
        report(&mut state.write(), error);
    }
}
fn begin_callback_current(shell: &ShellState, detail_generation: u64) -> bool {
    shell.overlay == Some(Overlay::SlackList)
        && shell
            .slack_lists
            .scope
            .as_ref()
            .is_some_and(|scope| scope.current(shell))
        && shell
            .slack_lists
            .view
            .as_ref()
            .is_some_and(|view| record_detail::delete_target(view, detail_generation).is_some())
}
fn confirm_callback_current(shell: &ShellState, generation: u64) -> bool {
    shell
        .slack_lists
        .delete
        .confirmation
        .as_ref()
        .is_some_and(|confirmation| {
            confirmation.attempt.serial == generation && confirmation.attempt.visible(shell)
        })
}
pub(super) fn begin_shell(
    shell: &mut ShellState,
    detail_generation: u64,
) -> Result<u64, &'static str> {
    poll(shell);
    if let Some(reason) = unavailable(shell) {
        return Err(reason);
    }
    let view = shell
        .slack_lists
        .view
        .as_ref()
        .ok_or("Open this item again.")?;
    let (detail_open, record, title) = record_detail::delete_target(view, detail_generation)
        .ok_or("This item changed. Open it again.")?;
    let loaded = view.loaded.as_ref().ok_or("Wait for this list to load.")?;
    let intent = DeletionIntent::capture(
        &loaded.document,
        &loaded.records,
        &record,
        loaded.allow_lists.as_deref(),
        loaded.personal_task_lists.as_ref(),
    )
    .map_err(|e| e.0)?;
    let attempt = Attempt {
        scope: shell
            .slack_lists
            .scope
            .clone()
            .ok_or("Open this list again.")?,
        serial: next_id(),
        visit: view.generation,
        revision: view.revision,
        detail_generation,
        detail_open,
        intent: Arc::new(intent),
        restoring: false,
        redo: false,
    };
    let serial = attempt.serial;
    shell.slack_lists.delete.confirmation = Some(Confirmation { attempt, title });
    shell.slack_lists.delete.notice = None;
    Ok(serial)
}
pub(crate) fn confirm_delete(mut state: Signal<ShellState>, generation: u64) {
    if !confirm_callback_current(&state.peek(), generation) {
        return;
    }
    let result = confirm_shell(&mut state.write(), generation);
    let report_current = confirm_callback_current(&state.peek(), generation);
    match result {
        Ok(attempt) => network::dispatch(state, attempt),
        Err(error) if report_current => report(&mut state.write(), error),
        Err(_) => {}
    }
}
pub(super) fn confirm_shell(
    shell: &mut ShellState,
    generation: u64,
) -> Result<Attempt, &'static str> {
    let confirmation = shell
        .slack_lists
        .delete
        .confirmation
        .as_ref()
        .filter(|confirm| confirm.attempt.serial == generation)
        .ok_or("This confirmation is no longer current.")?;
    let attempt = confirmation.attempt.clone();
    if !attempt.visible(shell)
        || shell
            .slack_lists
            .delete
            .receipts
            .contains_key(&attempt.key())
        || create::blocks_mutation(shell)
        || rename::blocks_mutation(shell)
        || reminder::blocks_mutation(shell)
        || completion::blocks_mutation(shell)
        || editor::assignment::blocks_mutation(shell)
        || shell
            .slack_lists
            .operations
            .contains_key(attempt.intent.list_id())
    {
        return Err("This item changed. Cancel and review it before deleting.");
    }
    shell.slack_lists.delete.confirmation = None;
    shell.slack_lists.delete.receipts.insert(
        attempt.key(),
        Receipt {
            attempt: attempt.clone(),
            stage: Stage::Preparing,
            retry_at: 0,
            read: None,
            message: None,
            response_seen: false,
            restore_intent: None,
        },
    );
    Ok(attempt)
}
pub(crate) fn cancel_confirmation(mut state: Signal<ShellState>, generation: u64) {
    cancel_confirmation_shell(&mut state.write(), generation);
}
pub(super) fn cancel_confirmation_shell(shell: &mut ShellState, generation: u64) -> bool {
    if shell
        .slack_lists
        .delete
        .confirmation
        .as_ref()
        .is_none_or(|confirm| {
            confirm.attempt.serial != generation || Some(confirm.attempt.key()) != key(shell)
        })
    {
        return false;
    }
    shell.slack_lists.delete.confirmation = None;
    true
}
pub(crate) fn cancel_preparing(mut state: Signal<ShellState>, serial: u64) {
    cancel_preparing_shell(&mut state.write(), serial);
}
pub(super) fn cancel_preparing_shell(shell: &mut ShellState, serial: u64) -> bool {
    let Some(key) = key(shell) else {
        return false;
    };
    if receipt(shell)
        .is_none_or(|receipt| receipt.attempt.serial != serial || receipt.stage != Stage::Preparing)
    {
        return false;
    }
    shell.slack_lists.delete.receipts.remove(&key);
    true
}
pub(crate) fn check_result(mut state: Signal<ShellState>, serial: u64) {
    let attempt = begin_check(&mut state.write(), serial);
    if let Some(attempt) = attempt {
        network::check(state, attempt);
    }
}
fn report(shell: &mut ShellState, message: &str) {
    if let Some(view) = shell.slack_lists.view.as_mut() {
        view.error = Some(message.into());
    }
}
impl Attempt {
    fn key(&self) -> Key {
        (self.scope.stable_owner(), self.intent.list_id().into())
    }
    fn owned(&self, shell: &ShellState) -> bool {
        shell
            .slack_lists
            .delete
            .receipts
            .get(&self.key())
            .is_some_and(|receipt| receipt.attempt.serial == self.serial)
    }
    fn visible(&self, shell: &ShellState) -> bool {
        self.scope.current(shell)
            && shell.overlay == Some(Overlay::SlackList)
            && !self.scope.deleted(shell, self.intent.list_id())
            && shell.slack_lists.view.as_ref().is_some_and(|view| {
                view.id == self.intent.list_id()
                    && view.generation == self.visit
                    && view.revision == self.revision
                    && !view.stale
                    && !view.loading
                    && !view.unavailable
                    && view.retry_at <= now()
                    && view.editor.is_none()
                    && (self.restoring
                        || record_detail::delete_target(view, self.detail_generation).is_some_and(
                            |(open, record, _)| {
                                open == self.detail_open && record == self.intent.record_id()
                            },
                        ))
            })
    }
}
pub(crate) fn close_visit(shell: &mut ShellState) {
    shell.slack_lists.delete.confirmation = None;
    let visit = shell.slack_lists.view.as_ref().map(|view| view.generation);
    shell.slack_lists.delete.receipts.retain(|_, receipt| {
        receipt.stage != Stage::Preparing || Some(receipt.attempt.visit) != visit
    });
}
pub(crate) fn poll(shell: &mut ShellState) -> bool {
    let mut changed = false;
    let timestamp = now();
    for receipt in shell.slack_lists.delete.receipts.values_mut() {
        if receipt.retry_at > 0 && receipt.retry_at <= timestamp {
            receipt.retry_at = 0;
            // The runtime uses silent timer writes; explicitly notify once so
            // Check result re-enables without unrelated activity or a retry.
            changed = true;
        }
    }
    if shell
        .slack_lists
        .delete
        .confirmation
        .as_ref()
        .is_some_and(|confirm| !confirm.attempt.visible(shell))
    {
        shell.slack_lists.delete.confirmation = None;
        changed = true;
    }
    let stale = shell
        .slack_lists
        .delete
        .receipts
        .iter()
        .filter(|(_, receipt)| receipt.stage == Stage::Preparing && !receipt.attempt.visible(shell))
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    for key in stale {
        shell.slack_lists.delete.receipts.remove(&key);
        changed = true;
    }
    let disconnected = shell
        .slack_lists
        .delete
        .receipts
        .iter()
        .filter(|(_, receipt)| {
            receipt.stage == Stage::Sending && !receipt.attempt.scope.account_current(shell)
        })
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    for key in disconnected {
        shell
            .slack_lists
            .delete
            .receipts
            .get_mut(&key)
            .unwrap()
            .stage = Stage::Uncertain;
        changed = true;
    }
    let stale_reads = shell
        .slack_lists
        .delete
        .receipts
        .iter()
        .filter(|(_, receipt)| {
            receipt
                .read
                .as_ref()
                .is_some_and(|read| !read.current(shell))
        })
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    for key in stale_reads {
        let receipt = shell.slack_lists.delete.receipts.get_mut(&key).unwrap();
        receipt.read = None;
        receipt.message=Some("The previous check stopped. Check the result again; the change will not be sent again.".into());
        changed = true;
    }
    changed
}
