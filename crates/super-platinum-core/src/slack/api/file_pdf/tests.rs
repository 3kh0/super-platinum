use super::*;
use serde_json::json;
#[test]
fn isolated_pdf_exact_purpose_requests_and_ip_require_literal_original_success() {
    let client = SlackClient::default();
    let mut w = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    };
    let metadata = Request::metadata(&client, &w, "F12345678").unwrap();
    assert_eq!(metadata.inner.method, "POST");
    assert!(!metadata.is_ip());
    assert_eq!(metadata.file_id(), "F12345678");
    let RequestBody::Form(fields) = &metadata.inner.body else {
        panic!()
    };
    assert_eq!(
        fields,
        &vec![
            ("token".into(), "fixture-only".into()),
            ("file".into(), "F12345678".into()),
            ("count".into(), "1".into()),
            ("page".into(), "1".into()),
            ("truncate".into(), "false".into())
        ]
    );
    let ip = Request::ip(&client, &w, "F12345678").unwrap();
    assert!(ip.is_ip());
    assert_eq!(ip.file_id(), "F12345678");
    assert!(
        ip.inner
            .url
            .contains("/api/teams.authorized.ip.range.checkIp?")
    );
    assert!(
        !ip.inner
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
    );
    let RequestBody::Form(fields) = &ip.inner.body else {
        panic!()
    };
    assert_eq!(
        fields,
        &vec![
            ("token".into(), "fixture-only".into()),
            ("file".into(), "F12345678".into())
        ]
    );
    assert!(ip_allowed(&json!({"ok":true,"ip_allowed":true})));
    for raw in [
        json!({}),
        json!({"ok":true}),
        json!({"ok":false,"ip_allowed":true}),
        json!({"ok":true,"ip_allowed":1}),
        json!({"ok":true,"ip_allowed":"true"}),
        json!({"ok":true,"ip_allowed":true,"error":null}),
        json!({"ok":true,"ip_allowed":true,"error":"denied"}),
    ] {
        assert!(!ip_allowed(&raw));
    }
    for id in ["", "Fbad", "C12345678", "F1234567890123456", "F123/5678"] {
        assert!(Request::metadata(&client, &w, id).is_err());
    }
    for url in [
        "https://foreign.example",
        "http://fixture.slack.com",
        "https://u@fixture.slack.com",
        "https://fixture.slack.com/other",
    ] {
        w.url = url.into();
        assert!(Request::ip(&client, &w, "F12345678").is_err());
    }
    w.url = "https://fixture.slack.com".into();
    w.token = " ".into();
    assert!(Request::metadata(&client, &w, "F12345678").is_err());
}
