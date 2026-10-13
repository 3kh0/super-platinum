use super::super::*;

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
        activity_unread_count: None,
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
        users: HashMap::new(),
        bots: Default::default(),
        usergroups: HashMap::new(),
        deleted_files: Default::default(),
        custom_emoji: HashMap::new(),
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
