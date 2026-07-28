use super::*;

#[test]
fn account_menu_toggles_and_closes_for_settings() {
    let mut app = test_app();

    let _ = update(
        &mut app,
        Message::Runtime(crate::app::RuntimeMessage::AccountMenuToggled),
    );
    assert!(app.show_account_menu);
    assert!(app.account_menu_open);

    let _ = update(
        &mut app,
        Message::Runtime(crate::app::RuntimeMessage::SettingsOpened),
    );
    assert!(app.show_account_menu);
    assert!(!app.account_menu_open);
    assert!(app.show_settings);
    assert!(app.settings_open);

    let _ = update(
        &mut app,
        Message::Runtime(crate::app::RuntimeMessage::AccountMenuDismissed),
    );
    assert!(!app.show_account_menu);
}

#[test]
fn appearance_preset_and_semantic_colors_update_immediately() {
    let mut app = test_app();

    let _ = update(
        &mut app,
        Message::Runtime(crate::app::RuntimeMessage::SettingsPresetSelected(
            config::ThemePreset::PaperBag,
        )),
    );
    let _ = update(
        &mut app,
        Message::Runtime(crate::app::RuntimeMessage::SettingsRoleColorChanged(
            config::ColorRole::Danger,
            "#A12233".to_owned(),
        )),
    );

    assert_eq!(app.settings.preset, config::ThemePreset::PaperBag);
    assert_eq!(
        app.settings.colors.danger.expect("custom danger").as_hex(),
        "#A12233"
    );
    assert!(
        !app.settings_color_errors
            .contains_key(&config::ColorRole::Danger)
    );

    let _ = update(
        &mut app,
        Message::Runtime(crate::app::RuntimeMessage::SettingsRoleColorChanged(
            config::ColorRole::Danger,
            "#bad".to_owned(),
        )),
    );
    assert_eq!(
        app.settings
            .colors
            .danger
            .expect("previous valid danger")
            .as_hex(),
        "#A12233"
    );
    assert!(
        app.settings_color_errors
            .contains_key(&config::ColorRole::Danger)
    );
}

#[test]
fn background_import_validates_and_copies_supported_images() {
    let source =
        std::env::temp_dir().join(format!("snack-background-{}.png", uuid::Uuid::new_v4()));
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("png header");
        writer
            .write_image_data(&[
                0xE8, 0x87, 0x5B, 0x24, 0x21, 0x1F, 0xE8, 0x87, 0x5B, 0x24, 0x21, 0x1F,
            ])
            .expect("png pixels");
    }
    std::fs::write(&source, bytes).expect("write source");

    let background = import_background_sync(&source).expect("import background");
    let managed = config::background_path(&background).expect("managed path");
    assert!(managed.exists());
    assert_eq!(background.fit, config::BackgroundFit::Cover);

    std::fs::remove_file(source).expect("remove source");
    std::fs::remove_file(managed).expect("remove managed background");
}

#[test]
fn background_import_rejects_oversized_files_before_decode() {
    let source =
        std::env::temp_dir().join(format!("snack-background-{}.png", uuid::Uuid::new_v4()));
    let file = std::fs::File::create(&source).expect("create oversized image");
    file.set_len(25 * 1024 * 1024 + 1).expect("set length");

    let error = import_background_sync(&source).expect_err("reject oversized background");

    assert!(error.contains("25 MB"));
    std::fs::remove_file(source).expect("remove oversized image");
}

#[test]
fn stale_account_scoped_messages_are_ignored_after_reset() {
    let mut app = test_app();
    let old_epoch = app.account_epoch;
    app.reset_account_runtime();

    let _ = update(
        &mut app,
        Message::AccountScoped(
            old_epoch,
            Box::new(Message::Runtime(
                crate::app::RuntimeMessage::AccountMenuToggled,
            )),
        ),
    );

    assert!(!app.show_account_menu);
    assert!(!app.account_menu_open);
}

#[test]
fn account_scoping_is_idempotent_for_nested_reducer_tasks() {
    let scoped = Message::AccountScoped(
        7,
        Box::new(Message::Runtime(
            crate::app::RuntimeMessage::AccountMenuToggled,
        )),
    );

    let result = scope_message(7, scoped);

    let Message::AccountScoped(7, inner) = result else {
        panic!("expected one account scope");
    };
    assert!(matches!(
        *inner,
        Message::Runtime(crate::app::RuntimeMessage::AccountMenuToggled)
    ));
}

#[test]
fn activating_account_replaces_account_scoped_runtime() {
    let mut app = test_app();
    let account_id = "33333333-3333-4333-8333-333333333333".to_owned();
    app.accounts.insert(
        account_id.clone(),
        account_session("T_OTHER", "U_OTHER", "Other"),
    );
    app.search_input = "old account search".into();
    app.composer = iced::widget::text_editor::Content::with_text("old account draft");
    app.avatar_profile_hydrated.insert("U_ALICE".into());
    let old_epoch = app.account_epoch;

    let _ = app.activate_account(account_id.clone());

    assert_eq!(app.active_account.as_deref(), Some(account_id.as_str()));
    assert_eq!(app.active_team.as_deref(), Some("T_OTHER"));
    assert!(app.workspaces.contains_key("T_OTHER"));
    assert!(!app.workspaces.contains_key("T_TEST"));
    assert!(app.search_input.is_empty());
    assert!(app.composer.text().is_empty());
    assert!(app.avatar_profile_hydrated.is_empty());
    assert_ne!(app.account_epoch, old_epoch);
}

#[test]
fn self_presence_selection_updates_active_workspace() {
    let mut app = test_app();
    app.show_account_menu = true;
    app.account_menu_open = true;

    let _ = update(
        &mut app,
        Message::Runtime(crate::app::RuntimeMessage::SelfPresenceSelected(
            Presence::Active,
        )),
    );

    let ws = app.active_workspace().unwrap();
    assert_eq!(ws.presence.get(SELF_USER), Some(&Presence::Active));
    assert!(!app.account_menu_open);

    let _ = update(
        &mut app,
        Message::Runtime(crate::app::RuntimeMessage::AccountMenuDismissed),
    );
    assert!(!app.show_account_menu);
}

#[test]
fn notification_created_for_inactive_channel_mention() {
    let app = test_app();
    let ws = app.active_workspace().unwrap();
    let message = SlackMessage {
        user: Some("U_ALICE".into()),
        text: Some("hi <@U_SELF>".into()),
        ..Default::default()
    };

    let notification = notification_for_message(ws, "C_DEV", &message, Some("C_GENERAL"))
        .expect("mention should notify");

    assert_eq!(notification.title, "Alice in #dev");
    assert_eq!(notification.body, "hi <@U_SELF>");
}

#[test]
fn notification_suppresses_self_and_active_channel_messages() {
    let app = test_app();
    let ws = app.active_workspace().unwrap();
    let self_message = SlackMessage {
        user: Some(SELF_USER.into()),
        text: Some("self <@U_SELF>".into()),
        ..Default::default()
    };
    assert!(notification_for_message(ws, "C_DEV", &self_message, Some("C_GENERAL")).is_none());

    let active_message = SlackMessage {
        user: Some("U_ALICE".into()),
        text: Some("active <@U_SELF>".into()),
        ..Default::default()
    };
    assert!(
        notification_for_message(ws, "C_GENERAL", &active_message, Some("C_GENERAL")).is_none()
    );
}

#[test]
fn notification_detects_block_user_mentions() {
    let app = test_app();
    let ws = app.active_workspace().unwrap();
    let message = SlackMessage {
        user: Some("U_ALICE".into()),
        blocks: vec![json!({
            "type": "rich_text",
            "elements": [{
                "type": "rich_text_section",
                "elements": [
                    {"type": "text", "text": "cc "},
                    {"type": "user", "user_id": "U_SELF"}
                ]
            }]
        })],
        ..Default::default()
    };

    let notification = notification_for_message(ws, "C_DEV", &message, Some("C_GENERAL"))
        .expect("block mention should notify");

    assert_eq!(notification.body, "cc @You");
}

#[test]
fn boot_selects_first_channel() {
    let app = test_app();
    assert_eq!(app.screen, Screen::Main);
    assert!(app.active_team.is_some());
    assert!(app.active_channel.is_some());
    assert!(!app.active_workspace().unwrap().channels.is_empty());
}

#[test]
fn channel_selection_preserves_loaded_messages() {
    let mut app = test_app();
    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::ChannelSelected(
            "C_DEV".into(),
        )),
    );
    assert_eq!(app.active_channel.as_deref(), Some("C_DEV"));
    let ws = app.active_workspace().unwrap();
    assert!(!ws.messages.get("C_GENERAL").unwrap().messages.is_empty());
    assert!(!ws.messages.get("C_DEV").unwrap().messages.is_empty());
}

#[test]
fn selecting_cached_channel_marks_cached_tail_while_refreshing() {
    let mut app = live_test_app();
    let team = app.active_team.clone().unwrap();

    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::ChannelSelected(
            "C_DEV".into(),
        )),
    );

    let cm = &app.workspaces[&team].messages["C_DEV"];
    assert!(cm.loaded);
    assert!(cm.history_refreshing);
    assert!(!cm.messages.is_empty());
    assert!(app.pending_marks.contains(&(
        ReadTarget::Conversation {
            team,
            channel: "C_DEV".into(),
        },
        "1783370010.000200".into(),
    )));
}

#[test]
fn cached_history_refresh_merges_new_messages_and_clears_refresh_state() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let channel = app.active_channel.clone().unwrap();
    app.workspaces
        .get_mut(&team)
        .unwrap()
        .messages
        .get_mut(&channel)
        .unwrap()
        .history_refreshing = true;

    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::HistoryLoaded(
            team.clone(),
            channel.clone(),
            HistoryLoadKind::Since,
            Ok(loaded_history(HistoryPage {
                messages: vec![msg("U_ALICE", "1783372400.000100", "fresh from Slack")],
                ..Default::default()
            })),
        )),
    );

    let cm = &app.workspaces[&team].messages[&channel];
    assert!(!cm.history_refreshing);
    assert!(cm.messages.iter().any(|message| {
        message.ts.as_deref() == Some("1783372400.000100")
            && message.text.as_deref() == Some("fresh from Slack")
    }));
}

#[test]
fn cached_history_refresh_failure_keeps_messages_and_allows_retry() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let channel = app.active_channel.clone().unwrap();
    let before = app.workspaces[&team].messages[&channel].messages.len();
    app.workspaces
        .get_mut(&team)
        .unwrap()
        .messages
        .get_mut(&channel)
        .unwrap()
        .history_refreshing = true;

    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::HistoryLoaded(
            team.clone(),
            channel.clone(),
            HistoryLoadKind::Since,
            Err(SlackError::Transport("offline".into())),
        )),
    );

    let cm = &app.workspaces[&team].messages[&channel];
    assert!(!cm.history_refreshing);
    assert!(cm.history_failed);
    assert_eq!(cm.messages.len(), before);
}

#[test]
fn late_history_refresh_for_inactive_channel_does_not_mark_it_read() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    app.active_channel = Some("C_DEV".into());
    app.workspaces
        .get_mut(&team)
        .unwrap()
        .messages
        .get_mut("C_GENERAL")
        .unwrap()
        .history_refreshing = true;

    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::HistoryLoaded(
            team.clone(),
            "C_GENERAL".into(),
            HistoryLoadKind::Since,
            Ok(loaded_history(HistoryPage {
                messages: vec![msg("U_ALICE", "1783372500.000100", "arrived after switch")],
                ..Default::default()
            })),
        )),
    );

    assert!(app.pending_marks.is_empty());
    assert!(
        app.workspaces[&team].messages["C_GENERAL"]
            .messages
            .iter()
            .any(|message| message.ts.as_deref() == Some("1783372500.000100"))
    );
}

#[test]
fn bounded_catch_up_replaces_stale_tail_but_preserves_pending_send() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let channel = app.active_channel.clone().unwrap();
    let pending_ts = "9999999999.000001".to_owned();
    {
        let cm = app
            .workspaces
            .get_mut(&team)
            .unwrap()
            .messages
            .get_mut(&channel)
            .unwrap();
        cm.upsert(msg(SELF_USER, &pending_ts, "pending send"));
        cm.pending.push(pending_ts.clone());
        cm.history_refreshing = true;
    }

    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::HistoryLoaded(
            team.clone(),
            channel.clone(),
            HistoryLoadKind::Since,
            Ok(LoadedHistory {
                page: HistoryPage {
                    messages: vec![msg("U_ALICE", "1783372600.000100", "new contiguous tail")],
                    has_more: true,
                    ..Default::default()
                },
                replace_cached: true,
            }),
        )),
    );

    let cm = &app.workspaces[&team].messages[&channel];
    assert_eq!(cm.messages.len(), 2);
    assert!(cm.is_pending(&pending_ts));
    assert!(
        cm.messages
            .iter()
            .any(|message| { message.ts.as_deref() == Some("1783372600.000100") })
    );
    assert!(cm.has_more_older);
}

#[test]
fn channel_selection_records_last_active_channel() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();

    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::ChannelSelected(
            "C_DEV".into(),
        )),
    );

    let ws = &app.workspaces[&team];
    assert_eq!(ws.last_active_channel.as_deref(), Some("C_DEV"));
    assert_eq!(preferred_channel(&app, &team).as_deref(), Some("C_DEV"));
}

#[test]
fn unread_channel_open_targets_first_unread_message() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let ws = app.workspaces.get_mut(&team).unwrap();
    let cm = ws.messages.get_mut("C_DEV").unwrap();
    cm.last_read = Some("1783370000.000100".into());
    cm.unread_count = 2;
    cm.upsert(msg("U_ALICE", "1783370001.000100", "first unread"));
    cm.upsert(msg("U_ALICE", "1783370002.000100", "second unread"));

    assert_eq!(
        channel_open_scroll_target(&app, &team, &"C_DEV".into()),
        Some(PendingScrollTarget::FirstUnreadAfter(
            "1783370000.000100".into()
        ))
    );
    assert_eq!(
        pending_target_ts(
            &app.workspaces[&team].messages["C_DEV"].messages,
            PendingScrollTarget::FirstUnreadAfter("1783370000.000100".into())
        ),
        Some("1783370001.000100".into())
    );
}

#[test]
fn unread_anchor_ignores_never_read_sentinel() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let ws = app.workspaces.get_mut(&team).unwrap();
    let cm = ws.messages.entry("D_NEW".into()).or_default();
    cm.last_read = Some("0000000000.000000".into());
    cm.unread_count = 1;

    assert_eq!(app.unread_anchor(&team, &"D_NEW".into()), None);

    let ws = app.workspaces.get_mut(&team).unwrap();
    let cm = ws.messages.get_mut("D_NEW").unwrap();
    cm.last_read = Some("1783370000.000100".into());
    assert_eq!(
        app.unread_anchor(&team, &"D_NEW".into()),
        Some("1783370000.000100".into())
    );
}

#[test]
fn dm_channels_hydrate_on_missing_user_not_missing_name() {
    let im_with_user = Channel {
        id: "D1".into(),
        is_im: true,
        user: Some("U1".into()),
        ..Default::default()
    };
    let im_without_user = Channel {
        id: "D2".into(),
        is_im: true,
        ..Default::default()
    };
    let unnamed_channel = Channel {
        id: "C1".into(),
        is_channel: true,
        ..Default::default()
    };
    assert!(!channel_needs_hydration(&im_with_user));
    assert!(channel_needs_hydration(&im_without_user));
    assert!(channel_needs_hydration(&unnamed_channel));
}

#[test]
fn read_channel_open_targets_latest_message() {
    let app = test_app();
    let team = app.active_team.clone().unwrap();

    assert_eq!(
        channel_open_scroll_target(&app, &team, &"C_DEV".into()),
        Some(PendingScrollTarget::Latest)
    );
}

#[test]
fn merge_history_pages_dedupes_anchor_message() {
    let merged = merge_history_pages(
        HistoryPage {
            messages: vec![
                msg("U_ALICE", "1783370001.000100", "newer"),
                msg("U_ALICE", "1783370000.000100", "anchor"),
            ],
            ..Default::default()
        },
        HistoryPage {
            messages: vec![
                msg("U_ALICE", "1783370002.000100", "newest"),
                msg("U_ALICE", "1783370001.000100", "newer duplicate"),
            ],
            ..Default::default()
        },
    );

    let ts: Vec<_> = merged
        .messages
        .iter()
        .filter_map(|message| message.ts.as_deref())
        .collect();
    assert_eq!(
        ts,
        vec![
            "1783370001.000100",
            "1783370000.000100",
            "1783370002.000100"
        ]
    );
}

#[test]
fn top_scroll_loads_older_only_when_available() {
    let mut cm = loaded_channel("U_ALICE", "1783370001.000100", "newer");
    cm.has_more_older = true;

    assert!(should_load_older_history(&cm, 0.0));
    assert!(should_load_older_history(&cm, 48.0));
    assert!(!should_load_older_history(&cm, 49.0));

    cm.history_loading_older = true;
    assert!(!should_load_older_history(&cm, 0.0));

    cm.history_loading_older = false;
    cm.has_more_older = false;
    assert!(!should_load_older_history(&cm, 0.0));
}

#[test]
fn activity_bottom_scroll_loads_older_only_when_available() {
    let mut activity = ActivityState {
        loaded: true,
        next_cursor: Some("older-activity".into()),
        ..Default::default()
    };

    assert!(should_load_older_activity(&activity, 0.0));
    assert!(should_load_older_activity(&activity, 96.0));
    assert!(!should_load_older_activity(&activity, 97.0));

    activity.loading = true;
    assert!(!should_load_older_activity(&activity, 0.0));

    activity.loading = false;
    activity.next_cursor = None;
    assert!(!should_load_older_activity(&activity, 0.0));
}

#[test]
fn unread_activity_auto_loads_until_a_matching_item_is_found() {
    let mut app = activity_app();
    app.activity.unread_only = true;
    app.activity.items.retain(|item| !item.is_unread);
    app.activity.next_cursor = Some("older-activity".into());

    assert!(should_auto_load_activity(&app.activity));

    app.activity.items[0].is_unread = true;
    assert!(!should_auto_load_activity(&app.activity));

    app.activity.items.clear();
    app.activity.next_cursor = None;
    assert!(!should_auto_load_activity(&app.activity));
}

#[test]
fn gif_emoji_preview_decodes_as_animation() {
    let mut bytes = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut bytes, 1, 1, &[]).unwrap();
        encoder.set_repeat(gif::Repeat::Infinite).unwrap();
        let mut first = vec![255, 0, 0, 255];
        let mut frame = gif::Frame::from_rgba(1, 1, &mut first);
        frame.delay = 2;
        encoder.write_frame(&frame).unwrap();
        let mut second = vec![0, 255, 0, 255];
        let mut frame = gif::Frame::from_rgba(1, 1, &mut second);
        frame.delay = 3;
        encoder.write_frame(&frame).unwrap();
    }

    match emoji_preview_from_bytes(bytes) {
        FilePreview::Animated {
            frames,
            allocations,
            delays,
            total,
        } => {
            assert_eq!(frames.len(), 2);
            assert!(allocations.is_empty());
            assert!(
                frames
                    .iter()
                    .all(|frame| matches!(frame, ImageHandle::Bytes(_, _)))
            );
            assert_eq!(
                delays,
                vec![
                    std::time::Duration::from_millis(20),
                    std::time::Duration::from_millis(30)
                ]
            );
            assert_eq!(total, std::time::Duration::from_millis(50));
        }
        other => panic!("expected animated preview, got {other:?}"),
    }
}

#[test]
fn file_animation_tick_only_tracks_visible_messages() {
    let mut app = gif_picker_app();
    assert_eq!(
        visible_media_animation_interval(&app),
        Some(std::time::Duration::from_millis(40))
    );

    app.active_channel = Some("C_DEV".into());
    assert_eq!(visible_media_animation_interval(&app), None);
}

#[test]
fn reaction_animation_tick_uses_visible_emoji_frame_delay() {
    let mut app = test_app();
    let message = &mut app
        .active_workspace_mut()
        .unwrap()
        .messages
        .get_mut("C_GENERAL")
        .unwrap()
        .messages[0];
    message.reactions.push(crate::slack::models::Reaction {
        name: "party".into(),
        users: vec!["U_ALICE".into()],
        count: 1,
        ..Default::default()
    });
    app.emoji_previews.insert(
        crate::state::emoji_preview_key("T_TEST", "party"),
        FilePreview::Animated {
            frames: vec![
                ImageHandle::from_rgba(1, 1, vec![255, 0, 0, 255]),
                ImageHandle::from_rgba(1, 1, vec![0, 255, 0, 255]),
            ],
            allocations: Vec::new(),
            delays: vec![
                std::time::Duration::from_millis(20),
                std::time::Duration::from_millis(50),
            ],
            total: std::time::Duration::from_millis(70),
        },
    );

    assert_eq!(
        visible_media_animation_interval(&app),
        Some(std::time::Duration::from_millis(20))
    );

    app.active_channel = Some("C_DEV".into());
    assert_eq!(visible_media_animation_interval(&app), None);
}

#[test]
fn known_user_without_avatar_still_gets_profile_hydration() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let ws = app.workspaces.get_mut(&team).unwrap();
    ws.users.insert(
        "U_ALICE".into(),
        User {
            id: "U_ALICE".into(),
            profile: Some(UserProfile {
                display_name: Some("Alice".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let ws = app.workspaces.get(&team).unwrap();
    assert!(needs_user_hydration(
        ws,
        &app.avatar_profile_hydrated,
        "U_ALICE"
    ));

    app.avatar_profile_hydrated.insert("U_ALICE".into());
    let ws = app.workspaces.get(&team).unwrap();
    assert!(!needs_user_hydration(
        ws,
        &app.avatar_profile_hydrated,
        "U_ALICE"
    ));
}

#[test]
fn known_user_with_avatar_skips_profile_hydration() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let ws = app.workspaces.get_mut(&team).unwrap();
    ws.users.insert(
        "U_ALICE".into(),
        User {
            id: "U_ALICE".into(),
            profile: Some(UserProfile {
                image_48: Some("https://example.test/alice.png".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let ws = app.workspaces.get(&team).unwrap();
    assert!(!needs_user_hydration(
        ws,
        &app.avatar_profile_hydrated,
        "U_ALICE"
    ));
}

#[test]
fn workspace_selection_switches_active_workspace_and_channel() {
    let mut app = test_app();
    add_second_workspace(&mut app);
    app.activity.loaded = true;
    app.activity.next_cursor = Some("first-workspace-cursor".into());
    app.composer = iced::widget::text_editor::Content::with_text("draft");
    app.thread_composer = iced::widget::text_editor::Content::with_text("reply draft");
    app.active_thread = Some(("C_GENERAL".into(), "1783372300.000100".into()));

    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::WorkspaceSelected(
            "T_SECOND".into(),
        )),
    );

    assert_eq!(app.active_team.as_deref(), Some("T_SECOND"));
    assert_eq!(app.active_channel.as_deref(), Some("C_SECOND"));
    assert!(app.active_thread.is_none());
    assert!(app.composer.text().is_empty());
    assert!(app.thread_composer.text().is_empty());
    assert!(!app.activity.loaded);
    assert!(app.activity.next_cursor.is_none());
}

#[test]
fn workspace_selection_remembers_previous_channel() {
    let mut app = test_app();
    add_second_workspace(&mut app);
    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::ChannelSelected(
            "C_DEV".into(),
        )),
    );
    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::WorkspaceSelected(
            "T_SECOND".into(),
        )),
    );
    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::WorkspaceSelected(
            "T_TEST".into(),
        )),
    );

    assert_eq!(app.active_team.as_deref(), Some("T_TEST"));
    assert_eq!(app.active_channel.as_deref(), Some("C_DEV"));
}

#[tokio::test]
async fn unique_download_path_avoids_overwrite() {
    let dir = std::env::temp_dir().join(format!("snack-test-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&dir).await.unwrap();
    tokio::fs::write(dir.join("report.pdf"), b"old")
        .await
        .unwrap();

    let path = unique_download_path(&dir, "report.pdf").await.unwrap();

    assert_eq!(
        path.file_name().and_then(|name| name.to_str()),
        Some("report-1.pdf")
    );
}
