//! Ordinary message app buttons: exact source, explicit confirmation, one send.
use crate::{
    scheduled::Context,
    state::{Overlay, ShellState},
};
use dioxus::prelude::*;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use super_platinum_core::{
    block_actions::{BlockElementMetadata, BlockElementSource, BlockText},
    slack::{Error, api},
};

pub(crate) mod check;
pub(crate) mod fixture;
pub(crate) mod hydration;
pub(crate) mod inputs;
pub(crate) mod radio_check;
pub(crate) mod radio_fixture;
pub(crate) mod select_check;
pub(crate) mod select_fixture;
pub(crate) mod service;
pub(crate) mod source;
use source::{Captured, Source};
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(crate) struct State {
    pub(crate) hydration_revision: u64,
    pub(crate) fixture: Option<fixture::Fixture>,
    pub(crate) select_fixture: Option<select_fixture::Fixture>,
    pub(crate) radio_fixture: Option<radio_fixture::Fixture>,
    inputs: inputs::Cache,
    confirmation: Option<Pending>,
    operations: BTreeMap<Key, Operation>,
}
struct Pending {
    input_revision: Option<u64>,
    captured: Captured,
    revision: u64,
}
struct Operation {
    captured: Captured,
    phase: Phase,
    generation: u64,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Pending,
    Uncertain,
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    epoch: u64,
    team: String,
    channel: String,
    ts: String,
    block: String,
    action: String,
}
impl Key {
    fn new(source: &Source, metadata: &BlockElementMetadata) -> Option<Self> {
        let owner = source.owner();
        Some(Self {
            epoch: owner.account_epoch,
            team: owner.team_id.clone(),
            channel: owner.channel_id.clone(),
            ts: owner.message_ts.clone(),
            block: metadata.block_id.clone()?,
            action: metadata.action_id.clone()?,
        })
    }
}
fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
const MAX_OPERATIONS: usize = 64;

pub(crate) struct ButtonState {
    pub enabled: bool,
    pub busy: bool,
    pub uncertain: bool,
    pub reason: Option<String>,
}
impl ButtonState {
    fn unavailable(reason: &str) -> Self {
        Self {
            enabled: false,
            busy: false,
            uncertain: false,
            reason: Some(reason.into()),
        }
    }
}
fn capture(
    shell: &ShellState,
    source: &Source,
    metadata: &BlockElementMetadata,
) -> Result<Captured, &'static str> {
    if metadata.source != BlockElementSource::Button {
        return Err("This control is not supported yet");
    }
    let (Some(block), Some(action)) = (metadata.block_id.as_deref(), metadata.action_id.as_deref())
    else {
        return Err("This action is missing its app identity");
    };
    let captured = source
        .capture_button(shell, block, action)
        .map_err(|_| "This app action is unavailable or has changed")?;
    if captured.metadata() != metadata {
        return Err("This app action changed. Open the updated message to continue");
    }
    if captured.app_team_id().is_none() {
        return Err("The app's workspace information is unavailable");
    }
    Ok(captured)
}
pub(crate) fn presentation_for_button(
    shell: &ShellState,
    source: &Source,
    metadata: &BlockElementMetadata,
) -> ButtonState {
    if let Err(reason) = capture(shell, source, metadata) {
        return ButtonState::unavailable(if hydration::pending(shell, source) {
            "Loading app information…"
        } else {
            reason
        });
    }
    operation_status(shell, source, metadata)
}

fn operation_status(
    shell: &ShellState,
    source: &Source,
    metadata: &BlockElementMetadata,
) -> ButtonState {
    if inputs::revision(shell, source).is_some()
        && shell.message_buttons.operations.values().any(|op| {
            op.phase == Phase::Pending && op.captured.source().same_action_revision(source)
        })
    {
        return ButtonState {
            enabled: false,
            busy: true,
            uncertain: false,
            reason: Some("Submitting…".into()),
        };
    }

    if let Some(operation) = Key::new(source, metadata)
        .and_then(|key| shell.message_buttons.operations.get(&key))
        .filter(|op| op.captured.source().same_action_revision(source))
    {
        return ButtonState {
            enabled: false,
            busy: operation.phase == Phase::Pending,
            uncertain: operation.phase == Phase::Uncertain,
            reason: Some(
                if operation.phase == Phase::Pending {
                    "Submitting…"
                } else {
                    "The app has not confirmed this action. It will not be sent again."
                }
                .into(),
            ),
        };
    }
    if shell.message_buttons.operations.len() >= MAX_OPERATIONS
        && Key::new(source, metadata)
            .is_none_or(|key| !shell.message_buttons.operations.contains_key(&key))
    {
        return ButtonState::unavailable("Resolve earlier app actions before trying another");
    }
    ButtonState {
        enabled: true,
        busy: false,
        uncertain: false,
        reason: None,
    }
}

pub(crate) struct Confirmation {
    pub title: BlockText,
    pub text: BlockText,
    pub confirm_label: BlockText,
    pub cancel_label: BlockText,
    pub style: Option<String>,
    pub revision: u64,
    pub busy: bool,
}
pub(crate) fn confirmation(shell: &ShellState) -> Option<Confirmation> {
    let pending = shell.message_buttons.confirmation.as_ref()?;
    let confirm = pending.captured.confirmation()?;
    Some(Confirmation {
        title: confirm.title.clone()?,
        text: confirm.text.clone()?,
        confirm_label: confirm.confirm.clone()?,
        cancel_label: confirm.deny.clone()?,
        style: confirm.style.clone(),
        revision: pending.revision,
        busy: !inputs::pending_current(shell, pending),
    })
}
pub(crate) fn cancel_local(shell: &mut ShellState) {
    if let Some(pending) = shell.message_buttons.confirmation.take() {
        inputs::invalidate(shell, pending.captured.source());
    }
    if shell.overlay == Some(Overlay::ButtonConfirmation) {
        shell.overlay = None;
    }
}
pub(crate) fn cancel(mut state: Signal<ShellState>) {
    cancel_local(&mut state.write());
}
pub(crate) fn click(mut state: Signal<ShellState>, source: Source, metadata: BlockElementMetadata) {
    let captured = {
        let mut shell = state.write();
        if shell.overlay.is_some() {
            return;
        }
        if !presentation_for_button(&shell, &source, &metadata).enabled {
            return;
        }
        let Ok(captured) = capture(&shell, &source, &metadata) else {
            return;
        };
        if captured.confirmation().is_some() {
            shell.message_buttons.confirmation = Some(Pending {
                captured,
                revision: next_id(),
                input_revision: None,
            });
            shell.overlay = Some(Overlay::ButtonConfirmation);
            return;
        }
        captured
    };
    dispatch(state, captured);
}
pub(crate) fn confirm(mut state: Signal<ShellState>) {
    let captured = {
        let mut shell = state.write();
        if shell.overlay != Some(Overlay::ButtonConfirmation) {
            return;
        }
        let Some(pending) = shell.message_buttons.confirmation.take() else {
            return;
        };
        shell.overlay = None;
        if !inputs::pending_current(&shell, &pending) {
            shell.show_toast("This app action changed. Open the updated message to continue.");
            return;
        }
        pending.captured
    };
    dispatch(state, captured);
}

fn dispatch(mut state: Signal<ShellState>, captured: Captured) {
    if captured.revalidate(&state.read()).is_err() {
        return;
    }
    if radio_fixture::dispatch(state, captured.clone())
        || select_fixture::dispatch(state, captured.clone())
        || fixture::dispatch(state, captured.clone())
    {
        return;
    }
    let prepared = {
        let mut shell = state.write();
        if shell.overlay.is_some() {
            return;
        }
        if !operation_status(&shell, captured.source(), captured.metadata()).enabled {
            return;
        }
        let Some(context) = Context::capture(&shell) else {
            shell.show_toast("Reconnect to use this app action.");
            return;
        };
        let Some(correlation) = crate::app_modal::register_action(&mut shell, captured.app_id())
        else {
            return;
        };
        let request = match prepare_request(
            &mut shell,
            &captured,
            &context.client,
            &context.session,
            &correlation.token,
        ) {
            Ok(request) => request,
            Err(_) => {
                crate::app_modal::reject_action(&mut shell, &correlation);
                shell.show_toast("This app action could not be prepared.");
                return;
            }
        };
        let key = Key::new(captured.source(), captured.metadata()).expect("validated action IDs");
        let generation = next_id();
        shell.message_buttons.operations.insert(
            key.clone(),
            Operation {
                captured: captured.clone(),
                phase: Phase::Pending,
                generation,
            },
        );
        (context, request, correlation, key, generation)
    };
    let url = captured.url().map(str::to_owned);
    let (context, request, correlation, key, generation) = prepared;
    dioxus::core::spawn_forever(async move {
        // The spawned task can first run after an account replacement or a
        // canonical edit. Check again before the first network side effect.
        if !begin_dispatch(&mut state.write(), &context, &key, generation, &correlation) {
            return;
        }
        let response = context
            .transport
            .execute_block_interaction(request)
            .await
            .and_then(|value| api::decode_block_action_response(&value));
        let mut shell = state.write();
        // Completion bookkeeping survives navigation, but belongs only to the
        // account/transport that sent it. Feedback appears only in its view.
        if context.epoch != shell.core.account_epoch
            || shell
                .core
                .transport
                .as_ref()
                .is_none_or(|t| !Arc::ptr_eq(t, &context.transport))
        {
            return;
        }
        finish(&mut shell, &key, generation, &correlation, response);
    });
    // Captured Slack schedules the validated action before opening a URL; it
    // does not wait for the HTTP acknowledgement. Cancel never reaches here.
    // This user-requested link opens immediately; a later account/source
    // change may cancel the independently scheduled, still-unsent app action.
    if let Some(url) = url {
        // A workspace switch would invalidate the unsent action and its modal
        // correlation. Keep that boundary explicit until actions can retain
        // their workspace independently of the visible shell.
        let allowed = crate::slack_links::button_url_allowed(&state.peek(), &url);
        match allowed {
            Ok(()) => crate::slack_links::open(state, &url),
            Err(reason) => state.write().show_toast(reason),
        }
    }
}

/// A click survives ordinary conversation navigation, but not account/transport
/// replacement, reconnect, or changes to its canonical source before sending.
/// Cancellation retires only this pending generation; a newer attempt and an
/// already uncertain operation must never be mistaken for an unsent request.
fn begin_dispatch(
    shell: &mut ShellState,
    context: &Context,
    key: &Key,
    generation: u64,
    correlation: &crate::app_modal::ActionToken,
) -> bool {
    if shell
        .message_buttons
        .operations
        .get(key)
        .is_some_and(|operation| {
            operation.generation == generation && operation.phase == Phase::Uncertain
        })
    {
        return false;
    }
    let matching = shell
        .message_buttons
        .operations
        .get(key)
        .filter(|operation| {
            operation.generation == generation && operation.phase == Phase::Pending
        });
    let valid = matching.is_some_and(|operation| {
        context.current(shell)
            && crate::app_modal::action_scope_current(shell, correlation)
            && operation.captured.source().revision_current(shell)
    });
    if valid {
        return true;
    }
    if matching.is_some() {
        shell.message_buttons.operations.remove(key);
    }
    crate::app_modal::reject_action(shell, correlation);
    false
}

fn finish(
    shell: &mut ShellState,
    key: &Key,
    generation: u64,
    correlation: &crate::app_modal::ActionToken,
    response: Result<api::BlockActionResponse, Error>,
) {
    let Some(operation) = shell
        .message_buttons
        .operations
        .get(key)
        .filter(|op| op.generation == generation)
    else {
        return;
    };
    let visible = operation.captured.source().current(shell);
    match response {
        Ok(api::BlockActionResponse::Accepted) => {
            shell.message_buttons.operations.remove(key);
        }
        Ok(api::BlockActionResponse::Rejected(error)) => {
            shell.message_buttons.operations.remove(key);
            crate::app_modal::reject_action(shell, correlation);
            if visible {
                shell.show_toast(
                    error
                        .message
                        .unwrap_or_else(|| "The app could not accept this action.".into()),
                );
            }
        }
        Err(Error::Api(code)) if code == "block_interaction_not_sent_offline" => {
            shell.message_buttons.operations.remove(key);
            crate::app_modal::reject_action(shell, correlation);
            if visible {
                shell.show_toast("Reconnect to use this app action. It has not been sent.");
            }
        }
        Err(_) => {
            shell.message_buttons.operations.get_mut(key).unwrap().phase = Phase::Uncertain;
            if visible {
                shell.show_toast(
                    "The app has not confirmed this action. It will not be sent again.",
                );
            }
        }
    }
}
/// Only runs cheap account checks normally; a visible confirmation additionally
/// validates its one source. No periodic full-message/operation scan.
pub(crate) fn poll(shell: &mut ShellState) -> bool {
    inputs::prune(shell);
    shell
        .message_buttons
        .operations
        .retain(|key, _| key.epoch == shell.core.account_epoch);
    if shell.message_buttons.confirmation.is_some() {
        if shell.overlay != Some(Overlay::ButtonConfirmation) {
            shell.message_buttons.confirmation = None;
        } else if shell
            .message_buttons
            .confirmation
            .as_ref()
            .is_some_and(|p| !inputs::pending_current(shell, p))
        {
            cancel_local(shell);
            shell.show_toast("This app action changed. Open the updated message to continue.");
            return true;
        }
    }
    false
}

use inputs::prepare as prepare_request;
