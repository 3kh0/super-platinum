//! Client-owned View logs never reaches ordinary blocks.actions dispatch.
use super::*;
use crate::app_home::about::management::entry;
use super_platinum_core::block_actions::BlockConfirmationState;
use super_platinum_core::block_actions::validation::{CapturedWorkflowLogs, WORKFLOW_LOGS_ACTION};
mod native;
#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(super) struct Pending {
    source: Source,
    action: CapturedWorkflowLogs,
    metadata: BlockElementMetadata,
    revision: u64,
}
pub(crate) fn recognizes(metadata: &BlockElementMetadata) -> bool {
    metadata.action_id.as_deref() == Some(WORKFLOW_LOGS_ACTION)
}
fn capture(s: &ShellState, source: &Source, metadata: &BlockElementMetadata) -> Option<Pending> {
    if !recognizes(metadata) || metadata.source != BlockElementSource::Button {
        return None;
    }
    let raw = source.client_message(s).ok()?;
    let action = CapturedWorkflowLogs::capture(
        source.owner().clone(),
        raw,
        metadata.block_id.as_deref()?,
        WORKFLOW_LOGS_ACTION,
    )
    .ok()?;
    if crate::blocks::action::metadata(
        action.button(),
        metadata.block_id.as_deref(),
        BlockElementSource::Button,
    ) != *metadata
    {
        return None;
    }
    Some(Pending {
        source: source.clone(),
        action,
        metadata: metadata.clone(),
        revision: 0,
    })
}
impl Pending {
    fn current(&self, s: &ShellState) -> bool {
        self.source
            .client_message(s)
            .is_ok_and(|message| self.action.revalidate(self.source.owner(), message).is_ok())
    }
}
pub(super) fn presentation(
    s: &ShellState,
    source: &Source,
    metadata: &BlockElementMetadata,
) -> Option<ButtonState> {
    recognizes(metadata).then(|| {
        if capture(s, source, metadata).is_some() {
            ButtonState {
                enabled: true,
                busy: false,
                uncertain: false,
                reason: Some("View workflow activity".into()),
            }
        } else {
            ButtonState::unavailable("This workflow action is unavailable or has changed")
        }
    })
}
pub(super) fn click(
    mut state: Signal<ShellState>,
    source: &Source,
    metadata: &BlockElementMetadata,
) -> bool {
    if !recognizes(metadata) {
        return false;
    }
    let pending = {
        let s = state.peek();
        if s.overlay.is_some() {
            return true;
        }
        capture(&s, source, metadata)
    };
    let Some(mut pending) = pending else {
        return true;
    };
    if matches!(pending.metadata.confirm, BlockConfirmationState::Parsed(_)) {
        pending.revision = next_id();
        let mut s = state.write();
        if pending.current(&s) && s.overlay.is_none() {
            s.message_buttons.workflow_logs = Some(pending);
            s.overlay = Some(Overlay::ButtonConfirmation);
        }
    } else {
        open(state, pending);
    }
    true
}
pub(super) fn confirmation(s: &ShellState) -> Option<Confirmation> {
    let pending = s.message_buttons.workflow_logs.as_ref()?;
    let BlockConfirmationState::Parsed(confirm) = &pending.metadata.confirm else {
        return None;
    };
    Some(Confirmation {
        title: confirm.title.clone()?,
        text: confirm.text.clone()?,
        confirm_label: confirm.confirm.clone()?,
        cancel_label: confirm.deny.clone()?,
        style: confirm.style.clone(),
        revision: pending.revision,
        busy: !pending.current(s),
    })
}
pub(super) fn confirm(mut state: Signal<ShellState>) -> bool {
    if state.peek().message_buttons.workflow_logs.is_none() {
        return false;
    }
    let pending = {
        let mut s = state.write();
        if s.overlay != Some(Overlay::ButtonConfirmation) {
            return true;
        }
        s.overlay = None;
        s.message_buttons.workflow_logs.take()
    };
    if let Some(pending) = pending.filter(|p| p.current(&state.peek())) {
        open(state, pending);
    }
    true
}
fn open(state: Signal<ShellState>, pending: Pending) {
    dioxus::core::spawn_forever(async move {
        let key = pending.source.client_key(&pending.metadata);
        let Some(release) = native::lock(&key).await else {
            return;
        };
        if pending.current(&state.peek()) && state.peek().overlay.is_none() {
            entry::open(
                state,
                pending.source,
                pending.action.workflow_id().into(),
                pending.action.execution_id().map(str::to_owned),
            );
        }
        release.release();
    });
}
