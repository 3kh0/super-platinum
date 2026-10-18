use super::*;
use serde_json::json;
fn workspace(url: &str, enterprise: bool) -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: enterprise.then(|| "E1".into()),
        user_id: "U1".into(),
        name: "Synthetic".into(),
        url: url.into(),
        token: "synthetic-not-a-token".into(),
    }
}
fn observe(url: &str, enterprise: bool) -> Result<NativeContext, Unavailable> {
    NativeContext::observe(
        &SlackClient::default(),
        &workspace(url, enterprise),
        &json!("on"),
        &json!("off"),
    )
}
#[test]
fn known_native_production_and_enterprise_routes_are_observed() {
    let result = observe("https://team.slack.com", false).unwrap();
    assert_eq!(result.context().app(), "A04T99UKKQE");
    assert_eq!(result.request_origin(), "https://team.slack.com");
    let enterprise = observe("https://team.slack.com/", true).unwrap();
    assert_eq!(
        enterprise.request_origin(),
        "https://team.enterprise.slack.com"
    );
    assert_eq!(enterprise.configured_url(), "https://team.slack.com/");
    assert_eq!(enterprise.context().app(), "A04T99UKKQE");
    assert!(observe("https://team.enterprise.slack.com", true).is_ok());
}
#[test]
fn known_gov_mil_dev_and_qa_use_exact_tin_app_identity() {
    for (url, expected) in [
        ("https://team.slack-gov.com", "A1W53KSJQ"),
        ("https://team.slack.mil", "A1W53KSJQ"),
        ("https://team.dev.slack.com", "A014JPFMT8T"),
        ("https://team.slack-dev.com", "A014JPFMT8T"),
        ("https://team.slack-gov-dev.com", "A07846N4L"),
        ("https://team.slack-mil-dev.com", "A07846N4L"),
        ("https://team.qa3.slack.com", "A04T99UKKQE"),
    ] {
        assert_eq!(observe(url, false).unwrap().context().app(), expected);
    }
    assert_eq!(
        observe("https://team.slack-gov-dev.com", true)
            .unwrap()
            .context()
            .app(),
        "A07846N4L"
    );
}
#[test]
fn enterprise_dev_rewrite_disagreement_is_not_a_production_default() {
    assert!(matches!(
        observe("https://team.dev.slack.com", true),
        Err(Unavailable::EnvironmentMismatch)
    ));
    assert!(matches!(
        observe("https://team.enterprise.dev.slack.com", true),
        Ok(_)
    ));
}
#[test]
fn local_unknown_credentialed_or_overridden_services_are_explicitly_unavailable() {
    for url in [
        "http://team.slack.com",
        "https://localhost",
        "https://example.test",
        "https://slack.com.example.test",
        "https://person:password@team.slack.com",
        "https://team.slack.com:8443",
        "https://a.b.c.slack.com",
    ] {
        assert!(
            matches!(
                observe(url, false),
                Err(Unavailable::UnsupportedServiceOrigin)
            ),
            "{url}"
        );
    }
    for url in [
        "https://team.dev.slack.com?gov_slack=1",
        "https://team.slack.com?",
        "https://team.slack.com/#client",
    ] {
        assert!(
            matches!(observe(url, false), Err(Unavailable::QueryOverride)),
            "{url}"
        );
    }
    assert!(matches!(
        observe("https://app.slack.com/client/T1", false),
        Err(Unavailable::NonOriginConfiguration)
    ));
}
#[test]
fn unknown_assignments_are_not_invented_and_whole_observation_changes() {
    let w = workspace("https://team.slack.com", false);
    let c = SlackClient::default();
    assert!(matches!(
        NativeContext::observe(&c, &w, &Value::Null, &json!("off")),
        Err(Unavailable::MissingAssignments)
    ));
    let first = observe("https://team.slack.com", false).unwrap();
    let second = observe("https://team.slack.com/", false).unwrap();
    assert_eq!(first.request_origin(), second.request_origin());
    assert!(first != second);
    let changed = NativeContext::observe(&c, &w, &json!("on"), &json!("on")).unwrap();
    assert!(first != changed);
}
