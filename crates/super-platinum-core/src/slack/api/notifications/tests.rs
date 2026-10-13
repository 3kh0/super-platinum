use super::*;
use crate::slack::client::RequestBody;
pub(super) fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    }
}
#[test]
fn mute_patch_names_only_target_and_string_boolean_without_expiry_clear() {
    for muted in [false, true] {
        let request =
            channel_mute_request(&SlackClient::default(), &workspace(), "C1", muted).unwrap();
        assert_eq!(
            url::Url::parse(&request.url).unwrap().path(),
            "/api/users.prefs.setNotifications"
        );
        let RequestBody::Form(fields) = request.body else {
            panic!("form")
        };
        let fields: BTreeMap<_, _> = fields.into_iter().collect();
        assert_eq!(fields.len(), 5); // auth plus exactly the four source arguments
        assert_eq!(fields["global"], "false");
        assert_eq!(fields["sync"], "false");
        assert_eq!(fields["channel_ids"], "C1");
        assert_eq!(
            serde_json::from_str::<Value>(&fields["prefs"]).unwrap(),
            json!([{"name":"muted","value":muted.to_string()}])
        );
    }
    for id in ["", "C", "C1,C2", "C1/other", "c1", "U1"] {
        assert!(channel_mute_request(&SlackClient::default(), &workspace(), id, true).is_err());
    }
    let read = notification_preferences_request(&SlackClient::default(), &workspace());
    assert_eq!(
        url::Url::parse(&read.url).unwrap().path(),
        "/api/users.prefs.get"
    );
    let RequestBody::Form(fields) = read.body else {
        panic!("form")
    };
    assert_eq!(
        fields.iter().find(|(k, _)| k == "prefs").unwrap().1,
        "all_notifications_prefs"
    );
}
#[test]
fn canonical_read_preserves_unknown_and_other_channel_prefs_and_temporary_state() {
    let raw = json!({"global":{"unknown":{"nested":true}},"channels":{"C1":{"muted":false,"temp_mute_expiration_ts":"200","desktop":"mentions_dms"},"C2":{"muted":true,"unknown":[1,2]}},"future":["preserved"]});
    for wire in [raw.clone(), Value::String(raw.to_string())] {
        let snapshot = parse_notification_preferences(
            &json!({"ok":true,"prefs":{"all_notifications_prefs":wire}}),
        )
        .unwrap();
        assert_eq!(snapshot.raw(), &raw);
        let state = snapshot.channel_state("C1").unwrap();
        assert!(!state.muted);
        assert!(state.effective_muted(199));
        assert!(!state.effective_muted(200));
        assert!(snapshot.channel_state("C2").unwrap().effective_muted(500));
        assert!(!snapshot.channel_state("C3").unwrap().effective_muted(0));
    }
}
#[test]
fn malformed_or_missing_preferences_cannot_prove_unmuted() {
    for response in [
        json!({}),
        json!({"ok":true}),
        json!({"ok":true,"prefs":{"all_notifications_prefs":"broken"}}),
        json!({"ok":true,"prefs":{"all_notifications_prefs":{"channels":null}}}),
        json!({"ok":true,"prefs":{"all_notifications_prefs":{"channels":{},"global":false}}}),
    ] {
        assert!(parse_notification_preferences(&response).is_err());
    }
    for prefs in [
        json!({"muted":"false"}),
        json!({"muted":null}),
        json!({"temp_mute_expiration_ts":"bad"}),
        json!({"temp_mute_expiration_ts":-1}),
        json!([]),
    ] {
        let snapshot = parse_notification_preferences(
            &json!({"ok":true,"prefs":{"all_notifications_prefs":{"channels":{"C1":prefs}}}}),
        )
        .unwrap();
        assert!(snapshot.channel_state("C1").is_err());
    }
    assert!(
        matches!(parse_notification_preferences(&json!({"ok":false,"error":"not_authed"})),Err(Error::Api(code)) if code=="not_authed")
    );
}
