use super::*;
use crate::slack::models::{
    Attachment, BootData, BootSelf, BotProfile, Emoji, MessageIcons, Reaction, UserProfile,
};

fn msg(ts: &str, text: &str) -> SlackMessage {
    SlackMessage {
        ts: Some(ts.to_owned()),
        text: Some(text.to_owned()),
        ..Default::default()
    }
}

#[test]
fn ts_key_orders_numerically_not_lexically() {
    assert!(ts_key("1783372360.000009") < ts_key("1783372360.000010"));
    assert!(ts_key("999.1") < ts_key("1000.0"));
}

#[test]
fn ordinal_day_suffixes_match_slack_date_labels() {
    assert_eq!(ordinal_day(1), "1st");
    assert_eq!(ordinal_day(2), "2nd");
    assert_eq!(ordinal_day(3), "3rd");
    assert_eq!(ordinal_day(4), "4th");
    assert_eq!(ordinal_day(11), "11th");
    assert_eq!(ordinal_day(22), "22nd");
}

#[test]
fn upsert_keeps_ascending_order_and_dedupes() {
    let mut cm = ChannelMessages::default();
    assert!(cm.upsert(msg("1783372360.741769", "b")));
    assert!(cm.upsert(msg("1783372350.000000", "a")));
    assert!(cm.upsert(msg("1783372370.000000", "c")));
    assert!(!cm.upsert(msg("1783372360.741769", "b-edited")));

    let texts: Vec<_> = cm
        .messages
        .iter()
        .map(|m| m.text.clone().unwrap())
        .collect();
    assert_eq!(texts, vec!["a", "b-edited", "c"]);
}

#[test]
fn remove_by_ts() {
    let mut cm = ChannelMessages::default();
    cm.upsert(msg("100.0", "x"));
    cm.upsert(msg("200.0", "y"));
    assert!(cm.remove("100.0"));
    assert!(!cm.remove("100.0"));
    assert_eq!(cm.messages.len(), 1);
    assert_eq!(cm.messages[0].text.as_deref(), Some("y"));
}

#[test]
fn confirm_reconciles_pending_by_client_msg_id() {
    let mut cm = ChannelMessages::default();
    let mut pending = msg("9999999999.000000", "hi");
    pending.client_msg_id = Some("cid-1".to_owned());
    cm.upsert(pending);
    cm.pending.push("9999999999.000000".to_owned());
    assert!(cm.is_pending("9999999999.000000"));

    let mut confirmed = msg("1783372400.111111", "hi");
    confirmed.client_msg_id = Some("cid-1".to_owned());
    assert!(cm.confirm("cid-1", confirmed));

    assert_eq!(cm.messages.len(), 1);
    assert_eq!(cm.messages[0].ts.as_deref(), Some("1783372400.111111"));
    assert!(!cm.is_pending("1783372400.111111"));
}

#[test]
fn latest_confirmed_ts_ignores_optimistic_pending_messages() {
    let mut cm = ChannelMessages::default();
    cm.upsert(msg("1783372400.111111", "confirmed"));
    cm.upsert(msg("9999999999.000000", "pending"));
    cm.pending.push("9999999999.000000".to_owned());

    assert_eq!(
        cm.latest_confirmed_ts().as_deref(),
        Some("1783372400.111111")
    );
}

#[test]
fn matching_pending_confirm_skips_ambiguous_duplicates() {
    let mut cm = ChannelMessages::default();
    cm.upsert(msg("9999999999.000001", "hi"));
    cm.upsert(msg("9999999999.000002", "hi"));
    cm.pending.push("9999999999.000001".to_owned());
    cm.pending.push("9999999999.000002".to_owned());

    assert!(!cm.confirm_matching_pending(None, Some("hi"), msg("1783372400.111111", "hi")));
    assert_eq!(cm.messages.len(), 2);
    assert!(cm.is_pending("9999999999.000001"));
    assert!(cm.is_pending("9999999999.000002"));
}

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

#[test]
fn message_text_fallbacks() {
    assert_eq!(message_text(&msg("1.0", "hello")), "hello");

    let file_only = SlackMessage {
        ts: Some("1.0".into()),
        files: vec![File {
            name: Some("mock.png".into()),
            ..Default::default()
        }],
        ..Default::default()
    };
    assert_eq!(message_text(&file_only), "");

    let empty = SlackMessage {
        ts: Some("1.0".into()),
        text: Some("   ".into()),
        subtype: Some("channel_join".into()),
        ..Default::default()
    };
    assert_eq!(message_text(&empty), "[channel_join]");

    let bare = SlackMessage {
        ts: Some("1.0".into()),
        ..Default::default()
    };
    assert_eq!(message_text(&bare), "[no text]");
}

#[test]
fn visible_message_unwraps_message_replied_envelope() {
    let envelope = SlackMessage {
        subtype: Some("message_replied".into()),
        channel: Some("C1".into()),
        reply_count: Some(2),
        message: Some(Box::new(SlackMessage {
            user: Some("U1".into()),
            ts: Some("1.0".into()),
            text: Some("actual".into()),
            ..Default::default()
        })),
        ..Default::default()
    };

    let visible = visible_message(envelope);
    assert_eq!(visible.user.as_deref(), Some("U1"));
    assert_eq!(visible.text.as_deref(), Some("actual"));
    assert_eq!(visible.channel.as_deref(), Some("C1"));
    assert_eq!(visible.reply_count, Some(2));
    assert_ne!(visible.subtype.as_deref(), Some("message_replied"));
}

#[test]
fn channel_timeline_hides_thread_replies_but_keeps_roots_and_broadcasts() {
    let root = SlackMessage {
        ts: Some("100.0".into()),
        thread_ts: Some("100.0".into()),
        reply_count: Some(3),
        ..Default::default()
    };
    assert!(is_channel_timeline_visible(&root));

    let plain = SlackMessage {
        ts: Some("101.0".into()),
        ..Default::default()
    };
    assert!(is_channel_timeline_visible(&plain));

    let reply = SlackMessage {
        ts: Some("102.0".into()),
        thread_ts: Some("100.0".into()),
        ..Default::default()
    };
    assert!(!is_channel_timeline_visible(&reply));

    let broadcast = SlackMessage {
        ts: Some("103.0".into()),
        thread_ts: Some("100.0".into()),
        subtype: Some("thread_broadcast".into()),
        ..Default::default()
    };
    assert!(is_channel_timeline_visible(&broadcast));

    let replied_envelope = SlackMessage {
        ts: Some("104.0".into()),
        subtype: Some("message_replied".into()),
        ..Default::default()
    };
    assert!(!is_channel_timeline_visible(&replied_envelope));
}

#[test]
fn file_summary_prefers_title_type_and_size() {
    let file = File {
        id: Some("F1".into()),
        name: Some("report.pdf".into()),
        title: Some("Quarterly report".into()),
        pretty_type: Some("PDF".into()),
        size: Some(1_572_864),
        ..Default::default()
    };

    assert_eq!(file_title(&file), "Quarterly report");
    assert_eq!(file_summary(&file), "PDF - 1.5 MB");
    assert_eq!(format_file_size(512), "512 B");
    assert_eq!(format_file_size(2048), "2.0 KB");
}

#[test]
fn file_download_name_sanitizes_paths() {
    let file = File {
        name: Some("../bad/name?.png".into()),
        title: Some("ignored".into()),
        ..Default::default()
    };
    assert_eq!(file_download_name(&file), "_bad_name_.png");

    let fallback = File::default();
    assert_eq!(file_download_name(&fallback), "download");
}

#[test]
fn file_preview_uses_largest_known_thumb_and_stable_key() {
    let mut file = File {
        id: Some("F123".into()),
        thumb_64: Some("https://files/thumb-64.png".into()),
        thumb_160: Some("https://files/thumb-160.png".into()),
        thumb_360: Some("https://files/thumb-360.png".into()),
        ..Default::default()
    };
    file.extra.insert(
        "thumb_1024".into(),
        serde_json::json!("https://files/thumb-1024.png"),
    );

    assert_eq!(file_preview_key(&file).as_deref(), Some("F123"));
    assert_eq!(
        file_preview_url(&file),
        Some("https://files/thumb-1024.png")
    );

    let without_id = File {
        thumb_80: Some("https://files/thumb-80.png".into()),
        ..Default::default()
    };
    assert_eq!(
        file_preview_key(&without_id).as_deref(),
        Some("https://files/thumb-80.png")
    );
}

#[test]
fn heic_uses_slack_raster_preview_while_video_uses_original() {
    let mut heic = File {
        name: Some("camera.heic".into()),
        mimetype: Some("image/heic".into()),
        filetype: Some("heic".into()),
        url_private: Some("https://files.slack.com/camera.heic".into()),
        thumb_360: Some("https://files.slack.com/camera-360.jpg".into()),
        ..Default::default()
    };
    heic.extra.insert(
        "thumb_1024".into(),
        serde_json::json!("https://files.slack.com/camera-1024.jpg"),
    );
    assert!(is_image_file(&heic));
    assert!(!file_original_is_viewer_decodable(&heic));
    assert_eq!(
        file_viewer_url(&heic),
        Some("https://files.slack.com/camera-1024.jpg")
    );

    let video = File {
        name: Some("demo.mp4".into()),
        mimetype: Some("video/mp4".into()),
        url_private: Some("https://files.slack.com/files-tmb/demo.mp4".into()),
        extra: BTreeMap::from([
            (
                "mp4".into(),
                serde_json::json!("https://files.slack.com/files-tmb/demo.mp4"),
            ),
            (
                "thumb_video".into(),
                serde_json::json!("https://files.slack.com/files-tmb/demo.jpeg"),
            ),
            (
                "url_private_download".into(),
                serde_json::json!("https://files.slack.com/files-pri/download/demo.mp4"),
            ),
        ]),
        ..Default::default()
    };
    assert!(is_video_file(&video));
    assert_eq!(
        file_viewer_url(&video),
        Some("https://files.slack.com/files-tmb/demo.mp4")
    );
    assert_eq!(
        file_preview_url(&video),
        Some("https://files.slack.com/files-tmb/demo.jpeg")
    );
    assert_eq!(
        file_download_url(&video),
        Some("https://files.slack.com/files-pri/download/demo.mp4")
    );
}

#[test]
fn emoji_text_tokens_extracts_shortcodes_and_display_text_keeps_custom() {
    assert_eq!(
        emoji_text_tokens("ship it :wave: :party-hack:"),
        vec![
            EmojiTextToken::Text("ship it ".into()),
            EmojiTextToken::Emoji("wave".into()),
            EmojiTextToken::Text(" ".into()),
            EmojiTextToken::Emoji("party-hack".into()),
        ]
    );
    assert_eq!(
        emoji_text_to_display("ship it :wave: :party-hack:"),
        "ship it 👋 :party-hack:"
    );
}

#[test]
fn custom_emoji_url_resolves_aliases() {
    let mut ws = Workspace::from_session(&crate::config::WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U_SELF".into(),
        name: "Test".into(),
        url: "https://test.slack.com".into(),
        token: "xoxc-test".into(),
    });
    ws.apply_emojis(vec![
        Emoji {
            name: "party-hack".into(),
            value: "https://emoji.test/party.png".into(),
            ..Default::default()
        },
        Emoji {
            name: "party-alias".into(),
            value: "alias:party-hack".into(),
            ..Default::default()
        },
    ]);

    assert_eq!(
        ws.custom_emoji_url("party-alias"),
        Some("https://emoji.test/party.png")
    );
}

#[test]
fn reaction_summary_format() {
    let r = Reaction {
        name: "thumbsup".into(),
        count: 3,
        ..Default::default()
    };
    assert_eq!(reaction_summary(&r), "👍 3");

    let custom = Reaction {
        name: "hackclub_bug".into(),
        count: 1,
        ..Default::default()
    };
    assert_eq!(reaction_summary(&custom), ":hackclub_bug: 1");
}

#[test]
fn applies_reaction_add_remove_without_double_counting() {
    let mut cm = ChannelMessages::default();
    cm.upsert(msg("1.0", "hello"));

    assert!(cm.apply_reaction("1.0", "U1", "thumbsup", true));
    assert_eq!(cm.messages[0].reactions[0].count, 1);
    assert_eq!(cm.messages[0].reactions[0].users, vec!["U1"]);

    assert!(!cm.apply_reaction("1.0", "U1", "thumbsup", true));
    assert_eq!(cm.messages[0].reactions[0].count, 1);

    assert!(cm.apply_reaction("1.0", "U2", "thumbsup", true));
    assert_eq!(cm.messages[0].reactions[0].count, 2);

    assert!(cm.apply_reaction("1.0", "U1", "thumbsup", false));
    assert_eq!(cm.messages[0].reactions[0].count, 1);
    assert_eq!(cm.messages[0].reactions[0].users, vec!["U2"]);

    assert!(cm.apply_reaction("1.0", "U2", "thumbsup", false));
    assert!(cm.messages[0].reactions.is_empty());
}

#[test]
fn prune_typing_drops_stale() {
    let mut ws = Workspace {
        team_id: "T1".into(),
        name: "test".into(),
        url: "https://t".into(),
        self_user_id: "USELF".into(),
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
        active_huddles: HashMap::new(),
        rt: RealtimeStatus::default(),
        rt_generation: 0,
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

#[test]
fn is_browser_url_accepts_only_http_and_https() {
    assert!(is_browser_url("https://slack.com/archives/C1/p1"));
    assert!(is_browser_url("http://example.com"));
    assert!(!is_browser_url("file:///etc/passwd"));
    assert!(!is_browser_url("javascript:alert(1)"));
    assert!(!is_browser_url(""));
    assert!(!is_browser_url("ftp://example.com"));
}

#[test]
fn authenticated_url_check_rejects_lookalike_hosts() {
    assert!(is_slack_authenticated_url(
        "https://files.slack.com/files-pri/T/F/image.png"
    ));
    assert!(is_slack_authenticated_url(
        "https://workspace.slack.com/files/image.png"
    ));
    assert!(!is_slack_authenticated_url(
        "https://files.slack.com.evil.example/image.png"
    ));
    assert!(!is_slack_authenticated_url(
        "https://slack.com@evil.example/image.png"
    ));
}

#[test]
fn attachment_viewer_prefers_original_and_names_download() {
    let attachment = Attachment {
        title: Some("Launch / board.png".into()),
        image_url: Some("https://cdn.example.com/original/board.png?size=large".into()),
        thumb_url: Some("https://cdn.example.com/thumb/board.png".into()),
        ..Default::default()
    };
    assert_eq!(
        attachment_viewer_url(&attachment),
        Some("https://cdn.example.com/original/board.png?size=large")
    );
    assert_eq!(attachment_download_name(&attachment), "Launch _ board.png");
}

#[test]
fn gif_picker_attachment_exposes_nested_image_block() {
    let message: SlackMessage = serde_json::from_value(serde_json::json!({
        "type": "message",
        "user": "U08TCSANHDX",
        "text": "",
        "ts": "1785200205.163019",
        "files": [],
        "blocks": [],
        "attachments": [{
            "id": 1,
            "fallback": "shared a GIF",
            "blocks": [{
                "type": "image",
                "image_url": "https://media0.giphy.com/media/OIKS4GcqcKqNtndh1o/200w.gif?rid=200w.gif",
                "image_width": 200,
                "image_height": 206,
                "image_bytes": 17943,
                "is_animated": true,
                "alt_text": "Main Character Instagram GIF"
            }]
        }]
    }))
    .expect("decode picker message");

    let images: Vec<_> = attachment_images(&message.attachments[0]).collect();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].width, Some(200));
    assert_eq!(images[0].height, Some(206));
    assert!(images[0].animated);
    assert_eq!(images[0].alt_text, Some("Main Character Instagram GIF"));
    assert!(images[0].preview_url.ends_with("rid=200w.gif"));
    assert_eq!(message_text(&message), "");
}

#[test]
fn image_file_detection_and_uploader_are_defensive() {
    let image = File {
        name: Some("launch.PNG".into()),
        extra: BTreeMap::from([("user".into(), serde_json::json!("U_ALICE"))]),
        ..Default::default()
    };
    assert!(is_image_file(&image));
    assert_eq!(file_uploader_id(&image), Some("U_ALICE"));
    assert!(!is_image_file(&File {
        name: Some("brief.pdf".into()),
        mimetype: Some("application/pdf".into()),
        ..Default::default()
    }));
}

#[test]
fn attachment_download_name_falls_back_to_url_then_image() {
    let from_url = Attachment {
        image_url: Some("https://cdn.example.com/path/design.jpg?width=1200".into()),
        ..Default::default()
    };
    assert_eq!(attachment_download_name(&from_url), "design.jpg");
    assert_eq!(attachment_download_name(&Attachment::default()), "image");
}

#[test]
fn relative_timestamp_uses_compact_hours() {
    let ts = format!("{}.000000", now_secs() - 12 * 60 * 60);
    assert_eq!(format_relative_ts(&ts), "12h ago");
}

#[test]
fn scroll_ratio_for_ts_at_start_is_zero() {
    let messages = vec![msg("1.0", "a"), msg("2.0", "b"), msg("3.0", "c")];
    assert_eq!(scroll_ratio_for_ts(&messages, "1.0"), Some(0.0));
}

#[test]
fn scroll_ratio_for_ts_at_end_is_one() {
    let messages = vec![msg("1.0", "a"), msg("2.0", "b"), msg("3.0", "c")];
    assert_eq!(scroll_ratio_for_ts(&messages, "3.0"), Some(1.0));
}

#[test]
fn scroll_ratio_for_ts_middle_is_between() {
    let messages = vec![msg("1.0", "a"), msg("2.0", "b"), msg("3.0", "c")];
    assert_eq!(scroll_ratio_for_ts(&messages, "2.0"), Some(0.5));
}

#[test]
fn scroll_ratio_for_ts_missing_is_none() {
    let messages = vec![msg("1.0", "a"), msg("2.0", "b")];
    assert_eq!(scroll_ratio_for_ts(&messages, "9.0"), None);
}

#[test]
fn scroll_ratio_for_ts_single_message_is_zero() {
    let messages = vec![msg("1.0", "a")];
    assert_eq!(scroll_ratio_for_ts(&messages, "1.0"), Some(0.0));
}

#[test]
fn scroll_ratio_for_ts_empty_is_none() {
    let messages: Vec<SlackMessage> = Vec::new();
    assert_eq!(scroll_ratio_for_ts(&messages, "1.0"), None);
}
