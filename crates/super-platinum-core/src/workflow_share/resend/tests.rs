use super::*;
fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-token".into(),
    }
}
fn message() -> Message {
    Message::new(
        &json!({"shortcut_url":"https://slack.com/shortcuts/FtRETRY"}),
        "Original rich text",
        "CDEST0001",
        "38126951-e78d-4b54-b3e4-5dc77d41f906",
        "1781524900.000001",
    )
    .unwrap()
}
#[test]
fn backend_exclusions_are_typed_and_unknown_requires_host_row_authority() {
    for code in SUPPRESSED {
        assert!(!eligible(Some(&json!({"ok":false,"error":code}))));
    }
    for code in [
        json!(6),
        json!(7),
        json!(8),
        json!(6.0),
        json!(7.0),
        json!(8.0),
    ] {
        assert!(!eligible(Some(&json!({"ok":false,"error":code}))));
    }
    for raw in [
        json!({"ok":false,"error":"invalid_auth"}),
        json!({"ok":false,"error":"6"}),
        json!({"ok":false,"error":9}),
        json!({"error":null}),
    ] {
        assert!(eligible(Some(&raw)));
    }
    assert!(eligible(None));
    assert!(!eligible(Some(&json!({"ok":true}))));
    assert!(!eligible(Some(&json!({"ok":false,"error":{}}))));
    assert!(!eligible(Some(
        &json!({"ok":false,"dlp_custom_message":"review"})
    )));
}
#[test]
fn retry_preserves_identity_rich_content_and_explicit_source_row_metadata() {
    let client = SlackClient::default();
    let message = message();
    let initial = message.request(&client, &session()).unwrap();
    let retry = message
        .resend_request(
            &client,
            &session(),
            Some(&json!({"ok":false,"error":"not_allowed"})),
        )
        .unwrap();
    assert!(retry.message() == initial.message());
    let initial_fields = initial.inspect()["fields"].as_array().unwrap().clone();
    let fields = retry.inspect()["fields"].as_array().unwrap().clone();
    assert_eq!(&fields[..initial_fields.len()], initial_fields);
    assert_eq!(
        &fields[initial_fields.len()..],
        json!([
            ["user", "U1"],
            ["showResend", "true"],
            ["showCancel", "true"],
            ["is_pending", "true"],
            ["isEKMError", "false"],
            ["isDLPUserWarningError", "false"],
            ["skip_dlp_user_warning", "false"]
        ])
        .as_array()
        .unwrap()
    );
    assert!(!fields.iter().any(|p| matches!(
        p[0].as_str(),
        Some("text" | "attachments" | "messageSendTrace")
    )));
    assert!(initial.fields_current());
    assert!(retry.fields_current());
    assert!(
        message
            .resend_request(&client, &session(), None)
            .unwrap()
            .fields_current()
    );
}
#[test]
fn closed_variants_reject_cross_shape_and_changed_retry_metadata() {
    let client = SlackClient::default();
    let message = message();
    let mut initial = message.request(&client, &session()).unwrap();
    let mut retry = message.resend_request(&client, &session(), None).unwrap();
    let retry_body = retry.request.body.clone();
    initial.request.body = retry_body;
    assert!(!initial.fields_current());
    let RequestBody::Form(fields) = &mut retry.request.body else {
        panic!("form")
    };
    fields
        .iter_mut()
        .find(|(k, _)| k == "skip_dlp_user_warning")
        .unwrap()
        .1 = "true".into();
    assert!(!retry.fields_current());
    assert!(
        message
            .resend_request(
                &client,
                &session(),
                Some(&json!({"ok":false,"error":"dlp_user_warning"}))
            )
            .is_err()
    );
}
