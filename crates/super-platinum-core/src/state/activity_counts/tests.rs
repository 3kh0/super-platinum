use super::*;
use serde_json::json;

fn counts(value: serde_json::Value) -> ActivityBadgeCounts {
    serde_json::from_value(value).unwrap()
}

fn workspace() -> crate::state::Workspace {
    crate::state::Workspace::from_session(&crate::config::WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Test".into(),
        url: "https://test.slack.com".into(),
        token: String::new(),
    })
}

#[test]
fn complete_snapshot_retains_unknown_categories_and_replaces_removed_categories() {
    let mut state = ActivityCountsState::default();
    state.apply_live(counts(json!({"dm": 3, "future_category": 4})));
    assert_eq!(
        state.snapshot.as_ref().unwrap().categories()["future_category"],
        4
    );
    state.apply_live(counts(json!({"thread_v2": 2})));
    assert_eq!(state.snapshot.as_ref().unwrap().total(), 2);
    assert!(
        !state
            .snapshot
            .as_ref()
            .unwrap()
            .categories()
            .contains_key("dm")
    );
    state.apply_live(counts(json!({})));
    assert_eq!(state.snapshot.as_ref().unwrap().total(), 0);
}

#[test]
fn late_http_cannot_restore_activity_after_live_clear_or_local_read() {
    let mut workspace = workspace();
    workspace.apply_activity_counts(counts(json!({"dm": 3})));
    let before = workspace.activity_counts.revision();
    workspace.apply_activity_counts(counts(json!({})));
    let page = || serde_json::from_value(json!({"activity_v2":{"dm":3}})).unwrap();
    workspace.apply_counts_snapshot_with_activity(page(), 0, 0, before);
    assert_eq!(workspace.activity_unread_count, Some(0));
    assert_eq!(
        workspace.activity_counts.snapshot.as_ref().unwrap().total(),
        0
    );

    let before = workspace.activity_counts.revision();
    workspace.set_activity_unread_count(1);
    workspace.apply_counts_snapshot_with_activity(page(), 0, 0, before);
    assert_eq!(workspace.activity_unread_count, Some(1));
    assert!(workspace.activity_counts.snapshot.is_none());
    let current = workspace.activity_counts.revision();
    workspace.apply_counts_snapshot_with_activity(page(), 0, 0, current);
    assert_eq!(workspace.activity_unread_count, Some(3));
    assert_eq!(
        workspace.activity_counts.snapshot.as_ref().unwrap().total(),
        3
    );
}

#[test]
fn missing_and_null_preserve_counts_but_empty_is_authoritative() {
    let mut workspace = workspace();
    workspace.apply_activity_counts(counts(json!({"at_user": 2})));
    let revision = workspace.activity_counts.revision();
    for raw in [json!({}), json!({"activity_v2":null})] {
        workspace.apply_counts(serde_json::from_value(raw).unwrap());
        assert_eq!(workspace.activity_unread_count, Some(2));
        assert_eq!(workspace.activity_counts.revision(), revision);
    }
    workspace.apply_counts(serde_json::from_value(json!({"activity_v2":{}})).unwrap());
    assert_eq!(workspace.activity_unread_count, Some(0));
    assert!(
        workspace
            .activity_counts
            .snapshot
            .unwrap()
            .categories()
            .is_empty()
    );
}

#[test]
fn category_values_are_typed_and_compatibility_total_saturates() {
    for raw in [
        json!({"dm":-1}),
        json!({"dm":null}),
        json!({"dm":"2"}),
        json!({"dm":4294967296u64}),
    ] {
        assert!(serde_json::from_value::<ActivityBadgeCounts>(raw).is_err());
    }
    assert_eq!(counts(json!({"dm":u32::MAX,"at_user":1})).total(), u32::MAX);
}

fn policy(assignments: serde_json::Value) -> BadgeExperiments {
    crate::slack::api::parse_badge_experiments(&json!({"ok":true,"assignments":assignments}))
        .unwrap()
}
fn install_policy(w: &mut crate::state::Workspace, assignments: serde_json::Value) {
    let ticket = w.activity_counts.begin_experiments();
    assert!(
        w.activity_counts
            .apply_experiments(ticket, policy(assignments))
    );
}

#[test]
fn inbox_projection_excludes_reactions_unknowns_and_disabled_categories() {
    let mut w = workspace();
    w.apply_activity_counts(counts(json!({"at_user":2,"dm":4,"thread_v2":3,
        "message_reaction":20,"unknown":100,"list_todo_notification":5,
        "list_approval_request":6,"team_joiner_invite":7,"saved_reminder":1})));
    assert_eq!(w.activity_inbox_enabled(), None);
    assert_eq!(w.activity_badge_count(), None);
    install_policy(&mut w, json!({"activity_inbox":{"group":"on"}}));
    assert_eq!(w.activity_badge_count(), Some(10));
    assert_eq!(w.dm_activity_badge_count(), Some(4));
    for value in [json!(true), json!("false"), json!("0"), json!([])] {
        w.notifications.apply_change("activity_dms_opt_out", value);
        assert_eq!(w.activity_badge_count(), Some(6));
        assert_eq!(w.dm_activity_badge_count(), Some(4));
    }
    for value in [json!(null), json!(false), json!(0), json!("")] {
        w.notifications.apply_change("activity_dms_opt_out", value);
        assert_eq!(w.activity_badge_count(), Some(10));
    }
    install_policy(
        &mut w,
        json!({"activity_inbox":{"group":"on"},
        "lists_monster_step":{"group":"on"},"approvals_in_todos":{"group":"on"},
        "inviter_joiner_first_dm_v2":{"group":"treatment"}}),
    );
    assert_eq!(w.activity_badge_count(), Some(28));
    assert_eq!(
        w.activity_counts.snapshot.as_ref().unwrap().categories()["unknown"],
        100
    );
}

#[test]
fn policy_replacement_and_stale_response_guard_preserve_exact_group_semantics() {
    let mut w = workspace();
    w.apply_activity_counts(counts(json!({"team_joiner_invite":7})));
    let old = w.activity_counts.begin_experiments();
    let current = w.activity_counts.begin_experiments();
    assert!(w.activity_counts.apply_experiments(
        current,
        policy(json!({"activity_inbox":{"group":"on"},
        "inviter_joiner_first_dm_v2":{"group":"on"}}))
    ));
    assert_eq!(w.activity_badge_count(), Some(0));
    assert!(!w.activity_counts.apply_experiments(old, policy(json!({}))));
    assert_eq!(w.activity_inbox_enabled(), Some(true));
    install_policy(&mut w, json!({}));
    assert_eq!(w.activity_inbox_enabled(), Some(false));
    assert_eq!(w.activity_badge_count(), None);
    assert_eq!(w.dm_activity_badge_count(), None);
    assert_eq!(workspace().activity_inbox_enabled(), None);
}

#[test]
fn full_boot_omission_resets_opt_out_and_partial_pref_keeps_other_keys() {
    let mut w = workspace();
    w.apply_activity_counts(counts(json!({"dm":4})));
    install_policy(&mut w, json!({"activity_inbox":{"group":"on"}}));
    let prefs = serde_json::from_value(json!({"activity_dms_opt_out":true})).unwrap();
    w.notifications.apply_prefs(&prefs);
    assert_eq!(w.activity_badge_count(), Some(0));
    w.notifications.apply_change("mute_sounds", json!(true));
    assert_eq!(w.activity_badge_count(), Some(0));
    w.notifications.apply_prefs(&Default::default());
    assert!(!w.notifications.user.contains_key("activity_dms_opt_out"));
    assert_eq!(w.activity_badge_count(), Some(4));
    w.set_activity_unread_count(3);
    assert_eq!(w.activity_inbox_enabled(), Some(true));
    assert_eq!(w.activity_badge_count(), None);
    assert_eq!(w.dm_activity_badge_count(), None);
}

#[test]
fn category_read_deltas_preserve_filtered_projection_and_reject_older_http() {
    let mut w = workspace();
    install_policy(&mut w, json!({"activity_inbox":{"group":"on"}}));
    w.apply_activity_counts(counts(
        json!({"at_user":2,"message_reaction":20,"future":9}),
    ));
    let before = w.activity_counts.revision();
    assert!(w.mark_activity_categories_read(&[("message_reaction".into(), 1)].into()));
    assert_eq!(w.activity_badge_count(), Some(2));
    assert!(w.mark_activity_categories_read(&[("at_user".into(), 5)].into()));
    assert_eq!(w.activity_badge_count(), Some(0));
    assert_eq!(
        w.activity_counts.snapshot.as_ref().unwrap().categories()["future"],
        9
    );
    w.apply_counts_snapshot_with_activity(
        serde_json::from_value(json!({"activity_v2":{"at_user":2}})).unwrap(),
        0,
        0,
        before,
    );
    assert_eq!(w.activity_badge_count(), Some(0));
    w.set_activity_unread_count(0);
    assert!(!w.mark_activity_categories_read(&[("dm".into(), 1)].into()));
    assert!(w.activity_counts.snapshot.is_none());
}

#[test]
fn conditional_categories_require_their_exact_captured_groups() {
    let mut w = workspace();
    w.apply_activity_counts(counts(json!({"unjoined_channel_mention":2,
        "quietly_added_to_channel":3,"generic_system_alert":4,
        "list_approval_reviewed":5,"list_todo_notification":6,"team_joiner_invite":7})));
    let gates = [
        ("unjoined_channel_mention_in_activity", "on", 2),
        ("quietly_added_to_channel_in_activity", "on", 3),
        ("generic_system_user_alert", "on", 4),
        ("approvals_in_todos", "on", 5),
        ("lists_monster_step", "on", 6),
        ("inviter_joiner_first_dm_v2", "treatment", 7),
    ];
    for (name, group, expected) in gates {
        let mut assignments = json!({"activity_inbox":{"group":"on"}});
        assignments[name] = json!({"group":group});
        install_policy(&mut w, assignments);
        assert_eq!(w.activity_badge_count(), Some(expected), "{name}");
    }
}
