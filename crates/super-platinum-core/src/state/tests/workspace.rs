use super::super::*;

fn counts_read_workspace() -> Workspace {
    Workspace::from_session(&crate::config::WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    })
}

#[test]
fn exact_counts_row_adopts_backward_cursor_without_touching_membership() {
    let mut workspace = counts_read_workspace();
    for id in ["C1", "C2"] {
        let channel: Channel = serde_json::from_value(serde_json::json!({
            "id":id,"is_member":true,"name":"Retained", "last_read":"9.000000",
            "has_unreads":true,"unread_count":8,"mention_count":3,"vip_count":2,
            "latest":"10.000000"
        }))
        .unwrap();
        workspace.channels.insert(id.into(), channel.clone());
        workspace.apply_channel_counts_read_state(&channel);
    }
    let revision = workspace.messages["C1"].read_revision;
    let membership = workspace.channel_lifecycle.revision();
    let unrelated = serde_json::to_value(&workspace.channels["C2"]).unwrap();
    let row = serde_json::from_value(serde_json::json!({
        "id":"C1","last_read":"4.000000","has_unreads":true,
        "unread_count":2,"unread_count_display":2,"mention_count":1,
        "vip_count":1,"latest":"11.000000","is_member":false,"is_starred":true
    }))
    .unwrap();
    workspace.apply_channel_counts_read_state(&row);
    let bag = &workspace.messages["C1"];
    assert_eq!(bag.last_read.as_deref(), Some("4.000000"));
    assert_eq!(bag.read_revision, revision + 1);
    assert_eq!(
        (bag.unread_count, bag.mention_count, bag.vip_count),
        (2, 1, 1)
    );
    assert_eq!(bag.latest.as_deref(), Some("11.000000"));
    let metadata = &workspace.channels["C1"];
    assert_eq!(metadata.last_read.as_deref(), Some("4.000000"));
    assert_eq!(metadata.unread_count, Some(2));
    assert_eq!(metadata.unread_count_display, Some(2));
    assert_eq!(metadata.mention_count, Some(1));
    assert!(metadata.has_unreads);
    assert!(!metadata.is_starred);
    assert_eq!(metadata.extra["is_member"], true);
    assert_eq!(metadata.name.as_deref(), Some("Retained"));
    assert_eq!(workspace.channel_lifecycle.revision(), membership);
    assert_eq!(
        serde_json::to_value(&workspace.channels["C2"]).unwrap(),
        unrelated
    );
    let other = &workspace.messages["C2"];
    assert_eq!(other.last_read.as_deref(), Some("9.000000"));
    assert_eq!(
        (other.unread_count, other.mention_count, other.vip_count),
        (8, 3, 2)
    );
}

#[test]
fn exact_counts_row_clears_missing_counts_and_preserves_forced_unread() {
    let mut workspace = counts_read_workspace();
    let row = |value| serde_json::from_value::<Channel>(value).unwrap();
    workspace.apply_channel_counts_read_state(&row(serde_json::json!({
        "id":"C1","has_unreads":true,"unread_count":0,"mention_count":3,"vip_count":2
    })));
    workspace.apply_channel_counts_read_state(&row(serde_json::json!({
        "id":"C1","has_unreads":true
    })));
    let bag = &workspace.messages["C1"];
    assert_eq!(
        (bag.unread_count, bag.mention_count, bag.vip_count),
        (1, 0, 0)
    );
    workspace.apply_channel_counts_read_state(&row(serde_json::json!({
        "id":"C1","has_unreads":false
    })));
    let bag = &workspace.messages["C1"];
    assert_eq!(
        (bag.unread_count, bag.mention_count, bag.vip_count),
        (0, 0, 0)
    );
    assert!(workspace.channels.is_empty());
}

#[test]
fn full_counts_still_updates_membership_stars_and_inserts_channels() {
    let mut workspace = counts_read_workspace();
    for id in ["C1", "C2"] {
        workspace.channels.insert(
            id.into(),
            serde_json::from_value(serde_json::json!({
                "id":id,"is_member":true
            }))
            .unwrap(),
        );
    }
    workspace.apply_counts(serde_json::from_value(serde_json::json!({"channels":[
        {"id":"C1","is_starred":true,"last_read":"4.000000","has_unreads":true,"unread_count":2},
        {"id":"C3","has_unreads":false}
    ]})).unwrap());
    assert!(workspace.channels["C1"].is_starred);
    assert_eq!(workspace.channels["C2"].extra["is_member"], false);
    assert_eq!(workspace.channels["C3"].extra["is_member"], true);
    assert_eq!(workspace.channels["C3"].mention_count, Some(0));
    assert_eq!(
        workspace.messages["C1"].last_read.as_deref(),
        Some("4.000000")
    );
    assert_eq!(workspace.messages["C1"].unread_count, 2);
}

#[test]
fn boot_locale_changes_preserve_raw_preference_and_missing_warm_value() {
    let mut workspace = Workspace::from_session(&crate::config::WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    });
    workspace.locale = Some("fr-FR".into());
    let boot = |value| serde_json::from_value::<crate::slack::models::BootData>(value).unwrap();
    workspace.apply_boot(boot(serde_json::json!({"prefs":{}})));
    assert_eq!(workspace.locale.as_deref(), Some("fr-FR"));
    workspace.apply_boot(boot(serde_json::json!({"prefs":{"locale":"de-DE"}})));
    assert_eq!(workspace.locale.as_deref(), Some("de-DE"));
    workspace.apply_boot(boot(serde_json::json!({"prefs":{"locale":null}})));
    assert_eq!(workspace.locale, None);
}

#[test]
fn prune_typing_drops_stale() {
    let mut ws = Workspace {
        team_id: "T1".into(),
        name: "test".into(),
        url: "https://t".into(),
        self_user_id: "USELF".into(),
        locale: None,
        team_plan: None,
        activity_unread_count: None,
        activity_counts: Default::default(),
        dock_counts: Default::default(),
        thread_counts: Default::default(),
        channels: BTreeMap::new(),
        starred_order: Vec::new(),
        dm_order: Vec::new(),
        recent_channels: Vec::new(),
        last_active_channel: None,
        priority_scores: BTreeMap::new(),
        frecency: BTreeMap::new(),
        priority_sidebar_section: false,
        vip_users: HashSet::new(),
        vip_revisions: Default::default(),
        sidebar: SidebarConfig::default(),
        notifications: NotificationConfig::default(),
        read_preferences: ReadPreferences::default(),
        clear_unreads_preference: ClearUnreadsPreference::default(),
        users: HashMap::new(),
        bots: Default::default(),
        usergroups: HashMap::new(),
        deleted_files: Default::default(),
        custom_emoji: HashMap::new(),
        custom_emoji_revision: 0,
        emoji_preferences: Default::default(),
        messages: HashMap::new(),
        typing: HashMap::new(),
        presence: HashMap::new(),
        self_dnd: Default::default(),
        active_huddles: HashMap::new(),
        rt: RealtimeStatus::default(),
        rt_generation: 0,
        channel_lifecycle: Default::default(),
    };
    let now = Instant::now();
    ws.set_typing("C1", "U1".into(), now - Duration::from_secs(10));
    ws.set_typing("C1", "U2".into(), now);
    assert!(ws.prune_typing(now, Duration::from_secs(4)));
    assert_eq!(ws.typing_names("C1"), vec![ws.display_name("U2")]);
}

#[test]
fn apply_room_tracks_and_clears_active_huddles() {
    let mut ws = Workspace::from_session(&crate::config::WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U0".into(),
        name: "T".into(),
        url: "https://t.slack.com".into(),
        token: "xoxc-x".into(),
    });

    let mut room = Room {
        id: "R1".into(),
        channels: vec!["C1".into()],
        participants: vec!["U1".into()],
        ..Default::default()
    };
    assert!(ws.apply_room(room.clone()));
    assert_eq!(ws.active_huddle("C1").map(|r| r.id.as_str()), Some("R1"));

    // A stale leave for a different room must not clear the tracked one.
    let other = Room {
        id: "R2".into(),
        channels: vec!["C1".into()],
        participants: vec![],
        has_ended: true,
        ..Default::default()
    };
    assert!(!ws.apply_room(other));
    assert!(ws.active_huddle("C1").is_some());

    // Ending the tracked room clears it.
    room.has_ended = true;
    room.participants.clear();
    assert!(ws.apply_room(room));
    assert!(ws.active_huddle("C1").is_none());
}
