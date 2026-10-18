use super::*;
use crate::slack::client::RequestBody;

fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Synthetic".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-not-a-token".into(),
    }
}
fn view() -> View {
    let home = super::super::super::Home {
        channel: "D1".into(),
        app: "A1".into(),
        installed_team: "T1".into(),
        enabled: true,
        messages: true,
        view_id: Some("V1".into()),
    };
    View::decode(&home,json!({"id":"V1","type":"home","app_id":"A1","app_installed_team_id":"T1","bot_id":"B1","hash":"h1","blocks":[{"type":"actions","block_id":"block","elements":[{"type":"workflow_button","action_id":"action","text":{"type":"plain_text","text":"Run"},"workflow":{"trigger":{"url":"https://slack.com/shortcuts/Ft12345678"}}}]}]})).unwrap()
}
fn review() -> Preflight {
    let workflow = Workflow::capture(&view(), "block", "action", "U1").unwrap();
    Preflight::decode(workflow,json!({"third_party_auths":{"is_shown":true,"is_satisfied":false,"auths":[{"app_id":"A04T99UKKQE","function_id":"Fn1","provider_key":"salesforce","provider_name":"Salesforce","salesforce_org_id":"00Dtarget","valid_tokens":[{"external_token_id":"legacy","external_user_id":"old","is_default":true}]}]},"missing_context_parameters":{"is_shown":false,"context_parameters":[]},"missing_customizable_input_parameters":{"is_shown":false,"customizable_input_parameters":[]},"scopes_acknowledgements":{"is_shown":false,"bot_scopes":[],"user_scopes":[]}})).unwrap()
}
fn context(jwt: bool) -> Context {
    Context::observe(
        "https://app.slack.com/client/T1",
        &json!("on"),
        &json!(if jwt { "on" } else { "off" }),
    )
    .unwrap()
}
fn target(jwt: bool) -> Target {
    Target::capture(
        &review(),
        &view(),
        &workspace(),
        "Fn1",
        "salesforce",
        &context(jwt),
    )
    .unwrap()
}
fn fields(request: PreparedRequest) -> std::collections::BTreeMap<String, String> {
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    fields.into_iter().collect()
}
#[test]
fn source_environment_app_identity_and_unknown_observations() {
    for (url, id) in [
        ("https://app.slack.com", "A04T99UKKQE"),
        ("https://app.qa2.slack.com", "A04T99UKKQE"),
        ("https://app.dev.slack.com", "A014JPFMT8T"),
        ("https://app.slack-dev.com", "A014JPFMT8T"),
        ("https://app.slack-gov.com", "A1W53KSJQ"),
        ("https://app.slack.mil", "A1W53KSJQ"),
        ("https://app.slack-gov-dev.com", "A07846N4L"),
        ("https://app.dev.slack.com?gov_slack=1", "A07846N4L"),
        ("https://app.slack.com?gov_slack=1", "A04T99UKKQE"),
    ] {
        assert_eq!(
            Context::observe(url, &json!("on"), &json!("off"))
                .unwrap()
                .app(),
            id
        );
    }
    assert!(Context::observe("https://app.slack.com", &Value::Null, &json!("on")).is_err());
    assert!(Context::observe("https://app.slack.com", &json!(true), &json!("on")).is_err());
    assert!(Context::observe("https://app.slack.com", &json!("on"), &Value::Null).is_err());
}
#[test]
fn org_presence_never_selects_branch_and_target_is_exact() {
    let p = review();
    let auth = &p.connections()[0];
    assert!(context(false).selected(auth));
    let mut wrong = auth.clone();
    wrong.app = "A2".into();
    assert!(!context(false).selected(&wrong));
    let off = Context::observe("https://app.slack.com", &json!("off"), &json!("off")).unwrap();
    assert!(!off.selected(auth));
    assert!(Target::capture(&p, &view(), &workspace(), "Fn1", "wrong", &context(false)).is_err());
    let mut w = workspace();
    w.user_id = "U2".into();
    assert!(Target::capture(&p, &view(), &w, "Fn1", "salesforce", &context(false)).is_err());
}
#[test]
fn generated_fetcher_request_shapes_and_jwt_branch() {
    let target = target(false);
    let c = SlackClient::default();
    let initial = fields(
        target
            .request(Action::List(ListReason::Initial), &c, &workspace(), &view())
            .unwrap(),
    );
    assert_eq!(
        initial.get("_x_reason").map(String::as_str),
        Some("fetch-orgs-and-sign-in-silently-v2")
    );
    assert!(!initial.contains_key("salesforce_org_id"));
    for action in [
        Action::Silent,
        Action::Manual,
        Action::LoginUrl,
        Action::Disconnect,
    ] {
        let request = target.request(action, &c, &workspace(), &view()).unwrap();
        assert!(!request.retry_safe());
        let data = fields(request);
        assert_eq!(
            data.get("salesforce_org_id").map(String::as_str),
            Some("00Dtarget")
        );
        assert_eq!(data.contains_key("force_reauth"), action == Action::Silent);
    }
    assert!(
        super::tests::target(true)
            .request(Action::Manual, &c, &workspace(), &view())
            .is_err()
    );
}
#[test]
fn jwt_and_legacy_silent_result_and_login_decision_are_distinct() {
    let response = json!({"ok":true,"is_manual_auth_needed":false,"authorization_url":"https://login.salesforce.com/auth"});
    assert!(!target(false).silent_succeeded(&response).unwrap());
    assert!(target(true).silent_succeeded(&response).unwrap());
    assert!(matches!(
        target(false)
            .manual_response(Action::Manual, &response)
            .unwrap(),
        AuthOutcome::OpenExternal(_)
    ));
    assert!(matches!(
        target(false)
            .manual_response(
                Action::Manual,
                &json!({"ok":true,"is_manual_auth_needed":true})
            )
            .unwrap(),
        AuthOutcome::LoginUrlRequired
    ));
    assert!(
        target(true)
            .manual_response(Action::Manual, &response)
            .is_err()
    );
    assert!(
        target(false)
            .silent_succeeded(&json!({"ok":true,"is_manual_auth_needed":"no"}))
            .is_err()
    );
}
#[test]
fn external_urls_need_https_and_no_credentials() {
    for url in [
        "javascript:alert(1)",
        "http://login.salesforce.com/",
        "https://user:secret@login.salesforce.com/",
    ] {
        assert!(
            target(true)
                .manual_response(Action::LoginUrl, &json!({"ok":true,"auth_url":url}))
                .is_err()
        );
    }
}
#[test]
fn list_proof_preserves_raw_and_never_substitutes_another_org() {
    let raw = json!({"ok":true,"salesforce_orgs":[{"salesforce_org_id":"00Dother","auth":"ok"},{"salesforce_org_id":"00Dtarget","auth":"none","name":"Target","unknown":{"kept":true}}],"unknown":"kept"});
    let snapshot = target(false).organizations(raw.clone()).unwrap();
    assert_eq!(snapshot.raw(), &raw);
    assert!(!snapshot.target.as_ref().unwrap().connected());
    assert!(snapshot.target.unwrap().silent_candidate());
    assert!(
        target(false)
            .organizations(
                json!({"ok":true,"salesforce_orgs":[{"salesforce_org_id":"00Dother","auth":"ok"}]})
            )
            .unwrap()
            .target
            .is_none()
    );
    assert!(target(false).organizations(json!({"ok":true,"salesforce_orgs":[{"salesforce_org_id":"00Dtarget","auth":"ok"},{"salesforce_org_id":"00Dtarget","auth":"ok"}]})).is_err());
    let disabled=target(false).organizations(json!({"ok":true,"salesforce_orgs":[{"salesforce_org_id":"00Dtarget","auth":"auto_auth_error","auth_failure_reason":"user_mapping_disabled"}]})).unwrap();
    assert!(!disabled.target.unwrap().silent_candidate());
}
#[test]
fn actual_virtual_event_maps_and_disconnect_restores_ordinary_default() {
    let mut p = review();
    let t = target(false);
    let event = json!({"type":"salesforce_org_authed","salesforce_org_id":"00Dtarget","app_id":"A04T99UKKQE","provider_key":"salesforce","virtual_auth_token":"sat_real","username":"person@example.test","auth":"ok"});
    for (key, value) in [
        ("salesforce_org_id", "00Dother"),
        ("app_id", "A2"),
        ("provider_key", "other"),
        ("user_id", "U2"),
    ] {
        let mut wrong = event.clone();
        wrong[key] = json!(value);
        assert!(
            !p.salesforce_event(&t, &wrong, &view(), &workspace())
                .unwrap()
        );
    }
    assert!(
        p.salesforce_event(&t, &event, &view(), &workspace())
            .unwrap()
    );
    assert_eq!(p.connections()[0].tokens[0].id, "sat_real");
    assert!(p.connections()[0].tokens[0].default);
    assert!(!p.connections()[0].tokens[1].default);
    assert!(
        p.salesforce_event(
            &t,
            &json!({"type":"salesforce_org_disconnected","salesforce_org_id":"00Dtarget"}),
            &view(),
            &workspace()
        )
        .unwrap()
    );
    assert_eq!(p.connections()[0].tokens.len(), 1);
    assert_eq!(p.connections()[0].tokens[0].id, "legacy");
    assert!(p.connections()[0].tokens[0].default);
}
