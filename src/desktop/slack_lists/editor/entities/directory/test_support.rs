//! Synthetic in-memory owners for the actual directory completion adapters.
use super::*;
use super_platinum_core::config::{Session, WorkspaceSession};

pub(crate) struct Harness {
    pub(crate) state: Signal<ShellState>,
    pub(crate) context: crate::scheduled::Context,
    pub(crate) _owner: crate::bootstrap::test_support::Harness,
}
pub(crate) fn harness(channel: bool, creation: bool) -> Harness {
    let owner = crate::bootstrap::test_support::harness();
    let mut shell = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        if creation {
            if channel {
                "slack-list-channel-field-create"
            } else {
                "slack-list-people-field-create"
            }
        } else {
            "slack-list"
        },
    );
    if creation {
        if channel {
            crate::slack_lists::fixture::text_field_create::channel::cache::seed(&mut shell)
                .unwrap();
        } else {
            crate::slack_lists::fixture::text_field_create::people::cache::seed(&mut shell)
                .unwrap();
        }
    } else if channel {
        crate::slack_lists::fixture::add_channel_edits(&mut shell).unwrap();
    } else {
        crate::slack_lists::fixture::add_user_edits(&mut shell).unwrap();
    }
    let team = shell.core.active_team.clone().unwrap();
    let session = WorkspaceSession {
        team_id: team.clone(),
        user_id: shell.core.workspaces[&team].self_user_id.clone(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
        enterprise_id: None,
    };
    shell.core.transport = Some(owner.transport.clone());
    shell.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        unified_user_client_enabled: Default::default(),
        workspaces: [(team, session)].into(),
    });
    shell.slack_lists.scope = Some(crate::slack_lists::Scope::capture(&shell).unwrap());
    let context = shell
        .slack_lists
        .scope
        .as_ref()
        .unwrap()
        .request(&shell)
        .unwrap();
    let mut state = owner.state;
    *state.write() = shell;
    Harness {
        state,
        context,
        _owner: owner,
    }
}
pub(crate) fn row(channel: bool, number: u32, veto: bool) -> Value {
    if channel {
        serde_json::json!({"id":format!("C{number:08}"),"name":format!("late-{number}"),"is_channel":true,"is_archived":veto})
    } else {
        serde_json::json!({"id":format!("U{number:08}"),"name":format!("Late {number}"),"profile":{"display_name":format!("Late {number}")},"deleted":veto})
    }
}
pub(crate) fn members(state: Signal<ShellState>) -> Value {
    let shell = state.peek();
    let team = shell.core.active_team.as_ref().unwrap();
    let w = &shell.core.workspaces[team];
    serde_json::json!({"users":w.users,"channels":w.channels})
}
pub(crate) fn replace_client(mut state: Signal<ShellState>) {
    let mut config = super_platinum_core::slack::client::SlackClientConfig::default();
    config.xparams.version_ts = "different-fixture-client".into();
    state.write().core.client = super_platinum_core::slack::SlackClient::new(config);
}
pub(crate) fn check_request(
    request: super_platinum_core::slack::PreparedRequest,
    channel: bool,
    call: usize,
) {
    use super_platinum_core::slack::client::RequestBody;
    assert!(request.url.contains(if channel {
        "/channels/search"
    } else {
        "/users/search"
    }));
    let RequestBody::Json(body) = request.body else {
        panic!("JSON")
    };
    assert_eq!(body["count"], 30);
    match call {
        0 => assert!(body.get("marker").is_none()),
        1 => assert_eq!(body["marker"], "next"),
        _ => panic!("unexpected refill"),
    }
}
