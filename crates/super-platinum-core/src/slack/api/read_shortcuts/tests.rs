use super::*;
use crate::slack::client::RequestBody;
use serde_json::json;

fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T_FIXTURE".into(),
        enterprise_id: None,
        user_id: "U_FIXTURE".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    }
}
fn owner() -> ReadShortcutOwner {
    ReadShortcutOwner::capture(7, &workspace()).unwrap()
}
fn fields(request: PreparedRequest) -> Vec<(String, String)> {
    let RequestBody::Form(fields) = request.body else {
        panic!("expected form")
    };
    fields
}

#[test]
fn bulk_clear_has_no_channel_list_enterprise_or_invented_scope_parameters() {
    let request = BulkReadIntent::clear(owner())
        .request(&SlackClient::default(), &workspace(), 7)
        .unwrap();
    assert_eq!(
        url::Url::parse(&request.url).unwrap().path(),
        "/api/users.markAllRead"
    );
    assert_eq!(
        fields(request),
        vec![
            ("token".into(), "synthetic".into()),
            ("_x_reason".into(), "ShiftEscapeConfirmDialog".into())
        ]
    );
}

#[test]
fn receipt_undo_preserves_exact_timestamp_and_owner() {
    let receipt =
        BulkReadReceipt::parse(owner(), &json!({"ok":true,"undo_ts":"1791884254.000001"})).unwrap();
    assert!(receipt.has_undo());
    let undo = receipt.into_undo().unwrap();
    assert_eq!(undo.owner(), &owner());
    assert_eq!(
        fields(
            undo.request(&SlackClient::default(), &workspace(), 7)
                .unwrap()
        ),
        vec![
            ("token".into(), "synthetic".into()),
            ("undo_ts".into(), "1791884254.000001".into()),
            ("_x_reason".into(), "UndoShiftEscapeMarkAllRead".into())
        ]
    );
    assert!(
        BulkReadReceipt::parse(owner(), &json!({"ok":true}))
            .unwrap()
            .into_undo()
            .is_none()
    );
}

#[test]
fn clear_and_undo_reject_new_account_workspace_user_and_route() {
    for change in 0..5 {
        let mut session = workspace();
        let epoch = if change == 0 { 8 } else { 7 };
        match change {
            1 => session.team_id = "T_OTHER".into(),
            2 => session.user_id = "U_OTHER".into(),
            3 => session.url = "https://other.slack.com".into(),
            4 => session.enterprise_id = Some("E_OTHER".into()),
            _ => {}
        }
        assert!(
            BulkReadIntent::clear(owner())
                .request(&SlackClient::default(), &session, epoch)
                .is_err()
        );
        let undo = BulkReadReceipt::parse(owner(), &json!({"ok":true,"undo_ts":"10.000001"}))
            .unwrap()
            .into_undo()
            .unwrap();
        assert!(
            undo.request(&SlackClient::default(), &session, epoch)
                .is_err()
        );
    }
}

#[test]
fn unsupported_enterprise_and_untrusted_routes_fail_before_preparation() {
    let mut session = workspace();
    session.enterprise_id = Some("E_FIXTURE".into());
    assert!(ReadShortcutOwner::capture(7, &session).is_err());
    session.enterprise_id = None;
    for url in [
        "http://fixture.slack.com",
        "https://slack.com.attacker.test",
        "https://fixture.slack.com?team=T_OTHER",
        "https://user@fixture.slack.com",
        "https://fixture.slack.com/client",
    ] {
        session.url = url.into();
        assert!(ReadShortcutOwner::capture(7, &session).is_err(), "{url}");
    }
}

#[test]
fn only_clean_known_denial_is_definite_and_contradictory_receipts_stay_unknown() {
    assert!(matches!(
        bulk_read_success(&json!({"ok":false,"error":"invalid_auth"})),
        Err(Error::Api(_))
    ));
    for response in [
        json!({"ok":false,"error":"invalid_auth","undo_ts":"10.0"}),
        json!({"ok":false,"error":"invalid_auth","undo_ts":null}),
        json!({"ok":false,"error":"invalid_auth","receipt":"mutated"}),
        json!({"ok":true,"error":"invalid_auth"}),
        json!({"ok":false,"error":"new_error"}),
        json!({"ok":false,"error":"invalid_auth","response_metadata":{"receipt":"mutated"}}),
        json!({"ok":true,"undo_ts":10}),
        json!({"ok":true,"undo_ts":"NaN"}),
        json!({"ok":true,"undo_ts":""}),
        json!({"ok":true,"updated":true}),
        json!({}),
    ] {
        assert!(
            matches!(bulk_read_success(&response), Err(Error::Transport(_))),
            "{response}"
        );
    }
}

#[test]
fn positive_ack_without_undo_is_valid_but_does_not_manufacture_an_undo() {
    for response in [
        json!({"ok":true}),
        json!({"ok":true,"undo_ts":null}),
        json!({"ok":true,"response_metadata":{"warnings":[]}}),
    ] {
        assert!(
            !BulkReadReceipt::parse(owner(), &response)
                .unwrap()
                .has_undo()
        );
    }
}

#[test]
fn confirmation_get_and_false_set_are_independent_of_reading_mode_keys() {
    let client = SlackClient::default();
    let pref = parse_clear_unreads_preference(
        &json!({"ok":true,"prefs":{"confirm_clear_all_unreads":true}}),
    )
    .unwrap();
    assert_eq!(
        fields(clear_unreads_preference_request(&client, &workspace())),
        vec![
            ("token".into(), "synthetic".into()),
            ("prefs".into(), CONFIRM_CLEAR_UNREADS.into())
        ]
    );
    assert_eq!(
        fields(
            disable_clear_unreads_confirmation_request(
                &client,
                &workspace(),
                7,
                &owner(),
                &pref,
                &pref
            )
            .unwrap()
        ),
        vec![
            ("token".into(), "synthetic".into()),
            ("name".into(), CONFIRM_CLEAR_UNREADS.into()),
            ("value".into(), "false".into()),
            ("_x_reason".into(), "prefs-api/setUserPrefByApi".into())
        ]
    );
}

#[test]
fn confirmation_parser_requires_envelope_but_retains_malformed_authority() {
    for response in [
        json!({"ok":true}),
        json!({"ok":true,"prefs":null}),
        json!({"ok":false,"prefs":{}}),
    ] {
        assert!(parse_clear_unreads_preference(&response).is_err());
    }
    let missing = parse_clear_unreads_preference(&json!({"ok":true,"prefs":{}})).unwrap();
    assert_eq!(missing.requires_confirmation(), Ok(false));
    let malformed = parse_clear_unreads_preference(
        &json!({"ok":true,"prefs":{"confirm_clear_all_unreads":"false"}}),
    )
    .unwrap();
    assert!(malformed.loaded());
    assert!(malformed.requires_confirmation().is_err());
    assert!(
        disable_clear_unreads_confirmation_request(
            &SlackClient::default(),
            &workspace(),
            7,
            &owner(),
            &malformed,
            &malformed
        )
        .is_err()
    );
}

#[test]
fn preference_write_rejects_raw_baseline_change_even_if_both_effectively_false() {
    let absent = parse_clear_unreads_preference(&json!({"ok":true,"prefs":{}})).unwrap();
    let present = parse_clear_unreads_preference(
        &json!({"ok":true,"prefs":{"confirm_clear_all_unreads":false}}),
    )
    .unwrap();
    assert!(
        disable_clear_unreads_confirmation_request(
            &SlackClient::default(),
            &workspace(),
            7,
            &owner(),
            &absent,
            &present
        )
        .is_err()
    );
}

#[test]
fn uncertain_network_failures_never_authorize_replay() {
    assert!(read_shortcut_definitely_rejected(&Error::Offline(
        "not sent".into()
    )));
    assert!(read_shortcut_definitely_rejected(&Error::Api(
        "invalid_auth".into()
    )));
    for error in [
        Error::Transport("lost".into()),
        Error::RateLimited {
            retry_after_secs: Some(7),
        },
        Error::Api("fatal_error".into()),
    ] {
        assert!(!read_shortcut_definitely_rejected(&error));
    }
}
