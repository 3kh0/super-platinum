use super::*;
use crate::slack::client::RequestBody;
use serde_json::json;

#[test]
fn closed_saved_file_sort_uses_exact_raw_scalar_key_and_api_contract_without_generic_pref_access() {
    let session = WorkspaceSession {
        team_id: "TONE".into(),
        user_id: "UONE".into(),
        name: "Synthetic".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-token".into(),
        enterprise_id: None,
    };
    let client = SlackClient::default();
    for (sort, expected) in [
        (Sort::Relevant, "score"),
        (Sort::Oldest, "timestamp_asc"),
        (Sort::Newest, "created"),
        (Sort::Name, "name"),
        (Sort::NameDescending, "name_desc"),
    ] {
        let request = request(&client, &session, sort).unwrap();
        assert_eq!(request.submitted_sort(), sort);
        let url = url::Url::parse(&request.inner.url).unwrap();
        assert_eq!(
            (url.host_str(), url.path()),
            (Some("fixture.slack.com"), "/api/users.prefs.set")
        );
        assert_eq!(request.inner.method, "POST");
        assert!(
            !request
                .inner
                .headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        );
        let RequestBody::Form(fields) = request.inner.body else {
            panic!("closed text form")
        };
        assert_eq!(
            fields,
            vec![
                ("token".into(), "synthetic-token".into()),
                ("name".into(), "search_file_sort".into()),
                ("value".into(), expected.into()),
                ("_x_reason".into(), "prefs-api/setUserPrefByApi".into())
            ]
        );
        assert!(!fields[2].1.starts_with('"'), "not a JSON-quoted string");
    }
    for endpoint in [
        "https://fixture.slack.com.attacker.test",
        "http://fixture.slack.com",
        "https://user:pass@fixture.slack.com",
        "https://fixture.slack.com:444",
    ] {
        let mut hostile = session.clone();
        hostile.url = endpoint.into();
        assert!(request(&client, &hostile, Sort::Name).is_err());
    }
    assert!(
        validate_response(&json!({"ok":true,"value":"untrusted different value"})).is_ok(),
        "response value is not a canonical preference observation"
    );
    for value in [
        json!({}),
        json!({"ok":"true"}),
        json!({"ok":false,"error":"invalid_auth"}),
        json!({"ok":true,"error":"rejected"}),
    ] {
        assert!(validate_response(&value).is_err());
    }
    assert!(matches!(
        validate_response(&json!({"ok":false,"error":"ratelimited"})),
        Err(Error::RateLimited { .. })
    ));
    assert!(
        matches!(validate_response(&json!({"ok":false,"error":"https://secret.invalid/token"})),Err(Error::Api(code)) if code=="file_sort_save_failed")
    );
    let ordinary = client.rest_form(&session, "users.info", vec![("user", "UOTHER".into())]);
    assert!(
        ordinary
            .headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("content-type")
                && v == "application/x-www-form-urlencoded")
    );
}
