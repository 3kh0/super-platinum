use super::*;
use crate::{config::WorkspaceSession, slack::api::parse_badge_experiments, state::Workspace};
use serde_json::{Value, json};

fn workspace(inbox: bool) -> Workspace {
    let mut w = Workspace::from_session(&WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Test".into(),
        url: "https://example.slack.com".into(),
        token: String::new(),
    });
    w.notifications.apply_prefs(
        &serde_json::from_value(json!({"all_notifications_prefs":{"global":{},"channels":{}}}))
            .unwrap(),
    );
    let policy = parse_badge_experiments(
        &json!({"ok":true,"assignments":{"activity_inbox":{"group":if inbox {"on"} else {"off"}}}}),
    )
    .unwrap();
    let ticket = w.activity_counts.begin_experiments();
    assert!(w.activity_counts.apply_experiments(ticket, policy));
    w.dock_counts.set_context(DockWorkspaceContext::Standalone);
    w.apply_counts(serde_json::from_value(json!({"activity_v2":{},"channel_badges":{},"saved":{"uncompleted_overdue_count":0},"threads":{"has_unreads":false}})).unwrap());
    w
}
fn categories(w: &mut Workspace, value: Value) {
    w.apply_activity_counts(serde_json::from_value(value).unwrap());
}

#[test]
fn inbox_dm_opt_out_conserves_numeric_and_reactions_never_create_badge() {
    let mut w = workspace(true);
    categories(&mut w, json!({"dm":4,"at_user":2,"message_reaction":99}));
    assert_eq!(w.dock_badge_contribution().unwrap().highlights, 6);
    w.notifications
        .apply_change("activity_dms_opt_out", json!("false"));
    assert_eq!(w.activity_badge_count(), Some(2));
    assert_eq!(w.dock_badge_contribution().unwrap().highlights, 6);
    categories(&mut w, json!({"message_reaction":99}));
    assert_eq!(
        DockBadge::aggregate([w.dock_badge_contribution().unwrap()]),
        DockBadge::Empty
    );
}

#[test]
fn later_uses_canonical_overdue_and_correct_visibility_defaults() {
    let mut w = workspace(true);
    assert!(
        w.dock_counts
            .apply_saved_counts(&json!({"uncompleted_overdue_count":7,"uncompleted_count":200}))
    );
    assert_eq!(w.dock_badge_contribution().unwrap().highlights, 0);
    w.notifications
        .global
        .insert("global_reminders_in_activity".into(), json!(false));
    assert_eq!(w.dock_badge_contribution().unwrap().highlights, 7);
    w.notifications
        .apply_change("tab_rail_customization", json!("{\"later\":false}"));
    assert_eq!(w.dock_badge_contribution().unwrap().highlights, 0);
    for value in [json!("{\"later\":null}"), json!("broken"), Value::Null] {
        w.notifications
            .apply_change("tab_rail_customization", value);
        assert_eq!(w.dock_badge_contribution().unwrap().highlights, 7);
    }
    w.notifications
        .global
        .insert("global_reminders_in_activity".into(), Value::Null);
    assert_eq!(w.dock_badge_contribution().unwrap().highlights, 0);
}

#[test]
fn bullet_truthiness_is_raw_and_full_boot_clears_cached_preferences() {
    let mut w = workspace(true);
    w.apply_counts(serde_json::from_value(json!({"channels":[{"id":"C1","is_channel":true,"has_unreads":true}],"threads":{"has_unreads":false}})).unwrap());
    assert_eq!(
        DockBadge::aggregate([w.dock_badge_contribution().unwrap()]),
        DockBadge::Empty
    );
    w.notifications
        .apply_change("mac_ssb_bullet", json!("false"));
    assert_eq!(
        DockBadge::aggregate([w.dock_badge_contribution().unwrap()]),
        DockBadge::Dot
    );
    w.notifications
        .apply_change("activity_include_threads_in_badge_count", json!(true));
    w.notifications
        .apply_change("tab_rail_customization", json!("{\"later\":false}"));
    w.notifications.apply_prefs(&Default::default());
    for key in [
        "mac_ssb_bullet",
        "activity_include_threads_in_badge_count",
        "tab_rail_customization",
    ] {
        assert!(!w.notifications.user.contains_key(key));
    }
    assert_eq!(
        DockBadge::aggregate([w.dock_badge_contribution().unwrap()]),
        DockBadge::Empty
    );
}

#[test]
fn legacy_uses_channel_badges_not_activity_and_thread_pref_and_later() {
    let mut w = workspace(false);
    categories(&mut w, json!({"at_user":900}));
    w.dock_counts.merge_channel_badges(
        &json!({"channels":2,"thread_mentions":3,"thread_unreads":5,"app_dms":7,"dms":999}),
    );
    w.dock_counts
        .apply_saved_counts(&json!({"uncompleted_overdue_count":11}));
    assert_eq!(w.dock_badge_contribution().unwrap().highlights, 23);
    w.notifications
        .apply_change("activity_include_threads_in_badge_count", json!("0"));
    assert_eq!(w.dock_badge_contribution().unwrap().highlights, 28);
    w.notifications
        .apply_change("tab_rail_customization", json!("{\"later\":false}"));
    assert_eq!(w.dock_badge_contribution().unwrap().highlights, 28); // legacy no visibility gate
    w.dock_counts.set_context(DockWorkspaceContext::Unknown);
    assert_eq!(w.dock_badge_contribution(), None);
}

#[test]
fn grid_unknown_does_not_hide_known_inbox_number_or_fabricate_unread_dot() {
    let mut w = workspace(true);
    w.dock_counts.set_context(DockWorkspaceContext::Unknown);
    categories(&mut w, json!({"at_user":3}));
    assert_eq!(w.dock_badge_contribution().unwrap().highlights, 3);
    categories(&mut w, json!({}));
    w.notifications.apply_change("mac_ssb_bullet", json!(true));
    w.apply_counts(
        serde_json::from_value(json!({"channels":[{"id":"C1","has_unreads":true}]})).unwrap(),
    );
    assert_eq!(w.dock_badge_contribution(), None);
}

#[test]
fn realtime_saved_and_legacy_patches_retire_older_counts_without_erasing_siblings() {
    let mut w = workspace(false);
    let revision = w.dock_counts.revision();
    w.dock_counts
        .merge_channel_badges(&json!({"channels":8,"thread_mentions":3,"future":9}));
    w.dock_counts.merge_channel_badges(&json!({"channels":0}));
    w.dock_counts
        .apply_saved_counts(&json!({"uncompleted_overdue_count":4}));
    let old =
        json!({"saved":{"uncompleted_overdue_count":0},"channel_badges":{"thread_mentions":0}});
    assert!(
        !w.dock_counts
            .apply_snapshot(&serde_json::from_value(old).unwrap(), revision)
    );
    assert_eq!(w.dock_counts.overdue_count(), Some(4));
    let map = w.dock_counts.channel_badges().unwrap();
    assert_eq!(
        (map["channels"], map["thread_mentions"], map["future"]),
        (0, 3, 9)
    );
    for bad in [
        json!(null),
        json!([]),
        json!({"uncompleted_overdue_count":null}),
        json!({"uncompleted_overdue_count":-1}),
    ] {
        assert!(!w.dock_counts.apply_saved_counts(&bad));
    }
    assert_eq!(w.dock_counts.overdue_count(), Some(4));
    assert!(!w.dock_counts.merge_channel_badges(&json!({"channels":-1})));
    let revision = w.dock_counts.revision();
    assert!(w.dock_counts.apply_saved_counts(&json!({})));
    assert_eq!(w.dock_counts.overdue_count(), None);
    assert_ne!(w.dock_counts.revision(), revision);
    assert!(!w.dock_counts.apply_snapshot(
        &serde_json::from_value(json!({"saved":{"uncompleted_overdue_count":4}})).unwrap(),
        revision
    ));
    assert_eq!(w.dock_counts.overdue_count(), None);
}

#[test]
fn legacy_suppression_applies_to_org_dms_but_not_jo_lo_numeric_dm_loop() {
    let mut w = workspace(false);
    w.notifications.apply_change("mac_ssb_bullet", json!(true));
    w.apply_counts(serde_json::from_value(json!({"ims":[{"id":"D1","is_im":true,"is_org_shared":true,"has_unreads":true,"mention_count":5}]})).unwrap());
    w.dock_counts.set_context(DockWorkspaceContext::Legacy {
        suppressed_channels: ["D1".into()].into(),
        suppress_threads: true,
        include_later: false,
    });
    let projection = w.dock_badge_contribution().unwrap();
    assert_eq!(projection.highlights, 5);
    assert!(!projection.has_unreads);
    assert_eq!(DockBadge::aggregate([projection]), DockBadge::Count(5));
    w.dock_counts.set_context(DockWorkspaceContext::Unknown);
    categories(&mut w, json!({}));
    let ticket = w.activity_counts.begin_experiments();
    w.activity_counts.apply_experiments(
        ticket,
        parse_badge_experiments(
            &json!({"ok":true,"assignments":{"activity_inbox":{"group":"on"}}}),
        )
        .unwrap(),
    );
    assert_eq!(w.dock_badge_contribution(), None);
}

#[test]
fn source_normalization_ignores_raw_foreign_flag_for_supported_contexts() {
    let mut w = workspace(true);
    w.notifications.apply_change("mac_ssb_bullet", json!(true));
    w.apply_counts(
        serde_json::from_value(
            json!({"channels":[{"id":"C1","has_unreads":true,"fromAnotherTeam":true}]}),
        )
        .unwrap(),
    );
    for context in [
        DockWorkspaceContext::Standalone,
        DockWorkspaceContext::Unified,
    ] {
        w.dock_counts.set_context(context);
        assert!(w.dock_badge_contribution().unwrap().has_unreads);
    }
    w.apply_counts(serde_json::from_value(json!({"channels":[{"id":"C1","has_unreads":false}],"threads":{"has_unreads":true,"unread_count_by_channel":{"C1":3}}})).unwrap());
    assert!(w.dock_badge_contribution().unwrap().has_unreads);
    w.dock_counts.set_context(DockWorkspaceContext::Unknown);
    assert_eq!(w.dock_badge_contribution(), None);
}

#[test]
fn stale_saved_revision_does_not_prevent_canonical_channel_read_refresh() {
    let mut w = workspace(true);
    w.notifications.apply_change("mac_ssb_bullet", json!(true));
    let revision = w.dock_counts.revision();
    w.dock_counts.apply_overdue_count(9);
    let membership = w.channel_lifecycle.revision();
    let threads = w.thread_counts.revision();
    let activity = w.activity_counts.revision();
    w.apply_counts_snapshot_with_dock(serde_json::from_value(json!({"channels":[{"id":"Cnew","has_unreads":true}],"saved":{"uncompleted_overdue_count":0}})).unwrap(), membership,threads,activity,revision);
    assert_eq!(w.dock_counts.overdue_count(), Some(9));
    assert!(w.dock_badge_contribution().unwrap().has_unreads);
    w.confirm_channel_membership("Cnew", false, w.channel_lifecycle.revision());
    assert!(!w.dock_badge_contribution().unwrap().has_unreads);
}

#[test]
fn known_numeric_authority_survives_unknown_dot_and_clears_previous_number() {
    let mut w = workspace(true);
    w.notifications.apply_change("mac_ssb_bullet", json!(true));
    w.notifications
        .apply_change("activity_dms_opt_out", json!(true));
    w.dock_counts.set_context(DockWorkspaceContext::Unknown);
    w.apply_counts(
        serde_json::from_value(json!({"channels":[{"id":"C1","has_unreads":true}]})).unwrap(),
    );
    categories(&mut w, json!({"dm":4}));
    let projection = w.dock_badge_projection().unwrap();
    assert_eq!(projection.highlights, 4);
    assert_eq!(projection.has_unreads, None);
    assert_eq!(w.dock_badge_contribution(), None); // bridge remains honest
    categories(&mut w, json!({}));
    let projection = w.dock_badge_projection().unwrap();
    assert_eq!(projection.highlights, 0); // must replace cached four immediately
    assert_eq!(projection.has_unreads, None);
    assert!(projection.show_bullet);
}
