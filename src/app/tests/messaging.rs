use super::*;

#[test]
fn thread_open_tracks_selected_root() {
    let mut app = test_app();
    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::ThreadOpened {
            channel: "C_GENERAL".into(),
            ts: "1783372300.000100".into(),
            unread_range: None,
        }),
    );

    assert_eq!(app.active_channel.as_deref(), Some("C_GENERAL"));
    assert_eq!(
        app.active_thread.as_ref(),
        Some(&("C_GENERAL".into(), "1783372300.000100".into()))
    );
}

#[test]
fn thread_loaded_stores_replies() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let root_ts = "1783372300.000100".to_owned();
    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::ThreadLoaded {
            team: team.clone(),
            channel: "C_GENERAL".into(),
            root_ts: root_ts.clone(),
            unread_anchor: None,
            result: Ok(HistoryPage {
                messages: vec![
                    msg("U_ALICE", &root_ts, "morning"),
                    SlackMessage {
                        thread_ts: Some(root_ts.clone()),
                        ..msg("U_BOB", "1783372310.000100", "reply")
                    },
                ],
                ..Default::default()
            }),
        }),
    );

    let cm = &app.threads[&(team, "C_GENERAL".into(), root_ts)];
    assert!(cm.loaded);
    assert_eq!(cm.messages.len(), 2);
}

#[test]
fn thread_loaded_with_unread_anchor_sets_marker_and_reopen_clears_it() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let root_ts = "1783372300.000100".to_owned();
    let anchor = "1783372310.000100".to_owned();
    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::ThreadLoaded {
            team: team.clone(),
            channel: "C_GENERAL".into(),
            root_ts: root_ts.clone(),
            unread_anchor: Some(anchor.clone()),
            result: Ok(HistoryPage {
                messages: vec![
                    msg("U_ALICE", &root_ts, "morning"),
                    SlackMessage {
                        thread_ts: Some(root_ts.clone()),
                        ..msg("U_BOB", &anchor, "reply")
                    },
                ],
                ..Default::default()
            }),
        }),
    );

    assert_eq!(
        app.thread_unread_marker,
        Some(((team, "C_GENERAL".into(), root_ts.clone()), anchor.clone()))
    );

    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::ThreadOpened {
            channel: "C_GENERAL".into(),
            ts: root_ts,
            unread_range: None,
        }),
    );
    assert_eq!(app.thread_unread_marker, None);
}

#[test]
fn unread_thread_window_replaces_stale_full_thread() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let channel = "C_GENERAL".to_owned();
    let root_ts = "1783372300.000100".to_owned();
    let unread_ts = "1783372400.000100".to_owned();
    let key = (team.clone(), channel.clone(), root_ts.clone());
    let mut stale = ChannelMessages::default();
    stale.upsert(msg("U_ALICE", &root_ts, "root"));
    stale.upsert(msg("U_BOB", "1783372310.000100", "old read reply"));
    app.threads.insert(key.clone(), stale);

    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::ThreadLoaded {
            team,
            channel,
            root_ts: root_ts.clone(),
            unread_anchor: Some(unread_ts.clone()),
            result: Ok(HistoryPage {
                messages: vec![
                    msg("U_ALICE", &root_ts, "root"),
                    SlackMessage {
                        thread_ts: Some(root_ts),
                        ..msg("U_BOB", &unread_ts, "first unread")
                    },
                ],
                ..Default::default()
            }),
        }),
    );

    let cm = &app.threads[&key];
    assert_eq!(cm.messages.len(), 2);
    assert!(
        !cm.messages
            .iter()
            .any(|message| message.ts.as_deref() == Some("1783372310.000100"))
    );
    assert!(
        cm.messages
            .iter()
            .any(|message| message.ts.as_deref() == Some(&unread_ts))
    );
}

#[test]
fn optimistic_thread_reply_inserts_pending_without_transport() {
    let mut app = test_app();
    let root_ts = "1783372300.000100".to_owned();
    app.active_thread = Some(("C_GENERAL".into(), root_ts.clone()));
    app.thread_composer = iced::widget::text_editor::Content::with_text("thread answer");
    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::ThreadSendPressed),
    );

    assert!(app.thread_composer.text().is_empty());
    let team = app.active_team.clone().unwrap();
    let cm = &app.threads[&(team, "C_GENERAL".into(), root_ts)];
    let reply = cm.messages.last().unwrap();
    assert_eq!(reply.text.as_deref(), Some("thread answer"));
    assert_eq!(reply.thread_ts.as_deref(), Some("1783372300.000100"));
    assert!(cm.is_pending(reply.ts.as_deref().unwrap()));
}

#[test]
fn realtime_thread_reply_updates_open_thread_not_channel() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let root_ts = "1783372300.000100".to_owned();
    app.active_thread = Some(("C_GENERAL".into(), root_ts.clone()));
    app.threads.insert(
        (team.clone(), "C_GENERAL".into(), root_ts.clone()),
        ChannelMessages::default(),
    );
    let before = app.workspaces[&team].messages["C_GENERAL"].messages.len();
    let ev = RtEvent::Message(SlackMessage {
        user: Some("U_BOB".into()),
        ts: Some("1783372310.000100".into()),
        channel: Some("C_GENERAL".into()),
        thread_ts: Some(root_ts.clone()),
        text: Some("reply".into()),
        ..Default::default()
    });
    let _ = update(
        &mut app,
        Message::Runtime(crate::app::RuntimeMessage::Realtime(team.clone(), 1, ev)),
    );

    assert_eq!(
        app.workspaces[&team].messages["C_GENERAL"].messages.len(),
        before
    );
    assert_eq!(
        app.threads[&(team, "C_GENERAL".into(), root_ts)].messages[0]
            .text
            .as_deref(),
        Some("reply")
    );
}

#[test]
fn empty_send_is_noop() {
    let mut app = test_app();
    app.active_channel = Some("C_GENERAL".into());
    let before = app.active_workspace().unwrap().messages["C_GENERAL"]
        .messages
        .len();
    app.composer = iced::widget::text_editor::Content::with_text("   ");
    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::SendPressed),
    );
    let after = app.active_workspace().unwrap().messages["C_GENERAL"]
        .messages
        .len();
    assert_eq!(before, after);
}

#[test]
fn motion_delete_removes_the_spanned_text() {
    use iced::widget::text_editor::{Action, Motion};

    let mut app = test_app();
    app.composer = iced::widget::text_editor::Content::with_text("hello world");
    app.composer.perform(Action::Move(Motion::DocumentEnd));
    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::ComposerDelete {
            target: ComposerTarget::Channel,
            motion: Motion::WordLeft,
        }),
    );
    assert_eq!(app.composer.text(), "hello ");

    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::ComposerDelete {
            target: ComposerTarget::Channel,
            motion: Motion::Home,
        }),
    );
    assert_eq!(app.composer.text(), "");

    app.edit_content = iced::widget::text_editor::Content::with_text("fix typo");
    app.edit_content.perform(Action::Move(Motion::DocumentEnd));
    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::ComposerDelete {
            target: ComposerTarget::Edit,
            motion: Motion::WordLeft,
        }),
    );
    assert_eq!(app.edit_content.text(), "fix ");

    app.composer = iced::widget::text_editor::Content::with_text("done");
    app.composer.perform(Action::Move(Motion::DocumentEnd));
    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::ComposerDelete {
            target: ComposerTarget::Channel,
            motion: Motion::End,
        }),
    );
    assert_eq!(app.composer.text(), "done");
}

#[test]
fn optimistic_send_inserts_pending_without_transport() {
    let mut app = test_app();
    app.active_channel = Some("C_GENERAL".into());
    app.composer = iced::widget::text_editor::Content::with_text("hello world");
    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::SendPressed),
    );

    assert!(app.composer.text().is_empty());
    let cm = &app.active_workspace().unwrap().messages["C_GENERAL"];
    let last = cm.messages.last().unwrap();
    assert_eq!(last.text.as_deref(), Some("hello world"));
    assert_eq!(last.user.as_deref(), Some(SELF_USER));
    let ts = last.ts.clone().unwrap();
    assert!(cm.is_pending(&ts));
}

#[test]
fn message_sent_clears_pending() {
    let mut app = test_app();
    app.active_channel = Some("C_GENERAL".into());
    app.composer = iced::widget::text_editor::Content::with_text("confirm me");
    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::SendPressed),
    );

    let cid = {
        let cm = &app.active_workspace().unwrap().messages["C_GENERAL"];
        cm.messages
            .iter()
            .find(|m| m.text.as_deref() == Some("confirm me"))
            .unwrap()
            .client_msg_id
            .clone()
            .unwrap()
    };

    let team = app.active_team.clone().unwrap();
    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::MessageSent {
            team,
            channel: "C_GENERAL".into(),
            client_msg_id: cid,
            result: Ok(SentMessage {
                channel: "C_GENERAL".into(),
                ts: "1783372400.111111".into(),
                message: SlackMessage {
                    user: Some(SELF_USER.into()),
                    text: Some("confirm me".into()),
                    ts: Some("1783372400.111111".into()),
                    ..Default::default()
                },
            }),
        }),
    );

    let cm = &app.active_workspace().unwrap().messages["C_GENERAL"];
    let msg = cm
        .messages
        .iter()
        .find(|m| m.text.as_deref() == Some("confirm me"))
        .unwrap();
    assert!(!cm.is_pending(msg.ts.as_deref().unwrap()));
}

#[test]
fn realtime_message_upserts_into_channel() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let before = app.workspaces[&team].messages["C_GENERAL"].messages.len();
    let ev = RtEvent::Message(SlackMessage {
        user: Some("U_ALICE".into()),
        ts: Some("9999999999.000001".into()),
        channel: Some("C_GENERAL".into()),
        text: Some("live!".into()),
        ..Default::default()
    });
    let _ = update(
        &mut app,
        Message::Runtime(crate::app::RuntimeMessage::Realtime(team.clone(), 1, ev)),
    );
    let after = app.workspaces[&team].messages["C_GENERAL"].messages.len();
    assert_eq!(after, before + 1);
}

#[test]
fn scrolling_up_pauses_chat_and_bottom_resumes() {
    let mut app = test_app();
    assert!(!app.chat_paused.contains_key("C_GENERAL"));

    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::ChannelScrolled {
            channel: "C_GENERAL".into(),
            y: 500.0,
            bottom_gap: 300.0,
        }),
    );
    assert!(app.chat_paused.contains_key("C_GENERAL"));

    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::ChannelScrolled {
            channel: "C_GENERAL".into(),
            y: 800.0,
            bottom_gap: 0.0,
        }),
    );
    assert!(!app.chat_paused.contains_key("C_GENERAL"));
}

#[test]
fn near_bottom_scroll_does_not_pause_chat() {
    let mut app = test_app();
    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::ChannelScrolled {
            channel: "C_GENERAL".into(),
            y: 795.0,
            bottom_gap: 5.0,
        }),
    );
    assert!(!app.chat_paused.contains_key("C_GENERAL"));
}

#[test]
fn scrollbar_activity_auto_hides_after_idle_deadline() {
    let mut app = test_app();

    let _ = update(
        &mut app,
        Message::Runtime(crate::app::RuntimeMessage::ScrollActivity),
    );
    assert!(app.scrollbar_visible_until.is_some());

    app.scrollbar_visible_until = Some(Instant::now() - Duration::from_millis(1));
    let _ = update(
        &mut app,
        Message::Runtime(crate::app::RuntimeMessage::AnimationTick),
    );
    assert!(app.scrollbar_visible_until.is_none());
}

#[test]
fn realtime_message_counts_toward_paused_pill() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    app.chat_paused.insert("C_GENERAL".into(), 0);

    for (i, user) in ["U_ALICE", "U_BOB"].iter().enumerate() {
        let ev = RtEvent::Message(SlackMessage {
            user: Some((*user).into()),
            ts: Some(format!("9999999999.00000{i}")),
            channel: Some("C_GENERAL".into()),
            text: Some("live!".into()),
            ..Default::default()
        });
        let _ = update(
            &mut app,
            Message::Runtime(crate::app::RuntimeMessage::Realtime(team.clone(), 1, ev)),
        );
    }
    assert_eq!(app.chat_paused.get("C_GENERAL"), Some(&2));

    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::ChatResumePressed(
            "C_GENERAL".into(),
        )),
    );
    assert!(!app.chat_paused.contains_key("C_GENERAL"));
}

#[test]
fn realtime_file_replaces_pending_upload_and_keeps_local_preview() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let channel = "C_GENERAL".to_owned();
    let pending_ts = "9999999998.000001".to_owned();
    let client_msg_id = "pending-file-1".to_owned();
    let pending = SlackMessage {
        user: Some(SELF_USER.into()),
        ts: Some(pending_ts.clone()),
        client_msg_id: Some(client_msg_id.clone()),
        channel: Some(channel.clone()),
        text: Some("video".into()),
        ..Default::default()
    };
    let messages = app
        .workspaces
        .get_mut(&team)
        .unwrap()
        .messages
        .get_mut(&channel)
        .unwrap();
    messages.upsert(pending);
    messages.pending.push(pending_ts.clone());
    let before = messages.messages.len();
    app.pending_file_messages.push(PendingFileMessage {
        team: team.clone(),
        channel: channel.clone(),
        thread_ts: None,
        message_ts: pending_ts.clone(),
        client_msg_id,
        text: "video".into(),
        attachments: vec![ComposerAttachment {
            id: 1,
            path: PathBuf::from("/tmp/video.mp4"),
            name: "video.mp4".into(),
            bytes: 10,
            uploading: true,
            upload_started: Some(Instant::now()),
            upload_cancel: None,
            upload_progress: None,
            preview_path: Some(PathBuf::from("/tmp/video-preview.jpg")),
        }],
    });

    let event = RtEvent::Message(SlackMessage {
        user: Some(SELF_USER.into()),
        ts: Some("9999999999.000001".into()),
        channel: Some(channel.clone()),
        text: Some("video".into()),
        files: vec![File {
            id: Some("F_VIDEO".into()),
            name: Some("video.mp4".into()),
            ..Default::default()
        }],
        ..Default::default()
    });
    let _ = update(
        &mut app,
        Message::Runtime(crate::app::RuntimeMessage::Realtime(team.clone(), 1, event)),
    );

    let messages = &app.workspaces[&team].messages[&channel];
    assert_eq!(messages.messages.len(), before);
    assert!(
        !messages
            .messages
            .iter()
            .any(|message| { message.ts.as_deref() == Some(pending_ts.as_str()) })
    );
    assert!(app.pending_file_messages.is_empty());
    assert!(matches!(
        app.file_previews.get("F_VIDEO"),
        Some(FilePreview::Loaded(_))
    ));
}

#[test]
fn realtime_delete_removes_message() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let ev = RtEvent::MessageDeleted {
        channel: "C_GENERAL".into(),
        deleted_ts: "1783372300.000100".into(),
    };
    let _ = update(
        &mut app,
        Message::Runtime(crate::app::RuntimeMessage::Realtime(team.clone(), 1, ev)),
    );
    let exists = app.workspaces[&team].messages["C_GENERAL"]
        .messages
        .iter()
        .any(|m| m.ts.as_deref() == Some("1783372300.000100"));
    assert!(!exists);
}

#[test]
fn rt_connected_stores_connection() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let conn = Connection::from_sender(tx);
    let _ = update(
        &mut app,
        Message::Runtime(crate::app::RuntimeMessage::RtConnected(
            team.clone(),
            2,
            conn,
        )),
    );
    assert!(app.workspaces[&team].rt.is_connected());
    assert_eq!(app.workspaces[&team].rt_generation, 2);
    let _ = update(
        &mut app,
        Message::Runtime(crate::app::RuntimeMessage::RtDisconnected(team.clone(), 1)),
    );
    assert!(app.workspaces[&team].rt.is_connected());
    let _ = update(
        &mut app,
        Message::Runtime(crate::app::RuntimeMessage::RtDisconnected(team.clone(), 2)),
    );
    assert!(!app.workspaces[&team].rt.is_connected());
}

#[test]
fn stale_realtime_event_is_ignored() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    app.workspaces.get_mut(&team).unwrap().rt_generation = 5;
    let before = app.workspaces[&team].messages["C_GENERAL"].messages.len();
    let ev = RtEvent::Message(SlackMessage {
        user: Some("U_ALICE".into()),
        ts: Some("9999999999.000002".into()),
        channel: Some("C_GENERAL".into()),
        text: Some("stale".into()),
        ..Default::default()
    });
    let _ = update(
        &mut app,
        Message::Runtime(crate::app::RuntimeMessage::Realtime(team.clone(), 4, ev)),
    );
    let after = app.workspaces[&team].messages["C_GENERAL"].messages.len();
    assert_eq!(after, before);
}

#[test]
fn edit_pressed_populates_editor_with_current_text() {
    let mut app = test_app();
    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::EditPressed {
            channel: "C_GENERAL".into(),
            ts: "1783372300.000100".into(),
        }),
    );
    assert_eq!(
        app.edit_content.text(),
        "morning — shipping the agent UI harness today"
    );
    assert_eq!(
        app.editing.as_ref(),
        Some(&("C_GENERAL".into(), "1783372300.000100".into()))
    );
}

#[test]
fn edit_submit_optimistically_updates_text_and_marks_edited() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    app.editing = Some(("C_GENERAL".into(), "1783372300.000100".into()));
    app.edit_content = Content::with_text("morning (updated)");

    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::EditSubmit),
    );

    assert!(app.editing.is_none());
    assert!(app.edit_content.text().is_empty());
    let cm = &app.workspaces[&team].messages["C_GENERAL"];
    let msg = cm
        .messages
        .iter()
        .find(|m| m.ts.as_deref() == Some("1783372300.000100"))
        .unwrap();
    assert_eq!(msg.text.as_deref(), Some("morning (updated)"));
    assert!(msg.edited.is_some());
}

#[test]
fn empty_edit_submit_keeps_editor_open() {
    let mut app = test_app();
    app.editing = Some(("C_GENERAL".into(), "1783372300.000100".into()));
    app.edit_content = Content::with_text("   ");

    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::EditSubmit),
    );

    assert!(app.editing.is_some());
}

#[test]
fn edit_applies_to_open_thread_copy() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let root_ts = "1783372300.000100".to_owned();
    let mut cm = ChannelMessages::default();
    cm.upsert(msg("U_BOB", "1783372310.000100", "reply"));
    app.threads
        .insert((team.clone(), "C_GENERAL".into(), root_ts.clone()), cm);

    app.editing = Some(("C_GENERAL".into(), "1783372310.000100".into()));
    app.edit_content = Content::with_text("reply (fixed)");
    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::EditSubmit),
    );

    let cm = &app.threads[&(team, "C_GENERAL".into(), root_ts)];
    assert_eq!(cm.messages[0].text.as_deref(), Some("reply (fixed)"));
    assert!(cm.messages[0].edited.is_some());
}

#[test]
fn message_deleted_ok_removes_from_channel_and_threads() {
    let mut app = test_app();
    let team = app.active_team.clone().unwrap();
    let root_ts = "1783372300.000100".to_owned();
    let mut cm = ChannelMessages::default();
    cm.upsert(msg("U_ALICE", &root_ts, "morning"));
    app.threads
        .insert((team.clone(), "C_GENERAL".into(), root_ts.clone()), cm);

    let _ = update(
        &mut app,
        Message::Workspace(crate::app::WorkspaceMessage::MessageDeleted {
            team: team.clone(),
            channel: "C_GENERAL".into(),
            ts: root_ts.clone(),
            result: Ok(()),
        }),
    );

    assert!(
        !app.workspaces[&team].messages["C_GENERAL"]
            .messages
            .iter()
            .any(|m| m.ts.as_deref() == Some(root_ts.as_str()))
    );
    assert!(
        app.threads[&(team, "C_GENERAL".into(), root_ts)]
            .messages
            .is_empty()
    );
}

#[test]
fn selecting_other_channel_cancels_edit() {
    let mut app = test_app();
    app.editing = Some(("C_GENERAL".into(), "1783372300.000100".into()));
    app.edit_content = Content::with_text("in progress");
    let _ = update(
        &mut app,
        Message::Conversation(crate::app::ConversationMessage::ChannelSelected(
            "C_DEV".into(),
        )),
    );
    assert!(app.editing.is_none());
    assert!(app.edit_content.text().is_empty());
}
