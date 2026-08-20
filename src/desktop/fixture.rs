pub(crate) fn fixture_core() -> super_platinum_core::CoreAppState {
    let mut core =
        super_platinum_core::CoreAppState::new(super_platinum_core::config::Settings::default());
    core.screen = super_platinum_core::state::Screen::Main;
    core.active_team = Some("T1".into());
    core.active_channel = Some("C2".into());
    let session = super_platinum_core::config::WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U0".into(),
        name: "Echonet".into(),
        url: "https://echonet.slack.com".into(),
        token: String::new(),
    };
    let mut workspace = super_platinum_core::state::Workspace::from_session(&session);
    workspace.last_active_channel = Some("C2".into());
    workspace.priority_sidebar_section = true;
    workspace.hide_read_channels_unless_starred = false;
    for channel in [
        serde_json::json!({"id":"C1","name":"general","is_channel":true,"is_starred":true}),
        serde_json::json!({"id":"C2","name":"ship","is_channel":true,"has_unreads":true,"unread_count":2,"unread_count_display":2,"mention_count":1,"last_read":"1.0","topic":{"value":"Ship the desktop client without losing details."}}),
        serde_json::json!({"id":"C3","name":"design","is_channel":true}),
        serde_json::json!({"id":"C4","name":"random","is_channel":true}),
        serde_json::json!({"id":"C5","name":"ops-private","is_channel":true,"is_private":true,"is_group":true}),
        serde_json::json!({"id":"C6","name":"vercel-embassy","is_channel":true,"is_ext_shared":true}),
        serde_json::json!({"id":"D1","name":"Maya Chen","is_im":true,"user":"U1","has_unreads":true,"unread_count":1,"unread_count_display":1}),
        serde_json::json!({"id":"D2","name":"Jules","is_im":true,"user":"U2"}),
        serde_json::json!({"id":"G1","name":"mpdm-maya--jules--you-1","is_mpim":true,"is_group":true,"is_private":true}),
    ] {
        let channel: super_platinum_core::slack::models::Channel =
            serde_json::from_value(channel).expect("fixture channel");
        workspace.channels.insert(channel.id.clone(), channel);
    }
    workspace.starred_order = vec!["C1".into()];
    workspace.recent_channels = vec![
        "D1".into(),
        "D2".into(),
        "C2".into(),
        "C1".into(),
        "C3".into(),
    ];
    for user in [
        serde_json::json!({"id":"U0","name":"you","real_name":"You","profile":{"display_name":"You","image_72":"https://example.test/you.png"}}),
        serde_json::json!({"id":"U1","name":"maya","real_name":"Maya Chen","tz_offset":-25200,"profile":{"display_name":"Maya Chen","title":"Desktop engineer","status_text":"Shipping the migration","status_emoji":":ship:","email":"maya@example.com","pronouns":"she/her","image_72":"https://example.test/maya.png","image_512":"https://example.test/maya-lg.png"}}),
        serde_json::json!({"id":"U2","name":"jules","real_name":"Jules","profile":{"display_name":"Jules","image_72":"https://example.test/jules.png"}}),
    ] {
        let user: super_platinum_core::slack::models::User =
            serde_json::from_value(user).expect("fixture user");
        workspace.users.insert(user.id.clone(), user);
    }
    workspace
        .presence
        .insert("U1".into(), super_platinum_core::state::Presence::Active);
    workspace
        .presence
        .insert("U2".into(), super_platinum_core::state::Presence::Away);
    let messages = vec![
        super_platinum_core::slack::models::Message {
            user: Some("U1".into()),
            channel: Some("C2".into()),
            ts: Some("1719800000.000100".into()),
            text: Some("Morning! The desktop migration branch is ready for review.".into()),
            reply_count: Some(3),
            ..Default::default()
        },
        super_platinum_core::slack::models::Message {
            user: Some("U2".into()),
            channel: Some("C2".into()),
            ts: Some("1719800400.000200".into()),
            text: Some("Cached content stays visible if refresh fails.".into()),
            ..Default::default()
        },
        super_platinum_core::slack::models::Message {
            bot_id: Some("B1".into()),
            username: Some("Deploy Bot".into()),
            subtype: Some("bot_message".into()),
            channel: Some("C2".into()),
            ts: Some("1719800800.000300".into()),
            text: Some("Build #482 passed on main.".into()),
            bot_profile: Some(super_platinum_core::slack::models::BotProfile {
                id: Some("B1".into()),
                name: Some("Deploy Bot".into()),
                icons: Some(super_platinum_core::slack::models::MessageIcons {
                    image_72: Some("https://example.test/bot.png".into()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        },
    ];
    let channel_messages = super_platinum_core::state::ChannelMessages {
        messages,
        loaded: true,
        has_more_older: true,
        unread_count: 2,
        mention_count: 1,
        last_read: Some("1719800000.000100".into()),
        ..Default::default()
    };
    workspace.messages.insert("C2".into(), channel_messages);
    core.dms.upsert(
        serde_json::from_value(serde_json::json!({
            "id":"D1", "latest":"1719801000.000400",
            "message":{"user":"U1","channel":"D1","ts":"1719801000.000400","text":"Can you review the desktop capture?"},
            "channel":{"id":"D1","name":"Maya Chen","is_im":true,"user":"U1","unread_count":1,"unread_count_display":1}
        }))
        .expect("fixture DM"),
    );
    core.activity.upsert(
        serde_json::from_value(serde_json::json!({
            "is_unread":true,"feed_ts":"1719801200.000500","key":"mention-1",
            "item":{"type":"mention","message":{"channel":"C2","ts":"1719800400.000200","author_user_id":"U2","text":"@You can you check the desktop captures?"}}
        }))
        .expect("fixture activity"),
    );
    core.activity.upsert(
        serde_json::from_value(serde_json::json!({
            "is_unread":false,"feed_ts":"1719801300.000600","key":"reaction-1",
            "item":{"type":"message_reaction","message":{"channel":"C2","ts":"1719800000.000100","author_user_id":"U1"},"reaction":{"name":"eyes","user":"U2"}}
        }))
        .expect("fixture activity reaction"),
    );
    core.activity.upsert(
        serde_json::from_value(serde_json::json!({
            "is_unread":true,"feed_ts":"1719801400.000700","key":"thread-1",
            "item":{"type":"thread_v2","bundle_info":{"payload":{"thread_entry":{"channel_id":"C2","thread_ts":"1719800000.000100","latest_ts":"1719801000.000400","unread_msg_count":2}}}}
        }))
        .expect("fixture activity thread"),
    );
    core.activity.upsert(
        serde_json::from_value(serde_json::json!({
            "is_unread":true,"feed_ts":"1719801500.000800","key":"dm-act-1",
            "item":{"type":"dm","message":{"channel":"D1","ts":"1719801000.000400","author_user_id":"U1","text":"Can you review the desktop capture?"}}
        }))
        .expect("fixture activity dm"),
    );
    core.workspaces.insert("T1".into(), workspace);
    core
}
