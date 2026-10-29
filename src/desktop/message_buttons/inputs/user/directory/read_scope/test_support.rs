use super::*;
use super_platinum_core::config::{Session, WorkspaceSession};

pub(crate) fn deadline(
    state: Signal<ShellState>,
    epoch: u64,
    team: &str,
) -> Option<std::time::Instant> {
    state
        .peek()
        .message_buttons
        .user_retry
        .get(&(epoch, team.into()))
        .copied()
}
pub(crate) fn has_budget(state: Signal<ShellState>) -> bool {
    !state.peek().message_buttons.user_retry.is_empty()
}
pub(crate) fn mount(shell: ShellState) -> crate::bootstrap::test_support::Harness {
    let owner = crate::bootstrap::test_support::harness();
    let mut state = owner.state;
    *state.write() = shell;
    owner
}
pub(crate) fn install_context(shell: &mut ShellState) {
    let team = shell.core.active_team.clone().unwrap();
    let session = WorkspaceSession {
        team_id: team.clone(),
        user_id: shell.core.workspaces[&team].self_user_id.clone(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
        enterprise_id: None,
    };
    shell.core.transport = Some(crate::bootstrap::test_support::transport("synthetic"));
    shell.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        unified_user_client_enabled: Default::default(),
        workspaces: [(team, session)].into(),
    });
}
pub(crate) fn canonical(state: Signal<ShellState>) -> Value {
    let s = state.peek();
    let w = &s.core.workspaces[s.core.active_team.as_ref().unwrap()];
    serde_json::json!({"users":w.users,"channels":w.channels})
}
pub(crate) fn row(n: usize) -> Value {
    serde_json::json!({"id":format!("U{n:08}"),"name":format!("Later {n}"),"profile":{"display_name":format!("Later {n}")}})
}
pub(crate) fn page(call: usize, raw_budget: bool) -> Value {
    if call == 0 {
        serde_json::json!({"results":vec![Value::Null;if raw_budget{20}else{1}],"next_marker":"next"})
    } else if raw_budget {
        serde_json::json!({"results":(20000000..20000020).map(row).collect::<Vec<_>>(),"next_marker":{"unused":true}})
    } else {
        serde_json::json!({"results":[row(20000000)]})
    }
}
pub(crate) fn check(request: PreparedRequest, call: usize) {
    assert!(request.url.contains("/users/search"));
    let super_platinum_core::slack::client::RequestBody::Json(body) = request.body else {
        panic!("JSON")
    };
    assert_eq!(body["count"], 24);
    assert_eq!(body["filter"], "NOT deactivated");
    assert_eq!(body["fuzz"], 1);
    assert_eq!(body["enable_workspace_ranking"], true);
    assert_eq!(body["include_profile_only_users"], true);
    match call {
        0 => assert!(body.get("marker").is_none()),
        1 => assert_eq!(body["marker"], "next"),
        _ => panic!("unexpected refill"),
    }
}

pub(crate) fn public_channel_page(call: usize, raw_budget: bool, empty: bool) -> Value {
    if empty {
        assert_eq!(call, 0);
        return serde_json::json!({"results":[]});
    }
    if call == 0 {
        serde_json::json!({"results":vec![Value::Null;if raw_budget{20}else{1}],"next_marker":"next"})
    } else {
        let rows = (20000000..if raw_budget {20000020} else {20000001})
            .map(|n|serde_json::json!({"id":format!("C{n:08}"),"name":format!("later-{n}"),"is_channel":true,"updated":1}))
            .collect::<Vec<_>>();
        if raw_budget {
            serde_json::json!({"results":rows,"next_marker":{"unused":true}})
        } else {
            serde_json::json!({"results":rows})
        }
    }
}
pub(crate) fn check_public_channel(request: &PreparedRequest, call: usize, query: &str) {
    assert!(request.url.contains("/cache/T1/channels/search"));
    let super_platinum_core::slack::client::RequestBody::Json(body) = &request.body else {
        panic!("JSON")
    };
    assert_eq!(body["query"], query);
    assert_eq!(body["count"], 24);
    assert_eq!(body["fuzz"], 1);
    assert_eq!(body["check_membership"], true);
    assert_eq!(body["include_record_channels"], true);
    assert!(body.get("allowed_internal_teams").is_none());
    assert!(
        body.get("should_allowed_internal_teams_match_any")
            .is_none()
    );
    assert!(body.get("external_team").is_none());
    match call {
        0 => assert!(body.get("marker").is_none()),
        1 => assert_eq!(body["marker"], "next"),
        _ => panic!("refill"),
    }
}
