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
fn desktop_delivery_distinguishes_absent_null_and_explicit_channel_values() {
    let mut ws = workspace();
    let mention = message("C1", "<@U_SELF> hello");
    ws.notifications
        .global
        .remove("global_desktop_push_enabled");
    assert!(delivers(&ws, &mention));
    for value in [json!(null), json!(false), json!("true"), json!(1)] {
        ws.notifications
            .global
            .insert("global_desktop_push_enabled".into(), value);
        assert!(!delivers(&ws, &mention));
    }
    ws.notifications
        .global
        .insert("global_desktop_push_enabled".into(), json!(true));
    for (value, expected) in [
        (json!(null), false),
        (json!(false), false),
        (json!(0), false),
        (json!(""), false),
        (json!(true), true),
        (json!("false"), true),
        (json!(1), true),
        (json!([]), true),
    ] {
        ws.notifications
            .channels
            .entry("C1".into())
            .or_default()
            .insert("desktop_push_enabled".into(), value.clone());
        assert_eq!(delivers(&ws, &mention), expected, "channel value {value}");
    }
    ws.notifications.channels.get_mut("C1").unwrap().clear();
    assert!(delivers(&ws, &mention));
    ws.notifications.loaded = false;
    assert!(!delivers(&ws, &mention));
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

#[test]
fn temporary_schedule_boot_and_partial_updates_preserve_cache_capability_boundary() {
    let mut config = NotificationConfig::default();
    config.apply_change("dnd_end_hour", json!("08:00"));
    assert!(!config.temporary_schedule_loaded);
    let prefs = serde_json::from_value(json!({"dnd_days":"custom","dnd_end_hour":"09:00","dnd_enabled_monday":"partial","dnd_before_monday":"10:00"})).unwrap();
    config.apply_prefs(&prefs);
    assert!(config.temporary_schedule_loaded);
    assert_eq!(config.user["dnd_before_monday"], json!("10:00"));
    config.apply_change("dnd_before_monday", json!("11:00"));
    assert_eq!(config.user["dnd_before_monday"], json!("11:00"));
    let restored: NotificationConfig =
        serde_json::from_value(serde_json::to_value(&config).unwrap()).unwrap();
    assert!(restored.temporary_schedule_loaded);
    config.apply_prefs(&Default::default());
    assert!(!config.user.contains_key("dnd_before_monday"));
    let legacy: NotificationConfig = serde_json::from_value(json!({"loaded":true})).unwrap();
    assert!(!legacy.temporary_schedule_loaded);
}

#[test]
fn complete_schedule_hydration_preserves_raw_null_and_cache_migration() {
    let mut config = NotificationConfig::default();
    let prefs:BootPrefs=serde_json::from_value(serde_json::json!({"dnd_enabled":true,"dnd_days":"custom","dnd_start_hour":"20:00","dnd_end_hour":"8:00","dnd_after_monday":"22:00","dnd_before_monday":null,"tz":null,"set_tz_automatically":true,"time24":true})).unwrap();
    config.apply_prefs(&prefs);
    assert!(config.schedule_loaded && config.temporary_schedule_loaded);
    assert_eq!(config.user["dnd_after_monday"], "22:00");
    assert_eq!(config.user["dnd_end_hour"], "8:00");
    assert_eq!(config.user["dnd_before_monday"], Value::Null);
    let encoded = serde_json::to_value(&config).unwrap();
    let restored: NotificationConfig = serde_json::from_value(encoded).unwrap();
    assert!(restored.schedule_loaded);
    assert_eq!(restored.user, config.user);
    let old: NotificationConfig = serde_json::from_value(
        serde_json::json!({"temporary_schedule_loaded":true,"user":{"dnd_days":"every_day"}}),
    )
    .unwrap();
    assert!(!old.schedule_loaded);
    config.apply_change("dnd_after_monday", serde_json::json!("19:30"));
    assert_eq!(config.user["dnd_after_monday"], "19:30");
    config.apply_prefs(&BootPrefs::default());
    assert!(!config.user.contains_key("dnd_after_monday"));
    assert!(!config.user.contains_key("tz"));
}

#[test]
fn absent_message_sound_is_silent_even_for_vip_but_none_can_use_vip_override() {
    let mut ws = workspace();
    ws.vip_users.insert("U2".into());
    let mut m = message("D1", "hello");
    m.user = Some("U2".into());
    for value in [None, Some(Value::Null), Some(json!(""))] {
        ws.notifications.global.remove("desktop_sound");
        if let Some(value) = value {
            ws.notifications
                .global
                .insert("desktop_sound".into(), value);
        }
        assert_eq!(
            ws.notifications
                .message_options(&ws, &m, 100)
                .unwrap()
                .sound,
            None
        );
    }
    ws.notifications
        .global
        .insert("desktop_sound".into(), json!("none"));
    assert_eq!(
        ws.notifications
            .message_options(&ws, &m, 100)
            .unwrap()
            .sound
            .as_deref(),
        Some("knock_brush.mp3")
    );
    ws.notifications
        .global
        .insert("desktop_sound".into(), json!("b2.mp3"));
    ws.notifications
        .global
        .insert("priority_desktop_sound".into(), json!(""));
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
fn truthy_raw_privacy_and_mute_values_never_expose_or_sound_message() {
    for value in [json!("false"), json!(1), json!([]), json!({})] {
        let mut ws = workspace();
        ws.notifications
            .global
            .insert("no_text_in_notifications".into(), value.clone());
        ws.notifications.user.insert("mute_sounds".into(), value);
        let o = ws
            .notifications
            .message_options(&ws, &message("D1", "private"), 100)
            .unwrap();
        assert!(!o.preview);
        assert_eq!(o.sound, None);
    }
    for value in [Value::Null, json!(false), json!(0), json!("")] {
        let mut ws = workspace();
        ws.notifications
            .global
            .insert("no_text_in_notifications".into(), value.clone());
        ws.notifications.user.insert("mute_sounds".into(), value);
        let o = ws
            .notifications
            .message_options(&ws, &message("D1", "private"), 100)
            .unwrap();
        assert!(o.preview);
        assert!(o.sound.is_some());
    }
}

#[test]
fn team_plan_comes_only_from_current_boot_and_missing_boot_retires_old_entitlement() {
    let mut ws = workspace();
    assert_eq!(ws.team_plan, None);
    for plan in [json!("plus"), json!(""), Value::Null, json!(true)] {
        let boot: crate::slack::models::BootData =
            serde_json::from_value(json!({"team":{"id":"T1","plan":plan}})).unwrap();
        ws.apply_boot(boot);
        assert_eq!(ws.team_plan.as_deref(), plan.as_str());
    }
    ws.team_plan = Some("plus".into());
    ws.apply_boot(Default::default());
    assert_eq!(ws.team_plan, None);
}

#[test]
fn bounce_projection_preserves_stock_truthiness_and_existing_delivery_policy() {
    let mut ws = workspace();
    for (raw, expected) in [
        (Value::Null, None),
        (json!(""), None),
        (json!(false), None),
        (json!(0), None),
        (json!("short"), Some(DockAttention::Once)),
        (json!("future"), Some(DockAttention::Once)),
        (json!(true), Some(DockAttention::Once)),
        (json!([]), Some(DockAttention::Once)),
        (json!("long"), Some(DockAttention::UntilActive)),
    ] {
        ws.notifications.user.insert("mac_ssb_bounce".into(), raw);
        assert_eq!(ws.notifications.dock_attention(), expected);
    }
    ws.notifications.apply_change("mute_sounds", json!(true));
    let o = ws
        .notifications
        .message_options(&ws, &message("D1", "hello"), 100)
        .unwrap();
    assert_eq!(o.sound, None);
    assert_eq!(
        ws.notifications.dock_attention(),
        Some(DockAttention::UntilActive)
    );
    ws.notifications
        .global
        .insert("global_desktop_push_enabled".into(), json!(false));
    assert!(
        ws.notifications
            .message_options(&ws, &message("D1", "hello"), 100)
            .is_none()
    );
}
#[test]
fn bounce_hydrates_from_boot_realtime_and_old_cache() {
    let mut ws = workspace();
    let prefs: crate::slack::models::BootPrefs =
        serde_json::from_value(json!({"mac_ssb_bounce":"long"})).unwrap();
    ws.notifications.apply_prefs(&prefs);
    assert_eq!(
        ws.notifications.dock_attention(),
        Some(DockAttention::UntilActive)
    );
    let mut cached: NotificationConfig =
        serde_json::from_value(serde_json::to_value(&ws.notifications).unwrap()).unwrap();
    assert_eq!(cached.dock_attention(), Some(DockAttention::UntilActive));
    cached.apply_change("mute_sounds", json!(true));
    assert_eq!(cached.dock_attention(), Some(DockAttention::UntilActive));
    cached.apply_prefs(&crate::slack::models::BootPrefs::default());
    assert!(!cached.user.contains_key("mac_ssb_bounce"));
    assert_eq!(cached.dock_attention(), None);
    assert_eq!(cached.user.get("mute_sounds"), Some(&json!(true)));
    let null: crate::slack::models::BootPrefs =
        serde_json::from_value(json!({"mac_ssb_bounce":null})).unwrap();
    cached.apply_prefs(&null);
    assert_eq!(cached.user.get("mac_ssb_bounce"), Some(&Value::Null));
    assert_eq!(cached.dock_attention(), None);
    ws.notifications.apply_change("mac_ssb_bounce", json!(""));
    assert_eq!(ws.notifications.dock_attention(), None);
    let old: NotificationConfig = serde_json::from_value(json!({})).unwrap();
    assert_eq!(old.dock_attention(), None);
}
