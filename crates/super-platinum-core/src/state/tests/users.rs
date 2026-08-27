use super::super::*;
use crate::slack::models::{BootData, BootSelf, BotProfile, DndInfo, MessageIcons, UserProfile};

#[test]
fn display_name_fallback_chain() {
    let u = User {
        id: "U1".into(),
        name: Some("uname".into()),
        real_name: Some("Real Name".into()),
        profile: Some(UserProfile {
            display_name: Some("Display".into()),
            real_name: Some("Profile Real".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(display_name(Some(&u), "U1"), "Display");

    let u2 = User {
        id: "U2".into(),
        profile: Some(UserProfile {
            display_name: Some("  ".into()),
            real_name: Some("Profile Real".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(display_name(Some(&u2), "U2"), "Profile Real");

    let u3 = User {
        id: "U3".into(),
        name: Some("uname".into()),
        ..Default::default()
    };
    assert_eq!(display_name(Some(&u3), "U3"), "uname");

    assert_eq!(display_name(None, "U4"), "U4");
}

#[test]
fn message_author_prefers_bot_username_and_profile_over_raw_ids() {
    let ws = Workspace::from_session(&crate::config::WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U_SELF".into(),
        name: "Test".into(),
        url: "https://test.slack.com".into(),
        token: "xoxc-test".into(),
    });
    let msg = SlackMessage {
        user: Some("U_APP".into()),
        bot_id: Some("B_FAKE".into()),
        username: Some("mattsob".into()),
        bot_profile: Some(BotProfile {
            name: Some("app fallback".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(message_author_name(&ws, &msg), "mattsob");

    let profile_only = SlackMessage {
        bot_id: Some("B_FAKE".into()),
        bot_profile: Some(BotProfile {
            name: Some("slimebot".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(message_author_name(&ws, &profile_only), "slimebot");
}

#[test]
fn message_bot_avatar_prefers_profile_icon() {
    let msg = SlackMessage {
        bot_id: Some("B_FAKE".into()),
        bot_profile: Some(BotProfile {
            icons: Some(MessageIcons {
                image_48: Some("https://example.test/bot-48.png".into()),
                image_72: Some("https://example.test/bot-72.png".into()),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(
        message_bot_avatar(&msg),
        Some((
            "bot-icon:B_FAKE:https://example.test/bot-48.png".into(),
            "https://example.test/bot-48.png".into()
        ))
    );
}

#[test]
fn message_bot_avatar_key_includes_per_message_icon_url() {
    let mut first = SlackMessage {
        bot_id: Some("B_SAME".into()),
        icons: Some(MessageIcons {
            image_48: Some("https://example.test/first.png".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut second = first.clone();
    second.icons = Some(MessageIcons {
        image_48: Some("https://example.test/second.png".into()),
        ..Default::default()
    });

    let first_avatar = message_bot_avatar(&first).expect("first avatar");
    let second_avatar = message_bot_avatar(&second).expect("second avatar");

    assert_ne!(first_avatar.0, second_avatar.0);
    assert_eq!(first_avatar.1, "https://example.test/first.png");
    assert_eq!(second_avatar.1, "https://example.test/second.png");

    first.icons = second.icons.clone();
    assert_eq!(message_bot_avatar(&first), Some(second_avatar));
}

#[test]
fn user_avatar_url_prefers_profile_image_48() {
    let user = User {
        id: "U1".into(),
        profile: Some(UserProfile {
            image_32: Some("https://example.test/32.png".into()),
            image_48: Some("https://example.test/48.png".into()),
            image_72: Some("https://example.test/72.png".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(user_avatar_url(&user), Some("https://example.test/48.png"));

    let fallback = User {
        id: "U2".into(),
        profile: Some(UserProfile {
            image_48: Some(" ".into()),
            image_32: Some("https://example.test/32.png".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(
        user_avatar_url(&fallback),
        Some("https://example.test/32.png")
    );

    let original_only = User {
        id: "U3".into(),
        profile: Some(UserProfile {
            image_original: Some("https://example.test/original.png".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(
        user_avatar_url(&original_only),
        Some("https://example.test/original.png")
    );

    let hash_only = User {
        id: "U4".into(),
        profile: Some(UserProfile {
            avatar_hash: Some("31dc9a4e9298".into()),
            team: Some("E1".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(user_avatar_url(&hash_only), None);
}

#[test]
fn boot_self_user_provides_self_avatar_url() {
    let mut ws = Workspace {
        team_id: "T1".into(),
        name: "test".into(),
        url: "https://t".into(),
        self_user_id: "U_SESSION".into(),
        activity_unread_count: None,
        channels: BTreeMap::new(),
        starred_order: Vec::new(),
        dm_order: Vec::new(),
        recent_channels: Vec::new(),
        last_active_channel: None,
        priority_scores: BTreeMap::new(),
        frecency: BTreeMap::new(),
        hide_read_channels_unless_starred: false,
        priority_sidebar_section: false,
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

    ws.apply_boot(BootData {
        self_user: BootSelf {
            id: "U_SELF".into(),
            name: Some("rowan".into()),
            profile: Some(UserProfile {
                image_48: Some("https://example.test/self.png".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    });

    assert_eq!(ws.self_user_id, "U_SELF");
    assert_eq!(
        ws.avatar_url("U_SELF"),
        Some("https://example.test/self.png".into())
    );
}

#[test]
fn partial_boot_user_preserves_cached_profile_identity_and_avatar() {
    let mut ws = Workspace::from_session(&crate::config::WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U_SELF".into(),
        name: "Test".into(),
        url: "https://test.slack.com".into(),
        token: "xoxc-test".into(),
    });
    ws.users.insert(
        "U1".into(),
        User {
            id: "U1".into(),
            name: Some("cached-name".into()),
            real_name: Some("Cached Name".into()),
            profile: Some(UserProfile {
                display_name: Some("Cached Display".into()),
                image_original: Some("https://example.test/avatar.png".into()),
                status_text: Some("old status".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    ws.apply_boot(BootData {
        users: vec![User {
            id: "U1".into(),
            name: Some(String::new()),
            profile: Some(UserProfile {
                status_text: Some(String::new()),
                ..Default::default()
            }),
            ..Default::default()
        }],
        ..Default::default()
    });

    let user = ws.users.get("U1").expect("merged user");
    assert_eq!(display_name(Some(user), "U1"), "Cached Display");
    assert_eq!(
        user_avatar_url(user),
        Some("https://example.test/avatar.png")
    );
    assert_eq!(
        user.profile.as_ref().unwrap().status_text.as_deref(),
        Some("")
    );
}

#[test]
fn snooze_covers_manual_snooze_and_the_open_dnd_window() {
    // A scheduled window is reported even when it has not started yet, so
    // `dnd_enabled` on its own must not light the badge.
    let scheduled = DndInfo {
        dnd_enabled: true,
        next_dnd_start_ts: Some(200),
        next_dnd_end_ts: Some(300),
        ..Default::default()
    };
    assert!(!scheduled.is_snoozed(100));
    assert!(scheduled.is_snoozed(250));
    assert!(!scheduled.is_snoozed(300));

    // A manual snooze stands on its own, and expires with its end time.
    let snoozed = DndInfo {
        snooze_enabled: true,
        snooze_endtime: Some(150),
        ..Default::default()
    };
    assert!(snoozed.is_snoozed(100));
    assert!(!snoozed.is_snoozed(150));

    assert!(!DndInfo::default().is_snoozed(100));
}

#[test]
fn profile_pane_avatar_prefers_a_sized_variant_over_the_upload() {
    let user = User {
        id: "U1".into(),
        profile: Some(UserProfile {
            image_72: Some("https://example.test/72.png".into()),
            image_512: Some("https://example.test/512.png".into()),
            image_original: Some("https://example.test/original.png".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(
        user_profile_image_url(&user),
        Some("https://example.test/512.png")
    );

    let upload_only = User {
        id: "U2".into(),
        profile: Some(UserProfile {
            image_original: Some("https://example.test/original.png".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(
        user_profile_image_url(&upload_only),
        Some("https://example.test/original.png")
    );
}
