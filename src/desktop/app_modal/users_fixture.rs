//! Named disconnected fixture; prepares real views.submit envelopes, never sends.
use super::*;
use serde_json::json;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, client::RequestBody},
};

pub(crate) const VIEW: &str = "V_MODAL_USERS";
pub(crate) struct Recorder {
    pub states: Vec<Value>,
    pub valid: bool,
    pending: Option<(String, u64, String)>,
}
pub(crate) fn enabled() -> bool {
    super::multi_conversation_fixture::enabled()
        || super::multi_channel_fixture::enabled()
        || super::channel_fixture::enabled()
        || super::conversation_fixture::enabled()
        || single_enabled()
        || (std::env::var("SUPER_PLATINUM_APP_MODAL_USERS_CHECK").as_deref() == Ok("1")
            && matches!(
                std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
                Ok("app-modal-users" | "app-modal-users-narrow")
            ))
}
pub(crate) fn single_enabled() -> bool {
    std::env::var("SUPER_PLATINUM_APP_MODAL_USER_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-modal-user" | "app-modal-user-narrow")
        )
}
pub(crate) fn install(shell: &mut ShellState) {
    install_view(shell, view());
}
pub(crate) fn install_single(shell: &mut ShellState) {
    install_view(shell, single_view());
}
pub(super) fn install_view(shell: &mut ShellState, view: Value) {
    shell.core.transport = None;
    shell.toast = None;
    let w = shell.core.workspaces.get_mut("T1").expect("fixture team");
    w.users.clear();
    w.self_user_id = "U00000001".into();
    for raw in [
        json!({"id":"U00000001","name":"Ada Self","is_self":true}),
        json!({"id":"U00000002","name":"Blair Reviewer"}),
        json!({"id":"U00000003","name":"Casey Guest","is_restricted":true}),
        json!({"id":"U00000004","name":"Dev External","is_stranger":true}),
        json!({"id":"U00000005","name":"Ordinary Bot","is_bot":true}),
        json!({"id":"U00000006","name":"Deleted Member","deleted":true}),
        json!({"id":"U00000007","name":"Unavailable Agent","is_agentforce_bot":true}),
    ] {
        let user: super_platinum_core::slack::models::User = serde_json::from_value(raw).unwrap();
        w.users.insert(user.id.clone(), user);
    }
    let mut session = Session::fixture(view);
    session.users_fixture = Some(Recorder {
        states: vec![],
        valid: true,
        pending: None,
    });
    shell.app_modal = Some(session);
    shell.overlay = Some(Overlay::AppModal);
}
pub(crate) fn view() -> Value {
    json!({"id":VIEW,"app_id":"A_FIXTURE","type":"modal","title":{"type":"plain_text","text":"Choose reviewers"},
    "submit":{"type":"plain_text","text":"Submit review"},"close":{"type":"plain_text","text":"Cancel"},"blocks":[
        {"type":"input","block_id":"note","optional":true,"label":{"type":"plain_text","text":"Review note"},
            "element":{"type":"plain_text_input","action_id":"value","initial_value":"Retained sibling draft"}},
        {"type":"input","block_id":"reviewers","label":{"type":"plain_text","text":"Reviewers"},
            "element":{"type":"multi_users_select","action_id":"value","initial_users":["U00000001"],"max_selected_items":2,
                "placeholder":{"type":"plain_text","text":"Find reviewers"}}},
        {"type":"input","block_id":"optional","optional":true,"label":{"type":"plain_text","text":"Observers"},
            "element":{"type":"multi_users_select","action_id":"value","placeholder":{"type":"plain_text","text":"Find observers"}}}
    ]})
}
pub(crate) fn single_view() -> Value {
    json!({"id":VIEW,"app_id":"A_FIXTURE","type":"modal","title":{"type":"plain_text","text":"Assign a reviewer"},
    "submit":{"type":"plain_text","text":"Submit review"},"close":{"type":"plain_text","text":"Cancel"},"blocks":[
        {"type":"input","block_id":"note","optional":true,"label":{"type":"plain_text","text":"Review note"},
            "element":{"type":"plain_text_input","action_id":"value","initial_value":"Retained sibling draft"}},
        {"type":"input","block_id":"reviewers","label":{"type":"plain_text","text":"Reviewer"},
            "element":{"type":"users_select","action_id":"value","initial_user":"U00000001",
                "placeholder":{"type":"plain_text","text":"Find a reviewer"}}},
        {"type":"input","block_id":"optional","optional":true,"label":{"type":"plain_text","text":"Observer"},
            "element":{"type":"users_select","action_id":"value","placeholder":{"type":"plain_text","text":"Find an observer"}}}
    ]})
}
pub(super) fn record(session: &mut Session, payload: &Value, token: &str) -> bool {
    if !enabled() || !session.fixture || session.users_fixture.is_none() {
        return false;
    }
    let workspace = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U00000001".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-token".into(),
    };
    let request = api::block_view_submit(&SlackClient::default(), &workspace, VIEW, token, payload)
        .expect("fixture request");
    let valid = request.method == "POST"
        && !request.retry_safe()
        && request.url.split('?').next() == Some("https://fixture.slack.com/api/views.submit");
    let RequestBody::Form(fields) = request.body else {
        panic!("form request");
    };
    let get = |name: &str| {
        fields
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    };
    let state: Value = serde_json::from_str(get("state").unwrap()).unwrap();
    let revision = session.modal.revision();
    let f = session.users_fixture.as_mut().unwrap();
    f.valid &= valid
        && get("view_id") == Some(VIEW)
        && get("client_token") == Some(token)
        && state == *payload;
    f.states.push(state);
    f.pending = Some((VIEW.into(), revision, token.into()));
    session.fixture_attempts += 1;
    true
}
pub(crate) fn complete(mut state: Signal<ShellState>, uncertain: bool) {
    if !enabled() {
        return;
    }
    let mut shell = state.write();
    if shell.core.transport.is_some() {
        return;
    }
    let pending = shell
        .app_modal
        .as_mut()
        .and_then(|s| s.users_fixture.as_mut())
        .and_then(|f| f.pending.take());
    let Some((view, revision, token)) = pending else {
        return;
    };
    let result = if uncertain {
        Err(super_platinum_core::slack::Error::Api(
            "fixture_unknown_outcome".into(),
        ))
    } else {
        Ok(api::BlockViewResponse::Rejected(api::BlockViewRejection {
            error: "validation_errors".into(),
            message: None,
            errors: [("reviewers".into(), "Choose another reviewer.".into())].into(),
            view_error: None,
        }))
    };
    requests::complete(&mut shell, &view, revision, &token, result);
}
