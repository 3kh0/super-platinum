//! Exact-gated, disconnected overflow controller coverage.
//! Requests are prepared and inspected, never sent to a transport.
use dioxus::prelude::*;
use serde_json::{Value, json};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, api, client::RequestBody, models::Message},
    state::ChannelMessages,
};

use super::{Captured, Key, Operation, Phase};
use crate::state::{ChannelOpen, ChannelTab, ShellState};

pub(crate) const MAIN_TS: &str = "1719930000.000001";

#[derive(Default)]
pub(crate) struct Fixture {
    pub attempts: Vec<Value>,
    pub prepared_valid: bool,
    pub urls: Vec<String>,
    pending: Option<(Key, u64, crate::app_modal::ActionToken)>,
}

pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_MESSAGE_OVERFLOW_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("message-overflow" | "message-overflow-narrow")
        )
}

pub(crate) fn install(shell: &mut ShellState) {
    shell.message_buttons = super::State::default();
    shell.message_buttons.overflow_fixture = Some(Fixture {
        prepared_valid: true,
        ..Fixture::default()
    });
    shell.core.transport = None;
    shell.app_modal = None;
    shell.overlay = None;
    shell.toast = None;
    shell.channel_tab = ChannelTab::Messages;
    let message: Message = serde_json::from_value(json!({
        "ts":MAIN_TS,"channel":"C2","user":"U1","bot_id":"B_FIXTURE",
        "bot_profile":{"id":"B_FIXTURE","app_id":"A_FIXTURE","team_id":"T1"},
        "blocks":[
            {"type":"section","block_id":"intro","text":{"type":"plain_text","text":"App actions. Open a menu, choose an option, then confirm."}},
            {"type":"section","block_id":"choice","text":{"type":"plain_text","text":"Review actions"},
                "accessory":{"type":"overflow","action_id":"pick","options":[
                    {"text":{"type":"plain_text","text":"Approve review","emoji":true},"value":"approve"},
                    {"text":{"type":"plain_text","text":"Open review guide"},"value":"guide","url":"https://example.test/guide"}],
                    "confirm":{"title":{"type":"plain_text","text":"Apply review action?"},
                        "text":{"type":"mrkdwn","text":"Apply this *review action*?"},
                        "confirm":{"type":"plain_text","text":"Apply action"},
                        "deny":{"type":"plain_text","text":"Cancel"}}}},
            {"type":"actions","block_id":"review","elements":[
                {"type":"overflow","action_id":"other","options":[
                    {"text":{"type":"plain_text","text":"Save for later"},"value":"save"},
                    {"text":{"type":"plain_text","text":"Open project guide"},"value":"link","url":"https://example.test/project"}]},
                {"type":"static_select","action_id":"environment","placeholder":{"type":"plain_text","text":"Choose environment"},
                    "options":[{"text":{"type":"plain_text","text":"Production"},"value":"prod"}],
                    "initial_option":{"text":{"type":"plain_text","text":"Production"},"value":"prod"}},
                {"type":"button","action_id":"submit","value":"submit-review","text":{"type":"plain_text","text":"Submit review"}}
            ]}
        ]
    })).expect("overflow fixture message");
    let mut bag = ChannelMessages::default();
    bag.upsert(message);
    let workspace = shell
        .core
        .workspaces
        .get_mut("T1")
        .expect("fixture workspace");
    workspace.messages.insert("C2".into(), bag);
    workspace
        .messages
        .insert("C1".into(), ChannelMessages::default());
    if let Some(index) = shell.channels.iter().position(|channel| channel.id == "C2") {
        shell.select_channel(index, ChannelOpen::Global);
    }
    reproject(shell);
}

fn reproject(shell: &mut ShellState) {
    let workspace = &shell.core.workspaces["T1"];
    let rows = workspace.messages["C2"]
        .messages
        .iter()
        .map(|message| crate::message_vm::message_vm(workspace, message, &shell.media))
        .collect();
    shell.install_fixture_messages(rows);
}

pub(crate) fn dispatch(mut state: Signal<ShellState>, captured: Captured) -> bool {
    if !enabled() {
        return false;
    }
    let mut shell = state.write();
    if shell.core.transport.is_some()
        || shell.message_buttons.overflow_fixture.is_none()
        || shell.overlay.is_some()
        || captured.revalidate(&shell).is_err()
    {
        return true;
    }
    // The native check deliberately delivers synthetic change events while all
    // controls are disabled. They must not bypass the container's pending write.
    if shell.message_buttons.operations.values().any(|operation| {
        operation.captured.source().owner() == captured.source().owner()
            && operation
                .captured
                .source()
                .same_action_revision(captured.source())
    }) {
        return true;
    }
    let Some(correlation) = crate::app_modal::register_action(&mut shell, captured.app_id()) else {
        return true;
    };
    let owner = captured.source().owner();
    let workspace = WorkspaceSession {
        team_id: owner.team_id.clone(),
        enterprise_id: None,
        user_id: owner.self_user_id.clone(),
        name: "Disconnected time fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    };
    let request = match super::prepare_request(
        &mut shell,
        &captured,
        &SlackClient::default(),
        &workspace,
        &correlation.token,
    ) {
        Ok(request) => request,
        Err(reason) => {
            crate::app_modal::reject_action(&mut shell, &correlation);
            shell.show_toast(reason);
            return true;
        }
    };
    let target_valid = request.method == "POST"
        && !request.retry_safe()
        && request
            .url
            .split('?')
            .next()
            .is_some_and(|url| url.ends_with("/api/blocks.actions"));
    let RequestBody::Form(fields) = request.body else {
        panic!("expected overflow form request")
    };
    let field = |name: &str| {
        fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    let actions: Value =
        serde_json::from_str(field("actions").expect("action field")).expect("action JSON");
    let container: Value =
        serde_json::from_str(field("container").expect("container field")).expect("container JSON");
    let full_state: Value =
        serde_json::from_str(field("state").expect("complete state field")).expect("state JSON");
    let valid = target_valid
        && actions.as_array().is_some_and(|actions| actions.len() == 1)
        && matches!(actions[0]["type"].as_str(), Some("button" | "overflow"))
        && actions[0]["block_id"].as_str() == captured.metadata().block_id.as_deref()
        && actions[0]["action_id"].as_str() == captured.metadata().action_id.as_deref()
        && container
            == json!({"type":"message","channel_id":"C2","message_ts":MAIN_TS,"is_ephemeral":false})
        && field("service_id") == Some("B_FIXTURE")
        && field("app_id") == Some("A_FIXTURE")
        && field("service_team_id") == Some("T1")
        && field("client_token") == Some(correlation.token.as_str())
        && full_state["values"].is_object();
    let fixture = shell.message_buttons.overflow_fixture.as_mut().unwrap();
    if let Some(url) = captured.url() {
        fixture.urls.push(url.to_owned());
    }
    fixture.prepared_valid &= valid;
    fixture.attempts.push(json!({"action":actions[0],"state":full_state["values"],"container":container,"prepared_valid":valid}));
    let key = Key::new(captured.source(), captured.metadata()).expect("validated action identity");
    let generation = super::next_id();
    shell.message_buttons.operations.insert(
        key.clone(),
        Operation {
            captured,
            phase: Phase::Pending,
            generation,
        },
    );
    shell
        .message_buttons
        .overflow_fixture
        .as_mut()
        .unwrap()
        .pending = Some((key, generation, correlation));
    true
}

/// The native checker owns timing; no timer can hide duplicate-pending races.
pub(crate) fn complete(state: Signal<ShellState>) {
    finish_fixture(state, false);
}
pub(crate) fn uncertain(state: Signal<ShellState>) {
    finish_fixture(state, true);
}
fn finish_fixture(mut state: Signal<ShellState>, uncertain: bool) {
    if !enabled() {
        return;
    }
    let mut shell = state.write();
    if shell.core.transport.is_some() {
        return;
    }
    let Some((key, generation, correlation)) = shell
        .message_buttons
        .overflow_fixture
        .as_mut()
        .and_then(|fixture| fixture.pending.take())
    else {
        return;
    };
    super::finish(
        &mut shell,
        &key,
        generation,
        &correlation,
        if uncertain {
            Err(super_platinum_core::slack::Error::Api(
                "fixture_uncertain".into(),
            ))
        } else {
            Ok(api::BlockActionResponse::Accepted)
        },
    );
}

pub(crate) fn navigate_away_back(mut state: Signal<ShellState>) {
    if !enabled() {
        return;
    }
    let checkpoint = {
        let mut shell = state.write();
        if shell.core.transport.is_some() || shell.message_buttons.overflow_fixture.is_none() {
            return;
        }
        let Some(index) = shell.channels.iter().position(|channel| channel.id == "C1") else {
            return;
        };
        shell.select_channel(index, ChannelOpen::Global);
        (shell.core.account_epoch, shell.channel_generation)
    };
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        {
            let mut shell = state.write();
            if shell.core.transport.is_some()
                || shell.core.account_epoch != checkpoint.0
                || shell.channel_generation != checkpoint.1
                || shell.core.active_team.as_deref() != Some("T1")
            {
                return;
            }
            let Some(index) = shell.channels.iter().position(|channel| channel.id == "C2") else {
                return;
            };
            shell.select_channel(index, ChannelOpen::Global);
        }
        crate::bootstrap::refresh_selected_channel(state).await;
    });
}

pub(crate) fn change_initial(mut state: Signal<ShellState>) {
    if !enabled() {
        return;
    }
    let mut shell = state.write();
    if shell.core.transport.is_some() || shell.message_buttons.overflow_fixture.is_none() {
        return;
    }
    let message = shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut("C2")
        .unwrap()
        .messages
        .iter_mut()
        .find(|message| message.ts.as_deref() == Some(MAIN_TS))
        .unwrap();
    message.blocks[1]["accessory"]["options"][0]["text"]["text"] = json!("Approve updated review");
    reproject(&mut shell);
    super::poll(&mut shell);
}
