use super::*;
use serde_json::json;
use super_platinum_core::{
    config::{Session, Settings},
    slack::{
        models::{ActivityFeedPage, ActivityItem},
        realtime::parse_event,
    },
    state::Workspace,
};

fn core() -> CoreAppState {
    let mut core = CoreAppState::new(Settings::default());
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.invalid".into(),
        token: "fixture-token".into(),
    };
    core.workspaces
        .insert("T1".into(), Workspace::from_session(&session));
    core.active_account = Some("fixture-owner".into());
    core.active_team = Some("T1".into());
    core.session = Some(Session {
        d_cookie: "fixture-cookie".into(),
        workspaces: [("T1".into(), session)].into(),
    });
    core.transport = Some(Arc::new(Transport::new("fixture-cookie").unwrap()));
    core.activity.ensure_scope(core.account_epoch, "T1");
    core.activity.upsert(item("1.0"));
    core.activity.loaded = true;
    core.activity.selected = Some("item-1.0".into());
    core
}
fn item(ts: &str) -> ActivityItem {
    serde_json::from_value(json!({"key":format!("item-{ts}"),"feed_ts":ts,"is_unread":true,"item":{"type":"at_user","message":{"channel":"C1","ts":ts}}})).unwrap()
}
fn event(value: serde_json::Value) -> super_platinum_core::slack::events::RtEvent {
    parse_event(&json!({"type":"badge_counts_updated","activity_v2":value}).to_string()).unwrap()
}
fn apply(core: &mut CoreAppState, team: &str, generation: u64, value: serde_json::Value) {
    crate::realtime::apply_in_view(core, team, generation, event(value), false);
}
fn owner(core: &CoreAppState) -> CountsOwner {
    CountsOwner::capture(
        core,
        core.transport.as_ref().unwrap(),
        &core.session.as_ref().unwrap().workspaces["T1"],
    )
    .unwrap()
}
fn counts(value: serde_json::Value) -> CountsPage {
    serde_json::from_value(json!({"activity_v2":value})).unwrap()
}

fn inbox(core: &mut CoreAppState) {
    let rules = super_platinum_core::slack::api::parse_badge_experiments(
        &json!({"ok":true,"assignments":{"activity_inbox":{"group":"on"}}}),
    )
    .unwrap();
    assert!(owner(core).begin_rules(core).unwrap().apply(core, rules));
}

#[test]
fn direct_item_read_preserves_inbox_categories_until_authoritative_update() {
    let mut c = core();
    inbox(&mut c);
    apply(&mut c, "T1", 0, json!({"at_user":5,"message_reaction":40}));
    let revision = c.workspaces["T1"].activity_counts.revision();
    assert!(mark_item_read(&mut c, "item-1.0").is_some());
    assert_eq!(c.activity.unread_count(), 0);
    assert_eq!(c.workspaces["T1"].activity_badge_count(), Some(5));
    assert_eq!(c.workspaces["T1"].activity_unread_count, Some(45));
    assert_eq!(c.workspaces["T1"].activity_counts.revision(), revision);
    assert!(mark_item_read(&mut c, "item-1.0").is_none());
}

#[test]
fn channel_cursor_read_preserves_filtered_badge_and_remaining_categories() {
    let mut c = core();
    inbox(&mut c);
    c.activity.items.clear();
    for (key, kind) in [("mention", "at_user"), ("reaction", "message_reaction")] {
        c.activity.upsert(
            serde_json::from_value(json!({"key":key,"feed_ts":"1.0","is_unread":true,
            "item":{"type":kind,"message":{"channel":"C1","ts":"1.0"}}}))
            .unwrap(),
        );
    }
    apply(
        &mut c,
        "T1",
        0,
        json!({"at_user":5,"message_reaction":40,"dm":7}),
    );
    let old_http = owner(&c).begin(&c).unwrap();
    mark_channel_read(&mut c, "T1", "C1");
    let w = &c.workspaces["T1"];
    assert_eq!(w.activity_badge_count(), Some(11));
    assert_eq!(w.dm_activity_badge_count(), Some(7));
    assert_eq!(
        w.activity_counts.snapshot.as_ref().unwrap().categories()["message_reaction"],
        39
    );
    assert_eq!(c.activity.unread_count(), 0);
    assert!(old_http.apply(
        &mut c,
        counts(json!({"at_user":5,"message_reaction":40,"dm":7}))
    ));
    assert_eq!(c.workspaces["T1"].activity_badge_count(), Some(11));
}

#[test]
fn local_activity_read_cannot_apply_to_another_workspace_feed() {
    let mut c = core();
    c.active_team = Some("T2".into());
    assert!(mark_item_read(&mut c, "item-1.0").is_none());
    mark_channel_read(&mut c, "T1", "C1");
    assert_eq!(c.activity.unread_count(), 1);
}

#[test]
fn display_rules_adoption_rejects_replaced_owner_and_older_request() {
    let rules = || {
        super_platinum_core::slack::api::parse_badge_experiments(
            &json!({"ok":true,"assignments":{"activity_inbox":{"group":"on"}}}),
        )
        .unwrap()
    };
    let mut c = core();
    let old = owner(&c).begin_rules(&mut c).unwrap();
    let latest = owner(&c).begin_rules(&mut c).unwrap();
    assert!(latest.apply(&mut c, rules()));
    assert!(!old.apply(&mut c, rules()));
    assert_eq!(c.workspaces["T1"].activity_inbox_enabled(), Some(true));

    for replace_transport in [false, true] {
        let mut c = core();
        let read = owner(&c).begin_rules(&mut c).unwrap();
        if replace_transport {
            c.transport = Some(Arc::new(Transport::new("replacement").unwrap()));
        } else {
            c.account_epoch += 1;
        }
        assert!(!read.apply(&mut c, rules()));
        assert_eq!(c.workspaces["T1"].activity_inbox_enabled(), None);
    }
}

#[test]
fn display_rules_keep_last_known_policy_during_refresh_and_follow_workspace_not_selection() {
    let mut c = core();
    let enabled = super_platinum_core::slack::api::parse_badge_experiments(
        &json!({"ok":true,"assignments":{"activity_inbox":{"group":"on"}}}),
    )
    .unwrap();
    assert!(
        owner(&c)
            .begin_rules(&mut c)
            .unwrap()
            .apply(&mut c, enabled)
    );
    let read = owner(&c).begin_rules(&mut c).unwrap();
    assert_eq!(c.workspaces["T1"].activity_inbox_enabled(), Some(true));
    c.active_team = Some("another-workspace".into());
    let disabled = super_platinum_core::slack::api::parse_badge_experiments(
        &json!({"ok":true,"assignments":{}}),
    )
    .unwrap();
    assert!(read.apply(&mut c, disabled));
    assert_eq!(c.workspaces["T1"].activity_inbox_enabled(), Some(false));
}

#[test]
fn typed_live_counts_replace_categories_and_zero_reconciles_only_current_generation() {
    let mut c = core();
    c.workspaces.get_mut("T1").unwrap().rt_generation = 3;
    apply(&mut c, "T1", 2, json!({"mentions":45}));
    assert_eq!(c.workspaces["T1"].activity_unread_count, None);
    apply(&mut c, "T1", 3, json!({"mentions":40,"future":5}));
    assert_eq!(c.workspaces["T1"].activity_unread_count, Some(45));
    apply(&mut c, "T1", 3, json!({"replies":17}));
    let categories = c.workspaces["T1"]
        .activity_counts
        .snapshot
        .as_ref()
        .unwrap()
        .categories();
    assert_eq!(categories.len(), 1);
    assert_eq!(categories["replies"], 17);
    let old_page = c.activity.begin(false).unwrap();
    apply(&mut c, "T1", 3, json!({}));
    assert_eq!(c.workspaces["T1"].activity_unread_count, Some(0));
    assert_eq!(c.activity.items.len(), 1);
    assert_eq!(c.activity.selected.as_deref(), Some("item-1.0"));
    assert_eq!(c.activity.unread_count(), 0);
    assert!(c.activity.apply_page(
        &old_page,
        ActivityFeedPage {
            items: vec![item("1.0"), item("2.0")],
            ..Default::default()
        }
    ));
    assert_eq!(
        c.activity.unread_count(),
        0,
        "old unseen page rows stay read after zero"
    );
    assert_eq!(c.activity.selected.as_deref(), Some("item-1.0"));
    assert!(c.activity.begin(false).is_some());
}

#[test]
fn background_and_unknown_workspace_frames_cannot_clear_active_feed() {
    let mut c = core();
    let mut background = c.session.as_ref().unwrap().workspaces["T1"].clone();
    background.team_id = "T2".into();
    c.workspaces
        .insert("T2".into(), Workspace::from_session(&background));
    apply(&mut c, "T2", 0, json!({"dm":7}));
    assert_eq!(c.workspaces["T2"].activity_unread_count, Some(7));
    apply(&mut c, "T2", 0, json!({}));
    apply(&mut c, "missing", 0, json!({}));
    assert_eq!(c.activity.unread_count(), 1);
    assert!(c.activity.matches_scope(c.account_epoch, "T1"));
    assert!(parse_event(r#"{"type":"badge_counts_updated"}"#).is_none());
    assert!(parse_event(r#"{"type":"badge_counts_updated","activity_v2":null}"#).is_none());
}

#[test]
fn late_http_counts_cannot_overwrite_realtime_or_local_read_revision() {
    for local in [false, true] {
        let mut c = core();
        let read = owner(&c).begin(&c).unwrap();
        if local {
            c.workspaces
                .get_mut("T1")
                .unwrap()
                .set_activity_unread_count(4);
        } else {
            apply(&mut c, "T1", 0, json!({"future":4}));
        }
        assert!(read.apply(&mut c, counts(json!({}))));
        assert_eq!(c.workspaces["T1"].activity_unread_count, Some(4));
        assert_eq!(
            c.activity.unread_count(),
            1,
            "stale zero must not mark feed read"
        );
        assert_eq!(c.workspaces["T1"].activity_counts.snapshot.is_none(), local);
    }
}

#[test]
fn http_zero_reconciles_feed_but_missing_counts_do_not_clear_it() {
    let mut c = core();
    assert!(
        owner(&c)
            .begin(&c)
            .unwrap()
            .apply(&mut c, CountsPage::default())
    );
    assert_eq!(c.activity.unread_count(), 1);
    assert!(
        owner(&c)
            .begin(&c)
            .unwrap()
            .apply(&mut c, counts(json!({})))
    );
    assert_eq!(c.activity.unread_count(), 0);
}

#[test]
fn counts_response_rejects_changed_owner_credentials_and_transport() {
    let changes: [fn(&mut CoreAppState); 7] = [
        |c| c.account_epoch += 1,
        |c| c.active_account = Some("another".into()),
        |c| c.session.as_mut().unwrap().d_cookie = "replacement".into(),
        |c| {
            c.session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("T1")
                .unwrap()
                .token = "replacement".into()
        },
        |c| {
            c.session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("T1")
                .unwrap()
                .url = "https://other.invalid".into()
        },
        |c| c.workspaces.get_mut("T1").unwrap().self_user_id = "other".into(),
        |c| c.transport = Some(Arc::new(Transport::new("replacement").unwrap())),
    ];
    for change in changes {
        let mut c = core();
        let read = owner(&c).begin(&c).unwrap();
        change(&mut c);
        assert!(!read.apply(&mut c, counts(json!({"mention":9}))));
        assert_eq!(c.workspaces["T1"].activity_unread_count, None);
    }
}

#[test]
fn counts_owner_pins_earlier_boot_awaits_but_allows_background_navigation() {
    let mut c = core();
    let captured = owner(&c);
    c.active_team = Some("T2".into());
    let read = captured.clone().begin(&c).unwrap();
    assert!(read.apply(&mut c, counts(json!({"dm":2}))));
    assert_eq!(c.workspaces["T1"].activity_unread_count, Some(2));
    c.session.as_mut().unwrap().d_cookie = "replacement".into();
    assert!(captured.begin(&c).is_none());
}

#[test]
fn first_socket_connect_during_boot_counts_does_not_discard_authoritative_snapshot() {
    let mut c = core();
    let read = owner(&c).begin(&c).unwrap();
    c.workspaces.get_mut("T1").unwrap().rt_generation = 1;
    assert!(read.apply(&mut c, counts(json!({"mention": 8}))));
    assert_eq!(c.workspaces["T1"].activity_unread_count, Some(8));
}
