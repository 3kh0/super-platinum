//! Exact-gated synthetic options and prepared requests. No transport is executed.
use super::{Captured, Key, Operation, Phase};
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};
use super_platinum_core::{
    block_actions::external::ExternalSuggestions,
    config::WorkspaceSession,
    slack::{Error, SlackClient, api, client::RequestBody},
};

#[derive(Default)]
pub(crate) struct Fixture {
    pub queries: Vec<Value>,
    pub attempts: Vec<Value>,
    pub valid: bool,
    pub(crate) hold_suggestions: bool,
    pending: Option<(Key, u64, crate::app_modal::ActionToken)>,
    pub(crate) modal_pending: Option<(String, u64, String)>,
}
pub(crate) fn enabled() -> bool {
    super::multi_external_fixture::enabled()
        || single_enabled(
            std::env::var("SUPER_PLATINUM_EXTERNAL_SELECT_CHECK")
                .ok()
                .as_deref(),
            std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
        )
}
fn single_enabled(flag: Option<&str>, fixture: Option<&str>) -> bool {
    flag == Some("1")
        && matches!(
            fixture,
            Some(
                "message-external-select"
                    | "message-external-select-narrow"
                    | "app-modal-external-select"
                    | "app-modal-external-select-narrow"
            )
        )
}
pub(crate) fn modal() -> bool {
    std::env::var("SUPER_PLATINUM_FIXTURE").is_ok_and(|n| {
        n.starts_with("app-modal-external-select")
            || n.starts_with("app-modal-multi-external-select")
    })
}
pub(crate) fn option(label: &str, value: &str) -> Value {
    json!({"text":{"type":"plain_text","text":label},"value":value,"description":{"type":"plain_text","text":"App-provided release option"}})
}
pub(crate) fn view() -> Value {
    json!({
        "id":"V_EXTERNAL_FIXTURE","app_id":"A_FIXTURE","bot_id":"B_FIXTURE","app_installed_team_id":"T1","type":"modal","hash":"external1",
        "title":{"type":"plain_text","text":"Choose a release"},"submit":{"type":"plain_text","text":"Submit choices"},"close":{"type":"plain_text","text":"Cancel"},
        "blocks":[{"type":"input","block_id":"choice","optional":true,"label":{"type":"plain_text","text":"Release"},"element":{"type":"external_select","action_id":"pick","initial_option":option("Installed release","installed"),"placeholder":{"type":"plain_text","text":"Find release"}}},
        {"type":"input","block_id":"note","optional":true,"label":{"type":"plain_text","text":"Notes"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Keep this sibling"}}]
    })
}
pub(crate) fn install(shell: &mut ShellState) {
    super::select_fixture::install(shell);
    shell.message_buttons.select_fixture = None;
    shell.message_buttons.external_fixture = Some(Fixture {
        valid: true,
        ..Fixture::default()
    });
    if modal() {
        crate::app_modal::external::install_fixture(shell);
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
        .first_mut()
        .unwrap();
    let mut blocks = message.blocks.clone();
    blocks[1]["accessory"] = json!({"type":"external_select","action_id":"pick","initial_option":option("Installed release","installed"),"placeholder":{"type":"plain_text","text":"Find release"},"confirm":{"title":{"type":"plain_text","text":"Change release?"},"text":{"type":"plain_text","text":"Apply this app-provided option?"},"confirm":{"type":"plain_text","text":"Apply release"},"deny":{"type":"plain_text","text":"Cancel"}}});
    message.blocks = blocks;
    let workspace = &shell.core.workspaces["T1"];
    let rows = workspace.messages["C2"]
        .messages
        .iter()
        .map(|m| crate::message_vm::message_vm(workspace, m, &shell.media))
        .collect();
    shell.install_fixture_messages(rows);
}
pub(crate) fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        // fixture_core owns U0; U1 is the synthetic message author.
        user_id: "U0".into(),
        name: "Disconnected external fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    }
}
fn form(
    request: super_platinum_core::slack::PreparedRequest,
) -> (bool, std::collections::BTreeMap<String, String>) {
    let valid = request.method == "POST" && !request.retry_safe();
    let RequestBody::Form(fields) = request.body else {
        panic!("fixture request must be form encoded");
    };
    (valid, fields.into_iter().collect())
}
pub(crate) async fn suggestions(
    mut state: Signal<ShellState>,
    owner: &super::external::Owner,
    query: &str,
) -> Option<Result<ExternalSuggestions, Error>> {
    if !enabled()
        || state.read().core.transport.is_some()
        || state.read().message_buttons.external_fixture.is_none()
    {
        return None;
    }
    let request =
        match api::block_suggestions(&SlackClient::default(), &workspace(), owner.route(), query) {
            Ok(r) => r,
            Err(e) => return Some(Err(e)),
        };
    let endpoint = request
        .url
        .split('?')
        .next()
        .is_some_and(|u| u.ends_with("/api/blocks.suggestions"));
    let (valid, fields) = form(request);
    let valid = valid
        && endpoint
        && fields.get("value").map(String::as_str) == Some(query)
        && fields.get("service_id").map(String::as_str) == Some("B_FIXTURE")
        && fields.get("service_team_id").map(String::as_str) == Some("T1")
        && !["state", "client_token", "cursor", "offset", "page"]
            .iter()
            .any(|k| fields.contains_key(*k));
    {
        let mut shell = state.write();
        let f = shell.message_buttons.external_fixture.as_mut().unwrap();
        f.valid &= valid;
        f.queries
            .push(json!({"query":query,"fields":fields,"valid":valid}));
    }
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    if enabled() {
        // Only exact-gated synthetic single/multi checkers can hold Loading.
        // The enclosing query task still owns cancellation; never wait forever.
        let released = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let held = state
                    .read()
                    .message_buttons
                    .external_fixture
                    .as_ref()
                    .is_some_and(|f| f.hold_suggestions);
                if !held {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await;
        if released.is_err() {
            return Some(Err(Error::Api("fixture_loading_capture_timeout".into())));
        }
    }
    Some(if query == "error" {
        Err(Error::RateLimited {
            retry_after_secs: Some(3),
        })
    } else {
        api::decode_external_suggestions(&if query == "empty" {
            json!({"ok":true,"options":[]})
        } else {
            json!({"ok":true,"option_groups":[{"label":{"type":"plain_text","text":"Stable"},"options":[option("Release Alpha","alpha,desktop")]},{"label":{"type":"plain_text","text":"Preview"},"options":[option("Release Beta","beta")]}]})
        })
    })
}
pub(crate) fn dispatch(mut state: Signal<ShellState>, captured: Captured) -> bool {
    if !enabled() || modal() {
        return false;
    }
    let mut shell = state.write();
    if shell.core.transport.is_some()
        || shell.message_buttons.external_fixture.is_none()
        || shell.overlay.is_some()
        || captured.revalidate(&shell).is_err()
    {
        return true;
    }
    if shell
        .message_buttons
        .operations
        .values()
        .any(|op| op.captured.source().owner() == captured.source().owner())
    {
        return true;
    }
    let Some(correlation) = crate::app_modal::register_action(&mut shell, captured.app_id()) else {
        return true;
    };
    let request = match super::prepare_request(
        &mut shell,
        &captured,
        &SlackClient::default(),
        &workspace(),
        &correlation.token,
    ) {
        Ok(r) => r,
        Err(_) => return true,
    };
    let endpoint = request
        .url
        .split('?')
        .next()
        .is_some_and(|u| u.ends_with("/api/blocks.actions"));
    let (valid, fields) = form(request);
    let actions: Value = serde_json::from_str(&fields["actions"]).unwrap();
    let full: Value = serde_json::from_str(&fields["state"]).unwrap();
    let f = shell.message_buttons.external_fixture.as_mut().unwrap();
    f.valid &= valid && endpoint;
    f.attempts
        .push(json!({"action":actions[0],"state":full["values"],"fields":fields}));
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
    shell
        .message_buttons
        .external_fixture
        .as_mut()
        .unwrap()
        .pending = Some((key, generation, correlation));
    true
}
pub(crate) fn complete(mut state: Signal<ShellState>, uncertain: bool) {
    if !enabled() || state.read().core.transport.is_some() {
        return;
    }
    if modal() {
        crate::app_modal::external::complete_fixture(state, uncertain);
        return;
    }
    let mut shell = state.write();
    let Some((key, generation, correlation)) = shell
        .message_buttons
        .external_fixture
        .as_mut()
        .and_then(|f| f.pending.take())
    else {
        return;
    };
    super::finish(
        &mut shell,
        &key,
        generation,
        &correlation,
        if uncertain {
            Err(Error::Api("fixture_unknown_outcome".into()))
        } else {
            Ok(api::BlockActionResponse::Accepted)
        },
    );
}
pub(crate) fn record_modal(shell: &mut ShellState, payload: &Value, token: &str) -> bool {
    if !enabled() || !modal() || shell.core.transport.is_some() {
        return false;
    }
    let Some(session) = shell.app_modal.as_ref() else {
        return false;
    };
    let view = session.modal.stack().last().unwrap();
    let request = api::block_view_submit(
        &SlackClient::default(),
        &workspace(),
        &view.id,
        token,
        payload,
    )
    .unwrap();
    let (valid, fields) = form(request);
    let wire: Value = serde_json::from_str(&fields["state"]).unwrap();
    let f = shell.message_buttons.external_fixture.as_mut().unwrap();
    f.valid &= valid && wire == *payload;
    f.attempts.push(json!({"state":wire,"fields":fields}));
    f.modal_pending = Some((view.id.clone(), session.modal.revision(), token.into()));
    true
}

#[cfg(test)]
mod tests {
    use super::single_enabled;

    #[test]
    fn single_loading_barrier_requires_explicit_flag_and_exact_fixture() {
        for fixture in [
            "message-external-select",
            "message-external-select-narrow",
            "app-modal-external-select",
            "app-modal-external-select-narrow",
        ] {
            assert!(single_enabled(Some("1"), Some(fixture)));
            for flag in [None, Some(""), Some("0"), Some("true"), Some("1 ")] {
                assert!(!single_enabled(flag, Some(fixture)));
            }
            assert!(!single_enabled(
                Some("1"),
                Some(&format!("{fixture}-extra"))
            ));
        }
        for fixture in [
            None,
            Some("channel"),
            Some("message-multi-external-select"),
            Some("app-modal-multi-external-select"),
        ] {
            assert!(!single_enabled(Some("1"), fixture));
        }
    }
}
