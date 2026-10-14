use super::*;
use crate::slack::client::RequestBody;
use serde_json::json;
use std::collections::BTreeMap;

fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T_READ".into(),
        enterprise_id: None,
        user_id: "U_READ".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    }
}

fn snapshot(prefs: Value) -> ReadPreferences {
    parse_read_preferences(&json!({"ok":true,"prefs":prefs})).unwrap()
}

#[test]
fn get_selects_only_both_read_preferences() {
    let request = read_preferences_request(&SlackClient::default(), &workspace());
    assert_eq!(request.method, "POST");
    assert_eq!(
        url::Url::parse(&request.url).unwrap().path(),
        "/api/users.prefs.get"
    );
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "synthetic".into()),
            (
                "prefs".into(),
                "mark_msgs_read_immediately,start_scroll_at_oldest".into()
            ),
        ]
    );
}

#[test]
fn every_radio_choice_sends_exact_full_boolean_patch() {
    let baseline = snapshot(json!({}));
    for desired in [
        ReadMode::LeftOffAndRead,
        ReadMode::NewestAndRead,
        ReadMode::NewestKeepUnseen,
    ] {
        let intent = ReadPreferenceIntent::capture(&workspace(), &baseline, desired).unwrap();
        let request = read_preferences_update_request(
            &SlackClient::default(),
            &workspace(),
            &intent,
            &baseline,
        )
        .unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(
            url::Url::parse(&request.url).unwrap().path(),
            "/api/users.prefs.set"
        );
        let RequestBody::Form(fields) = request.body else {
            panic!("form")
        };
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0], ("token".into(), "synthetic".into()));
        assert_eq!(fields[1].0, "prefs");
        assert_eq!(
            fields[2],
            (
                "_x_reason".into(),
                "prefs-api/setMultipleUserPrefsByApi".into()
            )
        );
        let patch: BTreeMap<String, Value> = serde_json::from_str(&fields[1].1).unwrap();
        assert_eq!(patch, desired.patch());
        assert!(patch.values().all(Value::is_boolean));
    }
}

#[test]
fn invalid_envelopes_are_not_preference_authority() {
    for bad in [
        json!({"prefs":{}}),
        json!({"ok":false,"prefs":{}}),
        json!({"ok":true,"error":"ratelimited","prefs":{}}),
        json!({"ok":true}),
        json!({"ok":true,"prefs":null}),
        json!({"ok":true,"prefs":[]}),
    ] {
        assert!(parse_read_preferences(&bad).is_err(), "{bad}");
    }
    assert_eq!(snapshot(json!({})).mode(), Ok(ReadMode::NewestKeepUnseen));
}

#[test]
fn positive_malformed_snapshot_revokes_old_permission_and_cannot_build_mutation() {
    let mut current = snapshot(json!({"mark_msgs_read_immediately":true}));
    assert!(current.mark_on_open(true, false));
    current = snapshot(json!({"mark_msgs_read_immediately":"false","start_scroll_at_oldest":null}));
    assert_eq!(current.raw()["mark_msgs_read_immediately"], "false");
    assert_eq!(current.raw()["start_scroll_at_oldest"], Value::Null);
    assert!(current.mode().is_err());
    assert!(!current.mark_on_open(true, false));
    assert!(
        ReadPreferenceIntent::capture(&workspace(), &current, ReadMode::NewestAndRead).is_err()
    );
}

#[test]
fn unrelated_preferences_do_not_expand_write_scope_or_block_revalidation() {
    let baseline = snapshot(json!({"mark_msgs_read_immediately":true,"unrelated":"before"}));
    let fresh = snapshot(json!({"mark_msgs_read_immediately":true,"unrelated":"after"}));
    let intent =
        ReadPreferenceIntent::capture(&workspace(), &baseline, ReadMode::NewestKeepUnseen).unwrap();
    assert!(intent.revalidate(&fresh).is_ok());
    assert_eq!(baseline.raw().len(), 1);
}

#[test]
fn raw_change_with_same_effective_mode_rejects_stale_preflight() {
    let baseline =
        snapshot(json!({"mark_msgs_read_immediately":false,"start_scroll_at_oldest":true}));
    let fresh =
        snapshot(json!({"mark_msgs_read_immediately":false,"start_scroll_at_oldest":false}));
    assert_eq!(baseline.mode(), fresh.mode());
    let intent =
        ReadPreferenceIntent::capture(&workspace(), &baseline, ReadMode::NewestAndRead).unwrap();
    assert!(intent.revalidate(&fresh).is_err());
    assert!(
        read_preferences_update_request(&SlackClient::default(), &workspace(), &intent, &fresh)
            .is_err()
    );
}

#[test]
fn owner_change_and_unloaded_baseline_cannot_build_mutation() {
    let baseline = snapshot(json!({}));
    let intent =
        ReadPreferenceIntent::capture(&workspace(), &baseline, ReadMode::NewestAndRead).unwrap();
    for (team, user) in [("T_OTHER", "U_READ"), ("T_READ", "U_OTHER")] {
        let mut changed = workspace();
        changed.team_id = team.into();
        changed.user_id = user.into();
        assert!(
            read_preferences_update_request(&SlackClient::default(), &changed, &intent, &baseline)
                .is_err()
        );
    }
    assert!(
        ReadPreferenceIntent::capture(
            &workspace(),
            &ReadPreferences::default(),
            ReadMode::NewestAndRead
        )
        .is_err()
    );
    assert!(intent.revalidate(&ReadPreferences::default()).is_err());
}

#[test]
fn receipt_requires_both_exact_booleans_not_just_the_same_mode() {
    let baseline = snapshot(json!({"mark_msgs_read_immediately":true}));
    let intent =
        ReadPreferenceIntent::capture(&workspace(), &baseline, ReadMode::NewestKeepUnseen).unwrap();
    assert!(!intent.confirms(&snapshot(json!({}))));
    assert!(!intent.confirms(&snapshot(json!({"mark_msgs_read_immediately":false}))));
    assert!(!intent.confirms(&snapshot(
        json!({"mark_msgs_read_immediately":false,"start_scroll_at_oldest":true})
    )));
    assert!(intent.confirms(&snapshot(
        json!({"mark_msgs_read_immediately":false,"start_scroll_at_oldest":false})
    )));
}
