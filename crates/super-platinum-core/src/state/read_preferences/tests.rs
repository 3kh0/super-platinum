use super::*;
use serde_json::json;

#[test]
fn boot_missing_container_never_becomes_empty_preference_authority() {
    let session = crate::config::WorkspaceSession {
        team_id: "T_READ".into(),
        enterprise_id: None,
        user_id: "U_READ".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    let missing: crate::slack::models::BootData = serde_json::from_value(json!({})).unwrap();
    assert!(
        serde_json::to_value(&missing)
            .unwrap()
            .get("prefs")
            .is_none()
    );
    let explicit: crate::slack::models::BootData =
        serde_json::from_value(json!({"prefs":{}})).unwrap();
    assert!(
        serde_json::to_value(&explicit)
            .unwrap()
            .get("prefs")
            .is_some()
    );
    let missing: crate::slack::models::BootData =
        serde_json::from_value(serde_json::to_value(&missing).unwrap()).unwrap();
    let explicit: crate::slack::models::BootData =
        serde_json::from_value(serde_json::to_value(&explicit).unwrap()).unwrap();
    let mut workspace = crate::state::Workspace::from_session(&session);
    workspace.apply_boot(missing.clone());
    assert_eq!(
        workspace.read_preferences.mode(),
        Err(ReadPreferenceError::Unavailable)
    );
    workspace.read_preferences = mode(ReadMode::LeftOffAndRead);
    workspace.apply_boot(missing);
    assert_eq!(
        workspace.read_preferences.mode(),
        Ok(ReadMode::LeftOffAndRead)
    );
    workspace.apply_boot(explicit);
    assert_eq!(
        workspace.read_preferences.mode(),
        Ok(ReadMode::NewestKeepUnseen)
    );
    assert!(workspace.read_preferences.raw().is_empty());
}

fn prefs(value: Value) -> ReadPreferences {
    ReadPreferences::from_prefs(&serde_json::from_value(value).unwrap())
}

fn mode(value: ReadMode) -> ReadPreferences {
    ReadPreferences::from_prefs(&value.patch())
}

#[test]
fn all_boolean_pairs_and_loaded_missing_keys_follow_source_selector() {
    for (immediate, oldest, expected) in [
        (true, true, ReadMode::LeftOffAndRead),
        (true, false, ReadMode::NewestAndRead),
        (false, true, ReadMode::NewestKeepUnseen),
        (false, false, ReadMode::NewestKeepUnseen),
    ] {
        assert_eq!(
            prefs(json!({"mark_msgs_read_immediately":immediate,"start_scroll_at_oldest":oldest}))
                .mode(),
            Ok(expected)
        );
    }
    assert_eq!(prefs(json!({})).mode(), Ok(ReadMode::NewestKeepUnseen));
    assert_eq!(
        prefs(json!({"start_scroll_at_oldest":true})).mode(),
        Ok(ReadMode::NewestKeepUnseen)
    );
    assert_eq!(
        prefs(json!({"mark_msgs_read_immediately":true})).mode(),
        Ok(ReadMode::NewestAndRead)
    );
}

#[test]
fn partial_events_preserve_other_field_and_do_not_authorize_old_cache() {
    let mut p = mode(ReadMode::LeftOffAndRead);
    assert!(p.apply_change("start_scroll_at_oldest", json!(false)));
    assert_eq!(p.mode(), Ok(ReadMode::NewestAndRead));
    assert!(!p.apply_change("start_scroll_at_oldest", json!(false)));
    assert!(!p.apply_change("unrelated", json!(true)));
    assert_eq!(p.raw().len(), 2);
    let mut unknown = ReadPreferences::default();
    unknown.apply_change("mark_msgs_read_immediately", json!(true));
    assert_eq!(unknown.mode(), Err(ReadPreferenceError::Unavailable));
    assert!(!unknown.mark_on_open(true, false));
}

#[test]
fn malformed_values_remain_distinct_from_missing_and_disable_read_permission() {
    for bad in [json!(null), json!("false"), json!(1), json!([]), json!({})] {
        let mut p = mode(ReadMode::NewestAndRead);
        p.apply_change("start_scroll_at_oldest", bad.clone());
        assert_eq!(p.raw()["start_scroll_at_oldest"], bad);
        assert_eq!(p.mode(), Err(ReadPreferenceError::UnsupportedValue));
        assert!(!p.mark_on_open(true, false));
        assert!(!p.should_auto_mark(ReadObservation {
            eligible: true,
            cursor_covers_latest: true,
            ..Default::default()
        }));
    }
}

#[test]
fn snapshot_round_trip_and_old_cache_preserve_authority_and_raw_shape() {
    for original in [
        ReadPreferences::default(),
        prefs(json!({})),
        prefs(json!({"mark_msgs_read_immediately":true})),
        prefs(json!({"start_scroll_at_oldest":null})),
        mode(ReadMode::LeftOffAndRead),
    ] {
        let restored: ReadPreferences =
            serde_json::from_value(serde_json::to_value(&original).unwrap()).unwrap();
        assert_eq!(restored, original);
        assert_eq!(restored.mode(), original.mode());
    }
    let old: ReadPreferences = serde_json::from_value(json!({})).unwrap();
    assert_eq!(old.mode(), Err(ReadPreferenceError::Unavailable));
    let boot: BootPrefs =
        serde_json::from_value(json!({"mark_msgs_read_immediately":true})).unwrap();
    let mut p = mode(ReadMode::LeftOffAndRead);
    p.apply_boot(&boot);
    assert_eq!(p.mode(), Ok(ReadMode::NewestAndRead));
    assert!(!p.raw().contains_key("start_scroll_at_oldest"));
}

#[test]
fn opening_explicit_then_unread_then_scrollmark_then_latest_matches_source() {
    let saved = ReadScrollmark {
        ts: "1700000000.000050",
        offset: -17.5,
    };
    let p = mode(ReadMode::LeftOffAndRead);
    assert_eq!(
        p.opening_target(Some("explicit"), true, Some("unread"), Some(saved)),
        Ok(ReadOpeningTarget::Explicit("explicit"))
    );
    assert_eq!(
        p.opening_target(None, true, Some("unread"), Some(saved)),
        Ok(ReadOpeningTarget::FirstUnreadAfter("unread"))
    );
    assert_eq!(
        p.opening_target(None, true, None, Some(saved)),
        Ok(ReadOpeningTarget::Latest)
    );
    assert_eq!(
        p.opening_target(None, false, Some("read"), Some(saved)),
        Ok(ReadOpeningTarget::Saved(saved))
    );
    assert_eq!(
        p.opening_target(None, false, Some("read"), None),
        Ok(ReadOpeningTarget::Latest)
    );
    for value in [ReadMode::NewestAndRead, ReadMode::NewestKeepUnseen] {
        assert_eq!(
            mode(value).opening_target(None, true, Some("unread"), Some(saved)),
            Ok(ReadOpeningTarget::Latest)
        );
    }
    assert_eq!(
        ReadPreferences::default().opening_target(Some("explicit"), true, None, None),
        Ok(ReadOpeningTarget::Explicit("explicit"))
    );
    assert_eq!(
        ReadPreferences::default().opening_target(None, true, None, None),
        Err(ReadPreferenceError::Unavailable)
    );
}

#[test]
fn only_renderer_finite_saved_offsets_can_be_restored() {
    let p = mode(ReadMode::LeftOffAndRead);
    for mark in [
        ReadScrollmark {
            ts: "",
            offset: 0.0,
        },
        ReadScrollmark {
            ts: "ts",
            offset: f64::NAN,
        },
        ReadScrollmark {
            ts: "ts",
            offset: f64::INFINITY,
        },
    ] {
        assert_eq!(
            p.opening_target(None, false, None, Some(mark)),
            Ok(ReadOpeningTarget::Latest)
        );
    }
}

#[test]
fn both_immediate_modes_mark_on_open_only_under_current_attention() {
    for value in [
        ReadMode::LeftOffAndRead,
        ReadMode::NewestAndRead,
        ReadMode::NewestKeepUnseen,
    ] {
        let p = mode(value);
        assert_eq!(
            p.mark_on_open(true, false),
            value != ReadMode::NewestKeepUnseen
        );
        assert!(!p.mark_on_open(false, false));
        assert!(!p.mark_on_open(true, true));
    }
}

#[test]
fn unseen_backlog_at_bottom_is_not_read_until_real_divider_visible() {
    let p = mode(ReadMode::NewestKeepUnseen);
    let mut seen = ReadObservation {
        eligible: true,
        unread_count: Some(30),
        reached_end: true,
        at_bottom: true,
        ..Default::default()
    };
    assert!(!p.should_auto_mark(seen));
    seen.divider_in_viewport = true;
    assert!(p.should_auto_mark(seen));
    seen.ghost_divider = true;
    assert!(!p.should_auto_mark(seen));
    seen.ghost_divider = false;
    seen.loading = true;
    assert!(!p.should_auto_mark(seen));
    seen.loading = false;
    seen.eligible = false;
    assert!(!p.should_auto_mark(seen));
}

#[test]
fn zero_unreads_requires_loaded_end_and_bottom_unknown_is_not_zero() {
    let p = mode(ReadMode::NewestKeepUnseen);
    let mut seen = ReadObservation {
        eligible: true,
        unread_count: Some(0),
        reached_end: true,
        at_bottom: true,
        ..Default::default()
    };
    assert!(p.should_auto_mark(seen));
    seen.unread_count = None;
    assert!(!p.should_auto_mark(seen));
    seen.unread_count = Some(0);
    seen.reached_end = false;
    assert!(!p.should_auto_mark(seen));
    seen.reached_end = true;
    seen.at_bottom = false;
    assert!(!p.should_auto_mark(seen));
}

#[test]
fn ongoing_immediate_policy_differs_from_initial_left_off_mark() {
    let mut seen = ReadObservation {
        eligible: true,
        unread_count: Some(30),
        ..Default::default()
    };
    assert!(mode(ReadMode::NewestAndRead).should_auto_mark(seen));
    assert!(!mode(ReadMode::LeftOffAndRead).should_auto_mark(seen));
    seen.latest_from_self = true;
    assert!(!mode(ReadMode::NewestKeepUnseen).should_auto_mark(seen));
    seen.channel_read = true;
    assert!(mode(ReadMode::NewestKeepUnseen).should_auto_mark(seen));
    seen.channel_read = false;
    seen.cursor_covers_latest = true;
    assert!(mode(ReadMode::NewestKeepUnseen).should_auto_mark(seen));
    seen.eligible = false;
    for value in [
        ReadMode::LeftOffAndRead,
        ReadMode::NewestAndRead,
        ReadMode::NewestKeepUnseen,
    ] {
        assert!(!mode(value).should_auto_mark(seen));
    }
}
