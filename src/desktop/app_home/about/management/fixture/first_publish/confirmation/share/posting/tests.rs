use super::*;
fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TWORKFLOW".into(),
        enterprise_id: None,
        user_id: "UWORKFLOW".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-posting-token".into(),
    }
}
#[test]
fn exact_refresh_factory_refuses_generic_info_and_changed_context() {
    let client = SlackClient::default();
    let s = session();
    let actual = protocol::posting_refresh::request(&client, &s, raw::DEST).unwrap();
    let exact = protocol::posting_refresh::request(&client, &s, raw::DEST).unwrap();
    assert!(super::super::super::super::picker::same_request(&actual, &exact).is_ok());
    let generic = api::conversations_info(&client, &s, raw::DEST.into());
    assert!(super::super::super::super::picker::same_request(&actual, &generic).is_err());
    let other = protocol::posting_refresh::request(&client, &s, raw::PRIVATE).unwrap();
    assert!(super::super::super::super::picker::same_request(&actual, &other).is_err());
    let mut replaced = s;
    replaced.token = "different-synthetic-token".into();
    let replaced = protocol::posting_refresh::request(&client, &replaced, raw::DEST).unwrap();
    assert!(super::super::super::super::picker::same_request(&actual, &replaced).is_err());
    assert_eq!(
        safe_body(&actual.body),
        json!([
            ["channel", raw::DEST],
            ["_x_reason", "posting_permission_send_message_error"]
        ])
    );
}
#[test]
fn original_and_newer_responses_do_not_rename_or_create_policy_grants() {
    let old = response(false);
    let new = response(true);
    for v in [&old, &new] {
        assert_eq!(v["channel"]["id"], raw::DEST);
        assert_eq!(v["channel"]["name"], "review-destination");
        assert_eq!(v["channel"]["is_member"], true);
        assert!(v["channel"].get("can_post").is_none());
    }
    assert_eq!(old["channel"]["is_read_only"], true);
    assert_eq!(old["channel"]["is_thread_only"], false);
    assert_eq!(new["channel"]["is_read_only"], false);
    assert_eq!(new["channel"]["is_thread_only"], true);
    assert_ne!(old["channel"]["properties"], new["channel"]["properties"]);
}
#[test]
fn response_raw_is_exact_and_old_newer_are_independently_identifiable() {
    for newer in [false, true] {
        let v = response(newer);
        assert_eq!(
            protocol::posting_refresh::channel(&v, raw::DEST).unwrap(),
            v["channel"]
        );
        assert!(protocol::posting_refresh::channel(&v, raw::PRIVATE).is_err());
    }
    assert_ne!(
        response(false)["opaque_receipt"],
        response(true)["opaque_receipt"]
    );
}

#[test]
fn raw_team_plan_is_opt_in_and_missing_policy_preserves_original_input() {
    let original = json!({"id":"TWORKFLOW","name":"Home fixture"});
    for case in ["success", "read-failure", "held-newer", "retry-restricted"] {
        let mut team = original.clone();
        configure_team(&mut team, case);
        let mut expected = original.clone();
        expected["plan"] = json!("plus");
        assert_eq!(team, expected);
    }
    for case in ["missing-policy", "", "unrelated"] {
        let mut team = original.clone();
        configure_team(&mut team, case);
        assert_eq!(team, original);
    }
}

#[test]
fn actual_response_flags_require_missing_free_policy_but_paid_plan_is_resolved() {
    use super_platinum_core::block_actions::posting_policy::{Context, Observed, Unknown};
    let member = json!({"id":"UWORKFLOW","team_id":"TWORKFLOW"});
    for case in ["success", "missing-policy"] {
        let mut team = json!({"id":"TWORKFLOW","name":"Home fixture"});
        configure_team(&mut team, case);
        let context = Context {
            team_id: "TWORKFLOW",
            top_level_team_id: "TWORKFLOW",
            user_id: "UWORKFLOW",
            team: &team,
            member: &member,
            unified: false,
            manage_posting_access: None,
            channel_admin: None,
        };
        for newer in [false, true] {
            let raw = response(newer);
            let mut observed = Observed::unknown(raw::DEST.into());
            observed.observe(&raw["channel"], &context).unwrap();
            let result = observed.effective(&raw["channel"], &context);
            if case == "missing-policy" {
                assert_eq!(result, Err(Unknown::FeatureAccess));
            } else {
                let flags = result.unwrap();
                assert_eq!(flags.read_only, !newer);
                assert_eq!(flags.thread_only, newer);
                assert!(!flags.permits_root_post());
            }
        }
    }
}
