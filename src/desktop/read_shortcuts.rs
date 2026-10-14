//! Explicit reading keyboard commands. Automatic Reading modes remain separate.
use crate::state::ShellState;
use dioxus::prelude::*;
use eligibility::{Captured, ComposerPolicy, Editor, Focus, Intent, KeyEvent, Snapshot};
use serde::Deserialize;

pub(crate) mod bulk;
pub(crate) mod check;
mod eligibility;
mod escape;
mod receipts;
pub(crate) use receipts::{busy_workspace, carry_pending};
pub(crate) mod fixture;

#[derive(Default)]
pub(crate) struct State {
    pending: std::collections::HashMap<(u64, super_platinum_core::domain::ReadTarget), u64>,
    receipts: receipts::Registry,
}

pub(crate) fn busy_channel(shell: &ShellState) -> bool {
    let (Some(team), Some(channel)) = (&shell.core.active_team, &shell.core.active_channel) else {
        return false;
    };
    let target = super_platinum_core::domain::ReadTarget::Conversation {
        team: team.clone(),
        channel: channel.clone(),
    };
    receipts::busy(shell, &target)
        || shell.read_shortcuts.pending.contains_key(&(
            shell.core.account_epoch,
            super_platinum_core::domain::ReadTarget::Conversation {
                team: team.clone(),
                channel: channel.clone(),
            },
        ))
}

pub(crate) fn busy_thread(shell: &ShellState) -> bool {
    let Some((team, channel, root_ts)) = crate::thread_unread::key(shell) else {
        return false;
    };
    let target = super_platinum_core::domain::ReadTarget::Thread {
        team: team.clone(),
        channel: channel.clone(),
        root_ts: root_ts.clone(),
    };
    receipts::busy(shell, &target)
        || shell.read_shortcuts.pending.contains_key(&(
            shell.core.account_epoch,
            super_platinum_core::domain::ReadTarget::Thread {
                team,
                channel,
                root_ts,
            },
        ))
}

/// Rendered identity only, containing no session credentials or message text.
/// The DOM captures this at keydown so a delayed event cannot adopt a new view.
pub(crate) fn key(shell: &ShellState) -> String {
    serde_json::json!([
        shell.core.active_account,
        shell.core.account_epoch,
        shell.core.active_team,
        shell.timeline_scope(),
        shell.thread_scope(),
        shell.shortcut_generation,
        format!("{:?}", shell.main_view),
        format!("{:?}", shell.channel_tab),
        shell.overlay.is_some(),
        shell.profile_user.is_some(),
        shell.group_panel.is_some(),
        shell.profile_hover.is_some(),
        shell.signed_in,
        shell.loading,
        bulk::dialog_open(shell),
    ])
    .to_string()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Payload {
    token: u64,
    key: String,
    shift: bool,
    editor: Editor,
    focus: String,
    scope: Option<String>,
    focus_generation: u64,
}

fn snapshot(shell: &ShellState, payload: &Payload) -> Option<Snapshot> {
    let focus = match payload.focus.as_str() {
        "thread" => Focus::Thread(payload.scope.clone()?),
        "primary" => Focus::Primary,
        "workspace" => Focus::Workspace,
        _ => Focus::Other,
    };
    Some(Snapshot {
        account: shell.core.active_account.clone()?,
        epoch: shell.core.account_epoch,
        team: shell.core.active_team.clone()?,
        primary_scope: shell.transcript_visible().then(|| shell.timeline_scope()),
        thread_scope: shell.thread_root.as_ref().map(|_| shell.thread_scope()),
        focus,
        focus_generation: payload.focus_generation,
        overlay_generation: shell.shortcut_generation,
        focused: crate::read_visibility::focused(),
        visible: shell.signed_in,
        chat_attention: shell.overlay.is_none()
            && !bulk::dialog_open(shell)
            && shell.profile_user.is_none()
            && shell.group_panel.is_none()
            && shell.profile_hover.is_none(),
        error_screen: !shell.signed_in,
    })
}

pub(crate) fn install_keyboard() {
    document::eval(&format!(
        "const readKeyboard = {};\n{}",
        include_str!("read_shortcuts/keyboard.js"),
        include_str!("read_shortcuts/bridge.js")
    ));
}

pub(crate) async fn handle(state: Signal<ShellState>, payload: Payload) {
    // R614311 has no ignoreInputs flag; B2 o9RR652538 disables Mousetrap's
    // default input filter unless that flag is set. Native editor/completion
    // consumption and IME are checked before this event reaches the bridge.
    let Some(intent) = eligibility::intent(
        KeyEvent {
            escape: true,
            shift: payload.shift,
            editor: payload.editor,
            ..Default::default()
        },
        ComposerPolicy {
            empty: true,
            nonempty: true,
        },
    ) else {
        return;
    };
    let capture = {
        let shell = state.peek();
        if key(&shell) != payload.key {
            return;
        }
        let Some(snapshot) = snapshot(&shell, &payload) else {
            return;
        };
        let Some(capture) = Captured::new(snapshot, intent) else {
            return;
        };
        capture
    };
    let prepared = if intent == Intent::Escape {
        escape::prepare(&state.peek(), &payload.focus, payload.scope.as_deref())
    } else {
        None
    };
    let bulk_owner = if !crate::fixture::is_fixture() {
        let shell = state.peek();
        let Some(transport) = shell.core.transport.as_ref() else {
            return;
        };
        let Some(session) = shell
            .core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(shell.core.active_team.as_deref()?))
        else {
            return;
        };
        let Some(owner) =
            crate::realtime::activity_counts::CursorOwner::capture(&shell.core, transport, session)
        else {
            return;
        };
        Some(owner)
    } else {
        None
    };
    // Keep the evaluator alive through the focus recheck. The receipt is one
    // use and rejected if focus ever left/returned or a popup appeared.
    let valid = document::eval(&format!(
        "return window.superPlatinumReadShortcuts?.take({});",
        payload.token
    ))
    .await
    .ok()
    .and_then(|v| v.as_bool())
    .unwrap_or(false);
    if !valid {
        return;
    }
    {
        let shell = state.peek();
        if key(&shell) != payload.key {
            return;
        }
        if bulk_owner
            .as_ref()
            .is_some_and(|owner| !owner.current(&shell.core))
        {
            return;
        }
        let Some(current) = snapshot(&shell, &payload) else {
            return;
        };
        if capture.consume(&current).is_none() {
            return;
        }
    }
    if intent == Intent::ClearUnreads {
        bulk::request(state);
    } else if !receipts::handle(
        state,
        payload.focus == "thread",
        payload.focus_generation,
        &payload.key,
    )
    .await
    {
        let prepared = prepared
            .or_else(|| escape::prepare(&state.peek(), &payload.focus, payload.scope.as_deref()));
        if let Some(prepared) = prepared {
            escape::execute(state, prepared).await;
        }
    }
}
