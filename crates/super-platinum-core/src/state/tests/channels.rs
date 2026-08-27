use super::super::*;

#[test]
fn resolves_section_order_from_real_linked_list() {
    let page: crate::slack::models::ChannelSectionsPage = serde_json::from_str(
            r#"{
                "ok": true,
                "channel_sections": [
                    {"channel_section_id":"L_CONNECT","name":"Slack Connect","type":"slack_connect","next_channel_section_id":"L_DMS"},
                    {"channel_section_id":"L_DMS","name":"Direct Messages","type":"direct_messages","next_channel_section_id":"L_STARS"},
                    {"channel_section_id":"L_STARS","name":"","type":"stars","next_channel_section_id":"L_UG"},
                    {"channel_section_id":"L_UG","name":"helpers","type":"user_group","next_channel_section_id":"L_APPS"},
                    {"channel_section_id":"L_APPS","name":"Recent Apps","type":"recent_apps","next_channel_section_id":"L_CHANNELS"},
                    {"channel_section_id":"L_CHANNELS","name":"Channels","type":"channels","next_channel_section_id":"L_AGENTS"},
                    {"channel_section_id":"L_AGENTS","name":"Agents","type":"agents","next_channel_section_id":null}
                ]
            }"#,
        )
        .unwrap();
    let mut ws = Workspace::from_session(&crate::config::WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U_SELF".into(),
        name: "Test".into(),
        url: "https://t".into(),
        token: "xoxc".into(),
    });
    ws.priority_sidebar_section = true;
    ws.apply_channel_sections(page);

    let titles: Vec<String> = ws
        .resolved_sidebar_sections()
        .into_iter()
        .map(|s| s.title)
        .collect();

    assert_eq!(
        titles,
        [
            "VIP unreads",
            "External connections",
            "Direct messages",
            "Starred",
            "Channels"
        ]
    );
}

#[test]
fn is_vip_channel_from_vip_users_and_metadata() {
    let mut ws = Workspace {
        team_id: "T".into(),
        name: "Test".into(),
        url: "https://test.slack.com".into(),
        self_user_id: "U_SELF".into(),
        activity_unread_count: None,
        channels: BTreeMap::new(),
        starred_order: Vec::new(),
        dm_order: Vec::new(),
        recent_channels: Vec::new(),
        last_active_channel: None,
        priority_scores: BTreeMap::new(),
        frecency: BTreeMap::new(),
        hide_read_channels_unless_starred: false,
        priority_sidebar_section: true,
        vip_users: HashSet::new(),
        sidebar: SidebarConfig::default(),
        users: HashMap::new(),
        custom_emoji: HashMap::new(),
        messages: HashMap::new(),
        typing: HashMap::new(),
        presence: HashMap::new(),
        self_dnd: Default::default(),
        active_huddles: HashMap::new(),
        rt: RealtimeStatus::default(),
        rt_generation: 0,
    };
    ws.vip_users.insert("U_ALFIE".into());

    let vip_dm = Channel {
        id: "D_VIP".into(),
        is_im: true,
        user: Some("U_ALFIE".into()),
        ..Default::default()
    };
    assert!(is_vip_channel(&ws, &vip_dm));

    let vip_channel = Channel {
        id: "C_VIP_POST".into(),
        is_channel: true,
        ..Default::default()
    };
    let cm = ws.messages.entry("C_VIP_POST".into()).or_default();
    cm.upsert(SlackMessage {
        ts: Some("100.000000".into()),
        user: Some("U_OTHER".into()),
        ..Default::default()
    });
    cm.upsert(SlackMessage {
        ts: Some("200.000000".into()),
        user: Some("U_ALFIE".into()),
        ..Default::default()
    });
    assert!(is_vip_channel(&ws, &vip_channel));

    let plain_dm = Channel {
        id: "D_PLAIN".into(),
        is_im: true,
        user: Some("U_OTHER".into()),
        ..Default::default()
    };
    assert!(!is_vip_channel(&ws, &plain_dm));

    let mut named_vip = Channel {
        id: "C_NAMED".into(),
        is_channel: true,
        ..Default::default()
    };
    named_vip.extra.insert(
        "sidebar_section_name".into(),
        serde_json::json!("VIP unreads"),
    );
    assert!(is_vip_channel(&ws, &named_vip));
}

#[test]
fn channel_label_variants() {
    let public = Channel {
        id: "C1".into(),
        name: Some("general".into()),
        is_channel: true,
        ..Default::default()
    };
    assert_eq!(channel_label(&public), "#general");

    let dm = Channel {
        id: "D1".into(),
        name: Some("alice".into()),
        is_im: true,
        ..Default::default()
    };
    assert_eq!(channel_label(&dm), "alice");

    let unnamed = Channel {
        id: "C9".into(),
        ..Default::default()
    };
    assert_eq!(channel_label(&unnamed), "C9");
}
