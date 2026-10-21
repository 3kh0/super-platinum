use super::*;
use serde_json::json;
fn prefs(value: Value) -> Value {
    json!({"ok":true,"prefs":{"plain_text_mode":value}})
}
fn assignments(value: Value) -> Value {
    json!({"ok":true,"assignments":{"portable_content_composer":{"group":value}}})
}
#[test]
fn mode_requires_actual_boolean_and_assignment_not_missing_defaults() {
    assert_eq!(
        decode::mode(&prefs(json!(false)), &assignments(json!("control"))).unwrap(),
        (false, false)
    );
    assert_eq!(
        decode::mode(&prefs(json!(true)), &assignments(json!("treatment"))).unwrap(),
        (true, true)
    );
    // This exact selector is treatment, not the unrelated on treatment convention.
    assert_eq!(
        decode::mode(&prefs(json!(false)), &assignments(json!("on"))).unwrap(),
        (false, false)
    );
    for raw in [
        json!({}),
        json!({"ok":true,"prefs":{}}),
        prefs(Value::Null),
        prefs(json!("false")),
        json!({"ok":false,"prefs":{"plain_text_mode":false}}),
    ] {
        assert!(decode::mode(&raw, &assignments(json!("control"))).is_err());
    }
    for raw in [
        json!({"ok":true,"assignments":{}}),
        assignments(Value::Null),
        assignments(json!(false)),
        assignments(json!("")),
        json!({"ok":true,"error":"partial","assignments":{"portable_content_composer":{"group":"control"}}}),
    ] {
        assert!(decode::mode(&prefs(json!(false)), &raw).is_err());
    }
}
#[test]
fn edit_windows_use_all_relevant_rows_and_workspace_overrides_top_level() {
    let raw = json!({"ok":true,"team_prefs":{"toplevel_prefs":{"msg_edit_window_mins":-1},"workspace_prefs":{"T1":{"msg_edit_window_mins":5},"T2":{"msg_edit_window_mins":20}}}});
    assert_eq!(
        decode::windows(&raw, "T1", &["T1".into(), "T2".into()]).unwrap(),
        [5, 20]
    );
    assert_eq!(
        decode::windows(&raw, "E1", &["E1".into(), "T2".into()]).unwrap(),
        [-1, 20]
    );
    assert!(decode::windows(&raw, "T1", &["T3".into()]).is_err());
    assert!(decode::windows(&raw, "T1", &[]).is_err());
}
#[test]
fn missing_or_malformed_edit_window_never_becomes_unlimited() {
    for window in [
        Value::Null,
        json!("-1"),
        json!(-2),
        json!(1.5),
        json!(9_007_199_254_740_992i64),
    ] {
        let raw =
            json!({"ok":true,"team_prefs":{"toplevel_prefs":{"msg_edit_window_mins":window}}});
        assert!(decode::windows(&raw, "T1", &["T1".into()]).is_err());
    }
    for raw in [
        json!({"ok":true,"team_prefs":{"toplevel_prefs":{}}}),
        json!({"ok":true,"team_prefs":{"toplevel_prefs":{"msg_edit_window_mins":-1}},"failed_extras":["team_prefs"]}),
        json!({"ok":true,"team_prefs":{"toplevel_prefs":{"msg_edit_window_mins":-1},"workspace_prefs":false}}),
    ] {
        assert!(decode::windows(&raw, "T1", &["T1".into()]).is_err());
    }
}
#[test]
fn only_exact_original_channel_response_is_accepted() {
    let row = json!({"id":"CCOMMENT01","is_file":true});
    assert_eq!(
        decode::channel(&json!({"ok":true,"channels":[row]}), "CCOMMENT01").unwrap(),
        row
    );
    for rows in [
        json!([]),
        json!([row, row]),
        json!([{"id":"COTHER01"}]),
        json!([row,{"id":"COTHER01"}]),
    ] {
        assert!(decode::channel(&json!({"ok":true,"channels":rows}), "CCOMMENT01").is_err());
    }
}
#[test]
fn policy_requests_are_read_only_existing_exact_factories() {
    use super_platinum_core::{
        config::WorkspaceSession,
        slack::{SlackClient, client::RequestBody},
    };
    let shell = super::super::tests::shell();
    let target = &shell.slack_lists.comments.entry.as_ref().unwrap().target;
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    let requests = requests(&SlackClient::default(), &session, target);
    let names = [
        "conversations.genericInfo",
        "users.prefs.get",
        "experiments.getByUser",
        "client.extras",
    ];
    for (request, name) in requests.iter().zip(names) {
        assert_eq!(request.method, "POST");
        assert_eq!(
            url::Url::parse(&request.url).unwrap().path(),
            format!("/api/{name}")
        );
    }
    let args = |request: &PreparedRequest| {
        let RequestBody::Form(fields) = &request.body else {
            panic!("form")
        };
        fields
            .iter()
            .filter(|(k, _)| k != "token")
            .cloned()
            .collect::<BTreeMap<_, _>>()
    };
    assert!(args(&requests[1]).is_empty());
    assert_eq!(
        args(&requests[3]),
        BTreeMap::from([
            ("extras".into(), "[\"team_prefs\"]".into()),
            ("_x_reason".into(), "native-own-profile-capabilities".into())
        ])
    );
}
#[test]
fn original_owner_socket_preferences_and_target_all_fence_observations() {
    let mut shell = super::super::tests::shell();
    let entry = shell.slack_lists.comments.entry.as_ref().unwrap();
    let owner = entry.owner.clone();
    let target = entry.target.clone();
    let socket = shell.core.workspaces[&owner.scope.team].rt_generation;
    let revision = crate::app_home::autocomplete::preferences_revision(&shell);
    assert!(active(&shell, &owner, &target, socket, revision));
    assert!(observes_preferences(&shell, &owner.scope.team, socket));
    assert!(!observes_preferences(&shell, "OTHER", socket));
    assert!(!active(&shell, &owner, &target, socket + 1, revision));
    assert!(!active(&shell, &owner, &target, socket, revision + 1));
    controller::close_shell(&mut shell);
    assert!(!active(&shell, &owner, &target, socket, revision));
    assert!(!observes_preferences(&shell, &owner.scope.team, socket));
}
