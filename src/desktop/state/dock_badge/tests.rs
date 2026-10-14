use super::*;
use serde_json::json;
use super_platinum_core::{
    config::{Settings, WorkspaceSession},
    state::Workspace,
};

fn workspace(team: &str, count: u32) -> Workspace {
    let mut w = Workspace::from_session(&WorkspaceSession {
        team_id: team.into(),
        user_id: format!("U{team}"),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.invalid".into(),
        token: String::new(),
    });
    w.notifications.dock_loaded = true;
    let rules = super_platinum_core::slack::api::parse_badge_experiments(
        &json!({"ok":true,"assignments":{"activity_inbox":{"group":"on"}}}),
    )
    .unwrap();
    let ticket = w.activity_counts.begin_experiments();
    w.activity_counts.apply_experiments(ticket, rules);
    w.apply_activity_counts(serde_json::from_value(json!({"at_user":count})).unwrap());
    w
}
fn core() -> CoreAppState {
    let mut c = CoreAppState::new(Settings::default());
    c.active_account = Some("fixture-a".into());
    c.workspaces.insert("T1".into(), workspace("T1", 3));
    c.workspaces.insert("T2".into(), workspace("T2", 5));
    c
}

#[test]
fn targeted_background_updates_preserve_other_workspace_then_remove_retired_team() {
    let mut c = core();
    let mut cache = Cache::default();
    cache.refresh(&c, None);
    assert_eq!(cache.value, DockBadge::Count(8));
    c.workspaces
        .get_mut("T2")
        .unwrap()
        .apply_activity_counts(serde_json::from_value(json!({"at_user":7})).unwrap());
    cache.refresh(&c, Some("T2"));
    assert_eq!(cache.value, DockBadge::Count(10));
    c.workspaces.remove("T1");
    cache.refresh(&c, Some("T2"));
    assert_eq!(cache.value, DockBadge::Count(7));
    c.workspaces
        .get_mut("T2")
        .unwrap()
        .apply_activity_counts(serde_json::from_value(json!({})).unwrap());
    cache.refresh(&c, Some("T2"));
    assert_eq!(cache.value, DockBadge::Empty);
}

#[test]
fn missing_authority_retains_only_same_account_epoch_and_self_user() {
    let mut c = core();
    let mut cache = Cache::default();
    cache.refresh(&c, None);
    for w in c.workspaces.values_mut() {
        w.notifications.dock_loaded = false;
    }
    cache.refresh(&c, None);
    assert_eq!(cache.value, DockBadge::Count(8));
    c.workspaces.get_mut("T1").unwrap().self_user_id = "different-self".into();
    cache.refresh(&c, Some("T2"));
    assert_eq!(cache.value, DockBadge::Count(5));
    c.account_epoch += 1;
    cache.refresh(&c, None);
    assert_eq!(cache.value, DockBadge::Empty);
    c.workspaces.insert("T1".into(), workspace("T1", 3));
    cache.refresh(&c, Some("T1"));
    assert_eq!(cache.value, DockBadge::Count(3));
    c.active_account = Some("fixture-b".into());
    c.workspaces
        .get_mut("T1")
        .unwrap()
        .notifications
        .dock_loaded = false;
    cache.refresh(&c, None);
    assert_eq!(cache.value, DockBadge::Empty);
    c.active_account = None;
    c.workspaces.insert("T1".into(), workspace("T1", 99));
    cache.refresh(&c, None);
    assert_eq!(cache.value, DockBadge::Empty);
}

#[test]
fn shell_full_and_targeted_refresh_share_the_cached_projection() {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core = core();
    shell.refresh_from_core(); // Refresh before active-team early return.
    assert_eq!(shell.dock_badge_value(), DockBadge::Count(8));
    shell
        .core
        .workspaces
        .get_mut("T2")
        .unwrap()
        .apply_activity_counts(serde_json::from_value(json!({"at_user":1})).unwrap());
    assert_eq!(shell.dock_badge_value(), DockBadge::Count(8));
    shell.refresh_dock_badge_for("T2");
    assert_eq!(shell.dock_badge_value(), DockBadge::Count(4));
    shell.core.account_epoch += 1;
    assert_eq!(shell.dock_badge_value(), DockBadge::Empty);
}

#[test]
fn unknown_grid_dot_cannot_keep_a_retired_numeric_count() {
    let mut c = core();
    c.workspaces.remove("T2");
    let mut cache = Cache::default();
    cache.refresh(&c, None);
    assert_eq!(cache.value, DockBadge::Count(3));
    let workspace = c.workspaces.get_mut("T1").unwrap();
    workspace
        .notifications
        .apply_change("mac_ssb_bullet", json!(true));
    workspace.apply_activity_counts(serde_json::from_value(json!({})).unwrap());
    // No authoritative ordinary-unread/Enterprise context has loaded, but
    // the exact numeric component is now zero and must not remain three.
    cache.refresh(&c, Some("T1"));
    assert!(!matches!(cache.value, DockBadge::Count(_)));
    assert_eq!(cache.workspaces["T1"].contribution.highlights, 0);
}

#[test]
fn saved_counts_update_background_workspace_without_replacing_visible_later_state() {
    let mut c = core();
    c.active_team = Some("T1".into());
    c.later.ensure_scope(c.account_epoch, "T1");
    let before = c.later.generation;
    let event = super_platinum_core::slack::realtime::parse_event(
        r#"{"type":"saved_updated","counts":{"uncompleted_overdue_count":6}}"#,
    )
    .unwrap();
    crate::realtime::apply_in_view(&mut c, "T2", 0, event.clone(), false);
    assert_eq!(c.workspaces["T2"].dock_counts.overdue_count(), Some(6));
    assert_eq!(c.workspaces["T1"].dock_counts.overdue_count(), None);
    assert_eq!(c.later.generation, before);
    c.workspaces.get_mut("T2").unwrap().rt_generation = 1;
    crate::realtime::apply_in_view(&mut c, "T2", 0, event, false);
    assert_eq!(c.workspaces["T2"].dock_counts.overdue_count(), Some(6));
}

#[test]
fn combined_badge_frame_selects_known_policy_and_legacy_does_not_overwrite_inbox() {
    use super_platinum_core::slack::realtime::parse_event;
    let mut c = core();
    let frame =
        r#"{"type":"badge_counts_updated","activity_v2":{"at_user":9},"counts":{"channels":70}}"#;
    crate::realtime::apply_in_view(&mut c, "T1", 0, parse_event(frame).unwrap(), false);
    assert_eq!(c.workspaces["T1"].activity_badge_count(), Some(9));
    assert_eq!(c.workspaces["T1"].dock_counts.channel_badges(), None);
    let w = c.workspaces.get_mut("T1").unwrap();
    let policy = super_platinum_core::slack::api::parse_badge_experiments(
        &json!({"ok":true,"assignments":{"activity_inbox":{"group":"control"}}}),
    )
    .unwrap();
    let ticket = w.activity_counts.begin_experiments();
    w.activity_counts.apply_experiments(ticket, policy);
    crate::realtime::apply_in_view(&mut c, "T1", 0, parse_event(frame).unwrap(), false);
    assert_eq!(
        c.workspaces["T1"].dock_counts.channel_badges().unwrap()["channels"],
        70
    );
    crate::realtime::apply_in_view(
        &mut c,
        "T2",
        0,
        parse_event(r#"{"type":"badge_counts_updated","counts":{"channels":99}}"#).unwrap(),
        false,
    );
    assert_eq!(c.workspaces["T2"].dock_counts.channel_badges(), None);
}
