use super::*;
use serde_json::json;

fn preference(value: Value) -> ClearUnreadsPreference {
    ClearUnreadsPreference::from_prefs(&serde_json::from_value(value).unwrap())
}

#[test]
fn workspace_boot_preserves_absent_container_but_adopts_explicit_empty_authority() {
    let session = crate::config::WorkspaceSession {
        team_id: "T_SHORTCUTS".into(),
        enterprise_id: None,
        user_id: "U_SHORTCUTS".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    let mut workspace = crate::state::Workspace::from_session(&session);
    let missing: crate::slack::models::BootData = serde_json::from_value(json!({})).unwrap();
    let missing: crate::slack::models::BootData =
        serde_json::from_value(serde_json::to_value(missing).unwrap()).unwrap();
    workspace.apply_boot(missing.clone());
    assert_eq!(
        workspace.clear_unreads_preference.requires_confirmation(),
        Err(ClearUnreadsPolicyError::Unavailable)
    );
    workspace.clear_unreads_preference = preference(json!({CONFIRM_CLEAR_UNREADS:true}));
    workspace.apply_boot(missing);
    assert_eq!(
        workspace.clear_unreads_preference.requires_confirmation(),
        Ok(true)
    );
    workspace.apply_boot(serde_json::from_value(json!({"prefs":{}})).unwrap());
    assert_eq!(
        workspace.clear_unreads_preference.requires_confirmation(),
        Ok(false)
    );
    workspace.apply_boot(
        serde_json::from_value(json!({"prefs":{"confirm_clear_all_unreads":"false"}})).unwrap(),
    );
    assert_eq!(
        workspace.clear_unreads_preference.requires_confirmation(),
        Err(ClearUnreadsPolicyError::UnsupportedValue)
    );
}

#[test]
fn missing_complete_pref_is_falsy_but_unknown_and_partial_are_not_authority() {
    let mut pref = ClearUnreadsPreference::default();
    assert_eq!(
        pref.requires_confirmation(),
        Err(ClearUnreadsPolicyError::Unavailable)
    );
    pref.apply_change(CONFIRM_CLEAR_UNREADS, json!(false));
    assert_eq!(
        pref.requires_confirmation(),
        Err(ClearUnreadsPolicyError::Unavailable)
    );
    assert_eq!(preference(json!({})).requires_confirmation(), Ok(false));
    assert_eq!(
        preference(json!({CONFIRM_CLEAR_UNREADS:true})).requires_confirmation(),
        Ok(true)
    );
}

#[test]
fn malformed_updates_revoke_policy_and_unrelated_changes_are_ignored() {
    let mut pref = preference(json!({CONFIRM_CLEAR_UNREADS:false}));
    assert!(!pref.apply_change("start_scroll_at_oldest", json!(true)));
    for raw in [json!(null), json!("false"), json!(1), json!({}), json!([])] {
        pref.apply_change(CONFIRM_CLEAR_UNREADS, raw.clone());
        assert_eq!(pref.raw().get(CONFIRM_CLEAR_UNREADS), Some(&raw));
        assert_eq!(
            pref.requires_confirmation(),
            Err(ClearUnreadsPolicyError::UnsupportedValue)
        );
    }
}

#[test]
fn cache_roundtrip_preserves_unknown_missing_and_explicit_null() {
    for pref in [
        ClearUnreadsPreference::default(),
        preference(json!({})),
        preference(json!({CONFIRM_CLEAR_UNREADS:null})),
    ] {
        let back: ClearUnreadsPreference =
            serde_json::from_value(serde_json::to_value(&pref).unwrap()).unwrap();
        assert_eq!(pref, back);
        assert_eq!(pref.requires_confirmation(), back.requires_confirmation());
    }
    let old: ClearUnreadsPreference = serde_json::from_value(json!({})).unwrap();
    assert!(!old.loaded());
}

#[test]
fn channel_latest_uses_metadata_and_excludes_pending_or_malformed_rows() {
    let rows = [
        LoadedReadTimestamp {
            ts: "9.000001",
            pending: false,
        },
        LoadedReadTimestamp {
            ts: "11.000001",
            pending: true,
        },
        LoadedReadTimestamp {
            ts: "12.x",
            pending: false,
        },
    ];
    assert_eq!(
        latest_channel_read_target(Some("10.000001"), rows),
        Some("10.000001")
    );
    assert_eq!(latest_channel_read_target(None, rows), Some("9.000001"));
    assert_eq!(latest_channel_read_target(Some("1e3.0"), []), None);
    assert_eq!(
        latest_channel_read_target(
            Some("10.000001"),
            [LoadedReadTimestamp {
                ts: "10.000002",
                pending: false
            }]
        ),
        Some("10.000002")
    );
}

#[test]
fn escape_channel_requires_unread_or_ghost_and_equal_cursor_avoids_request() {
    assert_eq!(
        channel_escape(None, false, None, Some("10.0")),
        ChannelEscape::NoRead
    );
    assert_eq!(
        channel_escape(Some(0), false, None, Some("10.0")),
        ChannelEscape::NoRead
    );
    assert_eq!(
        channel_escape(None, true, Some("9.0"), Some("10.0")),
        ChannelEscape::Mark { ts: "10.0" }
    );
    assert_eq!(
        channel_escape(Some(3), false, Some("10.0"), Some("10.000000")),
        ChannelEscape::AlreadyAtLatest {
            repair_unread_count: true
        }
    );
    assert_eq!(
        channel_escape(Some(0), true, Some("10.0"), Some("10.0")),
        ChannelEscape::AlreadyAtLatest {
            repair_unread_count: false
        }
    );
    assert_eq!(
        channel_escape(Some(3), true, None, None),
        ChannelEscape::NoRead
    );
}

#[test]
fn subscribed_thread_escape_reads_then_clears_retained_divider_then_allows_close() {
    assert_eq!(
        thread_escape(Some(true), Some("9.0"), Some("9.0"), Some("10.0")),
        ThreadEscape::Mark { ts: "10.0" }
    );
    assert_eq!(
        thread_escape(Some(true), Some("9.0"), Some("10.0"), Some("10.0")),
        ThreadEscape::ClearDivider
    );
    assert_eq!(
        thread_escape(Some(true), None, Some("10.0"), Some("10.0")),
        ThreadEscape::NoRead
    );
    assert_eq!(
        thread_escape(Some(true), None, None, Some("10.0")),
        ThreadEscape::Mark { ts: "10.0" }
    );
}

#[test]
fn thread_unknown_unsubscribed_invalid_or_unloaded_never_marks() {
    for subscribed in [None, Some(false)] {
        assert_eq!(
            thread_escape(subscribed, None, Some("9.0"), Some("10.0")),
            ThreadEscape::NoRead
        );
    }
    assert_eq!(
        thread_escape(Some(true), None, Some("9.0"), None),
        ThreadEscape::NoRead
    );
    assert_eq!(
        thread_escape(Some(true), None, Some("bad"), Some("10.0")),
        ThreadEscape::NoRead
    );
    assert_eq!(
        thread_escape(Some(true), None, Some("9.0"), Some("pending-10")),
        ThreadEscape::NoRead
    );
}

#[test]
fn partial_history_never_rewinds_an_authoritative_channel_or_thread_cursor() {
    assert_eq!(
        channel_escape(Some(9), true, Some("20.000001"), Some("10.000001")),
        ChannelEscape::AlreadyAtLatest {
            repair_unread_count: false
        }
    );
    assert_eq!(
        thread_escape(Some(true), None, Some("20.000001"), Some("10.000001")),
        ThreadEscape::NoRead
    );
    assert_eq!(
        thread_escape(
            Some(true),
            Some("9.000001"),
            Some("20.000001"),
            Some("10.000001")
        ),
        ThreadEscape::ClearDivider
    );
}
