//! Exact-gated, disconnected ordinary-message action fixtures.
use dioxus::prelude::*;
use serde_json::{Value, json};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{Error, SlackClient, api, client::RequestBody, events::RtEvent, models::Message},
    state::ChannelMessages,
};

use super::{Captured, Key, Operation, Phase, source::Source};
use crate::state::{ChannelTab, ShellState};

pub(crate) const MAIN_TS: &str = "1719800000.000001";
const STALE_TS: &str = "1719800060.000001";

#[derive(Default, Clone)]
pub(crate) struct Fixture {
    pub attempts: Vec<(String, String, String)>,
    pub url_intents: usize,
    pub prepared_valid: bool,
    pub wrong_token_ignored: bool,
    pub lookup_started: Vec<String>,
    pub lookup_completed: Vec<String>,
    pub lookup_in_flight: usize,
    pub lookup_max_in_flight: usize,
    pub lookup_failed: bool,
    pub release_lookups: bool,
}

pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_MESSAGE_BUTTONS_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("message-button-actions" | "message-button-actions-narrow")
        )
}

pub(crate) fn install(shell: &mut ShellState) {
    shell.message_buttons = super::State::default();
    shell.message_buttons.fixture = Some(Fixture {
        prepared_valid: true,
        ..Fixture::default()
    });
    shell.app_modal = None;
    shell.overlay = None;
    shell.toast = None;
    shell.channel_tab = ChannelTab::Messages;
    let confirmation = json!({"title":{"type":"plain_text","text":"Open release approval?"},
        "text":{"type":"mrkdwn","text":"Review *release A* before continuing. Your selection belongs to this message."},
        "confirm":{"type":"plain_text","text":"Open approval"},"deny":{"type":"plain_text","text":"Cancel"},"style":"primary"});
    let messages: Vec<Message> = vec![
        json!({"ts":MAIN_TS,"channel":"C2","user":"U1","bot_id":"B_FIXTURE",
            "bot_profile":{"id":"B_FIXTURE","app_id":"A_FIXTURE","team_id":"T1"},
            "blocks":[
                {"type":"section","block_id":"intro","text":{"type":"plain_text","text":"Two releases share a label. Each action retains its own block identity."}},
                {"type":"actions","block_id":"release-a","elements":[
                    {"type":"button","action_id":"open","value":"release-a","text":{"type":"plain_text","text":"Open"},"accessibility_label":"Open release A","style":"primary","url":"https://example.test/release-a","confirm":confirmation},
                    {"type":"button","action_id":"hidden","text":{"type":"plain_text","text":"Hidden recipient"},"visible_to_user_ids":["U_OTHER"]}]},
                {"type":"actions","block_id":"release-b","elements":[
                    {"type":"button","action_id":"open","value":"release-b","text":{"type":"plain_text","text":"Open"},"accessibility_label":"Open release B"}]}
            ]}),
        json!({"ts":STALE_TS,"channel":"C2","user":"U1","bot_id":"B_FIXTURE",
            "bot_profile":{"id":"B_FIXTURE","app_id":"A_FIXTURE","team_id":"T1"},
            "blocks":[{"type":"section","text":{"type":"plain_text","text":"A changed message invalidates its open confirmation."}},
                {"type":"actions","block_id":"stale","elements":[{"type":"button","action_id":"review","value":"before","text":{"type":"plain_text","text":"Review changed action"},"confirm":confirmation}]}]}),
        json!({"ts":"1719800120.000001","channel":"C2","user":"U1","bot_id":"B_FIXTURE",
            "bot_profile":{"id":"B_FIXTURE","app_id":"A_FIXTURE","team_id":"T1"},
            "blocks":[{"type":"actions","block_id":"ambiguous","elements":[
                {"type":"button","action_id":"same","text":{"type":"plain_text","text":"Ambiguous first"}},
                {"type":"button","action_id":"same","text":{"type":"plain_text","text":"Ambiguous second"}}]}]})
    ].into_iter().map(|value|serde_json::from_value(value).expect("message button fixture")).collect();
    let mut bag = ChannelMessages::default();
    for message in messages {
        bag.upsert(message);
    }
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .insert("C2".into(), bag);
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

pub(crate) fn edit_confirmed_source(mut state: Signal<ShellState>) {
    if !enabled() {
        return;
    }
    let mut shell = state.write();
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
        .find(|message| message.ts.as_deref() == Some(STALE_TS))
        .unwrap();
    message.blocks[1]["elements"][0]["value"] = json!("after");
    reproject(&mut shell);
    super::poll(&mut shell);
}

/// A pure prepared request is inspected, never executed. Even URL intents are
/// recorded instead of opening the external browser in this fixture path.
pub(crate) fn dispatch(mut state: Signal<ShellState>, captured: Captured) -> bool {
    if !enabled() {
        return false;
    }
    let mut shell = state.write();
    if shell.core.transport.is_some() || shell.message_buttons.fixture.is_none() {
        return true;
    }
    if shell.overlay.is_some()
        || !super::presentation_for_button(&shell, captured.source(), captured.metadata()).enabled
    {
        return true;
    }
    let Ok(current) = captured.revalidate(&shell) else {
        return true;
    };
    let Some(correlation) = crate::app_modal::register_action(&mut shell, captured.app_id()) else {
        return true;
    };
    let owner = captured.source().owner();
    let workspace = WorkspaceSession {
        team_id: owner.team_id.clone(),
        enterprise_id: None,
        user_id: owner.self_user_id.clone(),
        name: "Disconnected fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    };
    let request = super::prepare_request(
        &mut shell,
        &captured,
        &SlackClient::default(),
        &workspace,
        &correlation.token,
    )
    .expect("validated fixture request");
    let request_target_valid = request.method == "POST"
        && request
            .url
            .split('?')
            .next()
            .is_some_and(|url| url.ends_with("/api/blocks.actions"));
    let RequestBody::Form(fields) = request.body else {
        panic!("expected fixture form request")
    };
    let field = |name: &str| {
        fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    let actions: Value = serde_json::from_str(field("actions").unwrap()).unwrap();
    let container: Value = serde_json::from_str(field("container").unwrap()).unwrap();
    let prepared_valid = request_target_valid
        && actions == json!([current.action()])
        && actions[0]["block_id"].as_str() == captured.metadata().block_id.as_deref()
        && actions[0]["action_id"].as_str() == captured.metadata().action_id.as_deref()
        && actions[0]["value"].as_str() == captured.metadata().value.as_deref()
        && container == current.container()
        && container["channel_id"] == "C2"
        && container["message_ts"] == MAIN_TS
        && field("service_id") == Some("B_FIXTURE")
        && field("client_token") == Some(correlation.token.as_str())
        && field("app_id") == Some("A_FIXTURE")
        && field("service_team_id") == Some("T1");
    let block = captured.metadata().block_id.clone().unwrap();
    let action = captured.metadata().action_id.clone().unwrap();
    let value = captured.metadata().value.clone().unwrap_or_default();
    let fixture = shell.message_buttons.fixture.as_mut().unwrap();
    fixture.prepared_valid &= prepared_valid;
    fixture.attempts.push((block.clone(), action, value));
    fixture.url_intents += usize::from(captured.url().is_some());
    let key = Key::new(captured.source(), captured.metadata()).unwrap();
    let generation = super::next_id();
    shell.message_buttons.operations.insert(
        key.clone(),
        Operation {
            captured,
            phase: Phase::Pending,
            generation,
        },
    );
    drop(shell);
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let mut shell = state.write();
        if block == "release-b" {
            super::finish(
                &mut shell,
                &key,
                generation,
                &correlation,
                Err(Error::Api("fixture_ambiguous_outcome".into())),
            );
            return;
        }
        super::finish(
            &mut shell,
            &key,
            generation,
            &correlation,
            Ok(api::BlockActionResponse::Accepted),
        );
        let rt_generation = shell.core.workspaces["T1"].rt_generation;
        let event = |token: &str| {
            RtEvent::Unknown(serde_json::from_value(json!({
            "type":"view_opened","view_id":"V_FORM_FIXTURE","app_id":"A_FIXTURE","view_type":"modal",
            "client_token":token,"view":crate::app_modal::fixture::view()
        })).expect("correlated fixture view"))
        };
        crate::app_modal::realtime(
            &mut shell,
            "T1",
            rt_generation,
            &event("wrong-fixture-token"),
        );
        let ignored = shell
            .app_modal
            .as_ref()
            .is_some_and(|session| session.modal.stack().is_empty())
            && shell.overlay.is_none();
        shell
            .message_buttons
            .fixture
            .as_mut()
            .unwrap()
            .wrong_token_ignored = ignored;
        crate::app_modal::realtime(&mut shell, "T1", rt_generation, &event(&correlation.token));
        crate::app_modal::fixture::enable_responses(&mut shell);
    });
    true
}

pub(crate) fn metadata_enabled() -> bool {
    std::env::var("SUPER_PLATINUM_MESSAGE_BUTTONS_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("message-button-metadata" | "message-button-metadata-narrow")
        )
}

pub(crate) fn install_metadata(shell: &mut ShellState) {
    shell.message_buttons = super::State::default();
    shell.message_buttons.fixture = Some(Fixture::default());
    shell.app_modal = None;
    shell.overlay = None;
    shell.toast = None;
    shell.channel_tab = ChannelTab::Messages;
    let mut bag = ChannelMessages::default();
    for index in 1..=11 {
        let incomplete = index == 11;
        let number = if incomplete { 999 } else { index };
        let mut value = json!({
            "ts":format!("1719800{index:03}.000001"),"channel":"C2","user":"U1",
            "bot_id":format!("BLOOKUP{number:03}"),"app_id":format!("ALOOKUP{number:03}"),
            "blocks":[{"type":"actions","block_id":"open","elements":[{
                "type":"button","action_id":"open","value":format!("lookup-{number}"),
                "text":{"type":"plain_text","text":if incomplete {"Incomplete inline profile".into()}else{format!("Open app {index:02}")}},
                "accessibility_label":if incomplete {"Incomplete inline profile".into()}else{format!("Lookup bot {index:02}")}
            }]}]
        });
        if incomplete {
            value["bot_profile"] = json!({"id":"BLOOKUP999","name":"Incomplete inline identity"});
        }
        bag.upsert(serde_json::from_value(value).expect("metadata lookup fixture"));
    }
    let workspace = shell.core.workspaces.get_mut("T1").unwrap();
    workspace.bots.clear();
    workspace.messages.insert("C2".into(), bag);
    reproject(shell);
}

/// Mirror the bounded cache lifecycle with synthetic records, without a client
/// or transport. Initial completion is checker-controlled so loading is visible.
pub(crate) fn hydrate(mut state: Signal<ShellState>, source: Source) -> bool {
    if !metadata_enabled() {
        return false;
    }
    let candidate = {
        let shell = state.peek();
        if shell.core.transport.is_some() || shell.message_buttons.fixture.is_none() {
            return true;
        }
        source.lookup_target(&shell)
    };
    let Some((bot, hint)) = candidate else {
        return true;
    };
    let owner = source.owner().clone();
    let ticket = {
        #[allow(deprecated)]
        let mut shell = state.write_silent();
        if source.lookup_target(&shell).as_ref() != Some(&(bot.clone(), hint.clone())) {
            return true;
        }
        let ticket = shell
            .core
            .workspaces
            .get_mut(&owner.team_id)
            .unwrap()
            .bots
            .begin(&bot, hint.as_deref(), super::hydration::now());
        let Some(ticket) = ticket else {
            return true;
        };
        let fixture = shell.message_buttons.fixture.as_mut().unwrap();
        fixture.lookup_started.push(bot.clone());
        fixture.lookup_in_flight += 1;
        fixture.lookup_max_in_flight = fixture.lookup_max_in_flight.max(fixture.lookup_in_flight);
        ticket
    };
    drop(state.write());
    dioxus::core::spawn_forever(async move {
        // Hold the initial full batch until native DOM loading assertions pass.
        for _ in 0..500 {
            if state
                .peek()
                .message_buttons
                .fixture
                .as_ref()
                .is_some_and(|f| f.release_lookups)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        let mut shell = state.write();
        if shell.core.account_epoch != owner.account_epoch
            || shell.core.transport.is_some()
            || shell
                .core
                .workspaces
                .get(&owner.team_id)
                .is_none_or(|w| w.self_user_id != owner.self_user_id)
            || shell
                .message_buttons
                .fixture
                .as_ref()
                .is_none_or(|f| !f.release_lookups)
        {
            return;
        }
        let profile = serde_json::from_value(
            json!({"id":bot,"app_id":format!("A{}",bot.strip_prefix('B').unwrap()),
            "team_id":"TVALID001","name":"Disconnected lookup app"}),
        )
        .expect("synthetic bot profile");
        let completed = shell
            .core
            .workspaces
            .get_mut(&owner.team_id)
            .unwrap()
            .bots
            .complete(&ticket, Some(profile), super::hydration::now());
        let fixture = shell.message_buttons.fixture.as_mut().unwrap();
        fixture.lookup_in_flight -= 1;
        fixture.lookup_failed |= !completed;
        if completed {
            fixture.lookup_completed.push(bot);
        }
        shell.message_buttons.hydration_revision =
            shell.message_buttons.hydration_revision.wrapping_add(1);
    });
    true
}

pub(crate) fn release_metadata(mut state: Signal<ShellState>) {
    if !metadata_enabled() {
        return;
    }
    if let Some(fixture) = state.write().message_buttons.fixture.as_mut() {
        fixture.release_lookups = true;
    }
}
