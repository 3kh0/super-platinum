use super::*;
use crate::slack::client::RequestBody;
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T00000001".into(),
        enterprise_id: None,
        user_id: "U00000001".into(),
        name: "fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    }
}
fn fields(request: PreparedRequest) -> Vec<(String, String)> {
    let RequestBody::Form(v) = request.body else {
        panic!("form")
    };
    v
}
#[test]
fn actual_share_forms_only_grant_selected_access_to_one_resolved_dm() {
    let c = SlackClient::default();
    let w = workspace();
    for grant in [AssigneeGrant::Read, AssigneeGrant::Write] {
        let request = list_assignee_share(&c, &w, "F00000001", "D00000002", grant, None);
        assert!(request.url.contains("/api/files.share"));
        let f = fields(request);
        assert_eq!(
            f.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
            [
                "token",
                "files",
                "channel",
                "resharing_aware",
                "permissions",
                "skip_dlp_user_warning",
                "from_share_modal",
                "_x_reason"
            ]
        );
        assert_eq!(
            serde_json::from_str::<Value>(&f[4].1).unwrap(),
            json!([{"file_id":"F00000001","grant":grant.as_str()}])
        );
        assert_eq!(f[5].1, "false");
        assert_eq!(f[6].1, "true");
        assert_eq!(f[7].1, "share_dialog_permission_share_to_user");
    }
    let f = fields(list_assignee_open_dm(&c, &w, "U00000002"));
    assert_eq!(
        f.iter().skip(1).cloned().collect::<Vec<_>>(),
        vec![
            ("users".into(), "U00000002".into()),
            ("quiet".into(), "0".into()),
            ("return_im".into(), "1".into()),
            ("_x_reason".into(), "get_channel_id_for_sharing".into())
        ]
    );
    let f = fields(list_assignee_prefs(&c, &w, "D00000002"));
    assert_eq!(f[2].1, "client_redux_store");
}
#[test]
fn dm_resolution_cannot_substitute_a_different_person_or_channel_kind() {
    let good = json!({"ok":true,"channel":{"id":"D00000002","user":"U00000002","is_im":true,"is_ext_shared":false}});
    assert!(assignee_dm(&good, "U00000002").is_ok());
    for (key, value) in [
        ("user", json!("U00000003")),
        ("id", json!("C00000002")),
        ("is_im", json!(false)),
        ("is_ext_shared", json!("false")),
    ] {
        let mut raw = good.clone();
        raw["channel"][key] = value;
        assert!(assignee_dm(&raw, "U00000002").is_err());
    }
}
#[test]
fn external_policy_requires_user_and_team_release_and_exact_prefs() {
    let raw = json!({"ok":true,"assignments":{"lists_pilot":{"group":"on"},"lists_pilot_teams":{"group":"on"}}});
    let flags = assignee_share_experiments(&raw).unwrap();
    assert!(flags.lists_enabled);
    let channel = json!({"is_ext_shared":true});
    assert_eq!(
        assignee_share_policy(&channel, flags, false, None).unwrap(),
        AssigneeSharePolicy::Allowed
    );
    assert!(assignee_share_policy(&channel, flags, true, None).is_err());
    assert_eq!(
        assignee_share_policy(
            &channel,
            flags,
            true,
            Some(&json!({"ok":true,"prefs":{"list_share_status":"list_sharing_disabled"}}))
        )
        .unwrap(),
        AssigneeSharePolicy::Blocked
    );
    assert_eq!(
        assignee_share_policy(
            &channel,
            flags,
            true,
            Some(&json!({"ok":true,"prefs":{"can_upload_files":false}}))
        )
        .unwrap(),
        AssigneeSharePolicy::CheckUploads
    );
    let mut raw = raw;
    raw["assignments"]["lists_pilot_teams"]["group"] = json!("off");
    let flags = assignee_share_experiments(&raw).unwrap();
    assert!(!flags.lists_enabled);
    assert_eq!(
        assignee_share_policy(&channel, flags, false, None).unwrap(),
        AssigneeSharePolicy::Blocked
    );
    assert!(
        !assignee_upload_share_allowed(
            &json!({"ok":true,"can_share":false,"reason":"cannot_share_contain_file_upload"})
        )
        .unwrap()
    );
    assert!(
        assignee_upload_share_allowed(&json!({"ok":true,"can_share":false,"reason":"other"}))
            .unwrap()
    );
    assert!(assignee_upload_share_allowed(&json!({"ok":true})).is_err());
}
#[test]
fn netskope_uses_only_explicit_channel_context_under_exact_experiment() {
    let channel = json!({"context_team_id":"T00000002"});
    let flags = AssigneeShareExperiments {
        lists_enabled: true,
        context_team: false,
    };
    assert_eq!(assignee_context_team(&channel, flags).unwrap(), None);
    let flags = AssigneeShareExperiments {
        context_team: true,
        ..flags
    };
    assert_eq!(
        assignee_context_team(&channel, flags).unwrap(),
        Some("T00000002".into())
    );
    assert!(assignee_context_team(&json!({"context_team_id":false}), flags).is_err());
    assert!(assignee_share_experiments(&json!({"ok":true})).is_err());
    assert!(
        assignee_share_experiments(
            &json!({"ok":true,"assignments":{"lists_pilot":{"group":true}}})
        )
        .is_err()
    );
}
