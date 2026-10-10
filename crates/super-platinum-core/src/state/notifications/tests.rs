use super::*;
use crate::slack::models::{Channel, DndInfo};
use serde_json::json;

fn workspace() -> Workspace {
    let mut ws = Workspace::from_session(&crate::config::WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U_SELF".into(),
        name: "Test".into(),
        url: "https://test".into(),
        token: String::new(),
    });
    ws.channels.insert(
        "D1".into(),
        Channel {
            id: "D1".into(),
            is_im: true,
            ..Default::default()
        },
    );
    ws.channels.insert(
        "C1".into(),
        Channel {
            id: "C1".into(),
            is_channel: true,
            ..Default::default()
        },
    );
    ws.notifications.apply_change("all_notifications_prefs", json!({
        "global": {"global_desktop": "mentions_dms", "global_mpdm_desktop": "everything",
            "global_desktop_push_enabled": true, "global_keywords": "echo, ro, counter strike",
            "threads_everything": false, "desktop_sound": "b2.mp3", "priority_desktop_sound": "knock_brush.mp3"},
        "channels": {}
    }));
    ws
}

fn message(channel: &str, text: &str) -> Message {
    Message {
        channel: Some(channel.into()),
        text: Some(text.into()),
        user: Some("U_OTHER".into()),
        ts: Some("200.0".into()),
        ..Default::default()
    }
}

fn delivers(ws: &Workspace, m: &Message) -> bool {
    ws.notifications.message_options(ws, m, 100).is_some()
}

#[test]
fn workspace_defaults_keywords_and_direct_messages() {
    let ws = workspace();
    for (channel, text, expected) in [
        ("C1", "hello", false),
        ("C1", "<@U_SELF> hi", true),
        ("C1", "<@U_SELF|name> hi", true),
        ("D1", "hello", true),
        ("C1", "ECHO!", true),
        ("C1", "hello ro", true),
        ("C1", "problem", false),
        ("C1", "echolocation", false),
        ("C1", "Counter Strike!", true),
        ("C1", "<https://echo.test|link>", false),
        ("C1", "@echo", false),
    ] {
        assert_eq!(
            delivers(&ws, &message(channel, text)),
            expected,
            "{channel} {text}"
        );
    }
}

#[test]
fn push_overrides_are_independent_of_message_filter_and_mobile() {
    let mut ws = workspace();
    ws.notifications.channels.insert(
        "C1".into(),
        serde_json::from_value(json!({"desktop":"everything", "mobile":"nothing"})).unwrap(),
    );
    assert!(delivers(&ws, &message("C1", "hello")));
    ws.notifications
        .global
        .insert("global_desktop_push_enabled".into(), json!(false));
    assert!(!delivers(&ws, &message("C1", "<@U_SELF>")));
    ws.notifications
        .channels
        .get_mut("C1")
        .unwrap()
        .insert("desktop_push_enabled".into(), json!(true));
    assert!(delivers(&ws, &message("C1", "hello")));
    ws.notifications
        .channels
        .get_mut("C1")
        .unwrap()
        .insert("desktop_push_enabled".into(), json!(false));
    assert!(!delivers(&ws, &message("C1", "<@U_SELF>")));
}

#[test]
fn muted_channels_allow_personal_mentions_but_muted_dms_stay_silent() {
    let mut ws = workspace();
    for channel in ["C1", "D1"] {
        ws.notifications.channels.insert(
            channel.into(),
            serde_json::from_value(json!({"muted":true})).unwrap(),
        );
    }
    assert!(delivers(&ws, &message("C1", "<@U_SELF>")));
    for text in ["echo", "<!channel>", "hello"] {
        assert!(!delivers(&ws, &message("C1", text)));
    }
    assert!(!delivers(&ws, &message("D1", "<@U_SELF>")));
    ws.notifications
        .channels
        .get_mut("C1")
        .unwrap()
        .insert("desktop_push_enabled".into(), json!(false));
    assert!(!delivers(&ws, &message("C1", "<@U_SELF>")));
}

#[test]
fn temporary_mutes_expire_and_broadcast_suppression_is_scoped() {
    let mut ws = workspace();
    ws.notifications.channels.insert(
        "C1".into(),
        serde_json::from_value(json!({"temp_mute_expiration_ts":150,"suppress_at_channel":true}))
            .unwrap(),
    );
    assert!(!delivers(&ws, &message("C1", "echo")));
    assert!(
        ws.notifications
            .message_options(&ws, &message("C1", "echo"), 151)
            .is_some()
    );
    assert!(
        ws.notifications
            .message_options(&ws, &message("C1", "<!channel>"), 151)
            .is_none()
    );
    assert!(delivers(&ws, &message("C1", "<@U_SELF>")));
}

#[test]
fn thread_replies_require_a_personal_mention_or_server_selected_enabled_reply() {
    let mut ws = workspace();
    let mut reply = message("C1", "echo <!channel>");
    reply.thread_ts = Some("100.0".into());
    assert!(!delivers(&ws, &reply));
    reply.text = Some("<@U_SELF>".into());
    assert!(delivers(&ws, &reply));
    reply.text = Some("a reply".into());
    reply.kind = Some("desktop_notification".into());
    assert!(!delivers(&ws, &reply));
    ws.notifications
        .global
        .insert("threads_everything".into(), json!(true));
    assert!(delivers(&ws, &reply));
    ws.notifications.channels.insert(
        "C1".into(),
        serde_json::from_value(json!({"follow_all_threads":false})).unwrap(),
    );
    assert!(!delivers(&ws, &reply));
    ws.notifications
        .channels
        .get_mut("C1")
        .unwrap()
        .insert("follow_all_threads".into(), json!(true));
    assert!(delivers(&ws, &reply));
}

#[test]
fn dnd_vip_override_and_sound_selection() {
    let mut ws = workspace();
    ws.self_dnd = DndInfo {
        snooze_enabled: true,
        snooze_endtime: Some(200),
        ..Default::default()
    };
    let m = message("D1", "hello");
    assert!(!delivers(&ws, &m));
    ws.vip_users.insert("U_OTHER".into());
    assert!(!delivers(&ws, &m));
    ws.notifications
        .apply_change("priority_dnd_override", json!(true));
    assert_eq!(
        ws.notifications
            .message_options(&ws, &m, 100)
            .unwrap()
            .sound
            .as_deref(),
        Some("knock_brush.mp3")
    );
    ws.notifications.apply_change("mute_sounds", json!(true));
    assert_eq!(
        ws.notifications
            .message_options(&ws, &m, 100)
            .unwrap()
            .sound,
        None
    );
    assert!(!delivers(&ws, &message("C1", "echo")));
    ws.notifications
        .global
        .insert("priority_desktop_sound".into(), json!("none"));
    ws.notifications.apply_change("mute_sounds", json!(false));
    assert_eq!(
        ws.notifications
            .message_options(&ws, &m, 100)
            .unwrap()
            .sound
            .as_deref(),
        Some("b2.mp3")
    );
}

#[test]
fn privacy_and_huddle_sounds_use_separate_preferences() {
    let mut ws = workspace();
    ws.notifications
        .global
        .insert("no_text_in_notifications".into(), json!(true));
    assert!(
        !ws.notifications
            .message_options(&ws, &message("D1", "secret"), 100)
            .unwrap()
            .preview
    );
    ws.notifications.apply_change("mute_sounds", json!(true));
    assert_eq!(
        ws.notifications
            .huddle_options(&ws, "D1", 100)
            .unwrap()
            .sound
            .as_deref(),
        Some("boop_remix.mp3")
    );
    ws.notifications
        .apply_change("mute_huddle_sounds", json!(true));
    assert_eq!(
        ws.notifications
            .huddle_options(&ws, "D1", 100)
            .unwrap()
            .sound,
        None
    );
    ws.self_dnd.snooze_enabled = true;
    assert!(ws.notifications.huddle_options(&ws, "D1", 100).is_none());
}

#[test]
fn boot_and_live_string_preferences_preserve_last_good_settings() {
    let mut config = NotificationConfig::default();
    assert!(!config.desktop_enabled("C1"));
    let prefs: BootPrefs = serde_json::from_value(json!({"all_notifications_prefs":"{\"global\":{\"global_desktop_push_enabled\":false},\"channels\":{}}", "mute_sounds":true})).unwrap();
    config.apply_prefs(&prefs);
    assert!(config.loaded);
    assert!(!config.desktop_enabled("C1"));
    config.apply_change("all_notifications_prefs", json!("bad json"));
    assert!(!config.desktop_enabled("C1"));
    config.apply_change(
        "all_notifications_prefs",
        json!("{\"global\":{},\"channels\":{}}"),
    );
    assert!(config.desktop_enabled("C1"));
    config.apply_change("mute_sounds", json!("false"));
    assert!(!config.user_flag("mute_sounds"));
}

#[test]
fn blocks_win_over_fallback_text_and_system_events_stay_silent() {
    let ws = workspace();
    let mut m = message("C1", "<@U_SELF>");
    m.blocks = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"hello"}]}]}),
    ];
    assert!(!delivers(&ws, &m));
    m.blocks = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"user","user_id":"U_SELF"}]}]}),
    ];
    assert!(delivers(&ws, &m));
    m.subtype = Some("channel_join".into());
    assert!(!delivers(&ws, &m));
    m.subtype = None;
    m.extra.insert("no_notifications".into(), json!(true));
    assert!(!delivers(&ws, &m));
}

#[test]
fn keywords_span_styled_text_and_unfurls_do_not_create_mentions() {
    let ws = workspace();
    let mut m = message("C1", "fallback");
    m.blocks = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Counter "},{"type":"text","text":"Strike","style":{"bold":true}}]}]}),
    ];
    assert!(delivers(&ws, &m));
    m.blocks.clear();
    m.attachments = vec![crate::slack::models::Attachment {
        from_url: Some("https://example.test".into()),
        text: Some("echo".into()),
        ..Default::default()
    }];
    assert!(!delivers(&ws, &m));
}
