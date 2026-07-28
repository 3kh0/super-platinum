use super::*;

#[test]
fn unreads_only_include_messages_after_last_read() {
    let app = unreads_app();
    let ws = app.active_workspace().expect("workspace");
    let messages = crate::ui::unreads::unread_messages(ws, "C_GENERAL");
    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages.first().and_then(|message| message.ts.as_deref()),
        Some("1783372310.000200")
    );
}

#[test]
fn unreads_sort_matches_selected_direction() {
    let app = unreads_app();
    let ws = app.active_workspace().expect("workspace");
    let newest = crate::ui::unreads::ordered_channels(ws, UnreadsSort::Newest);
    let oldest = crate::ui::unreads::ordered_channels(ws, UnreadsSort::Oldest);
    assert_eq!(newest[0].id, "C_GENERAL");
    assert_eq!(oldest[0].id, "C_DEV");
}

#[test]
fn unreads_history_result_populates_the_group_without_marking_it_read() {
    let mut app = unreads_app();
    app.unreads.loaded.remove("C_GENERAL");
    app.unreads.loading.insert("C_GENERAL".into(), 7);
    let _ = update(
        &mut app,
        Message::Runtime(RuntimeMessage::UnreadsChannelLoaded {
            team: "T_TEST".into(),
            channel: "C_GENERAL".into(),
            seq: 7,
            result: Ok(HistoryPage {
                messages: vec![msg(
                    "U_ALICE",
                    "1783372400.000100",
                    "new unread from the history request",
                )],
                ..Default::default()
            }),
        }),
    );
    assert!(app.unreads.loaded.contains("C_GENERAL"));
    assert!(!app.unreads.loading.contains_key("C_GENERAL"));
    let ws = app.active_workspace().expect("workspace");
    assert_eq!(ws.unread_total(&ws.channels["C_GENERAL"]), 2);
    assert!(
        ws.messages["C_GENERAL"]
            .messages
            .iter()
            .any(|message| message.text.as_deref() == Some("new unread from the history request"))
    );
}

#[test]
fn successful_mark_removes_the_conversation_from_unreads() {
    let mut app = unreads_app();
    app.unreads.failed.insert("C_GENERAL".into());
    app.unreads.collapsed.insert("C_GENERAL".into());
    let _ = update(
        &mut app,
        Message::Workspace(WorkspaceMessage::ChannelMarked(
            "T_TEST".into(),
            "C_GENERAL".into(),
            "1783372320.000300".into(),
            Ok(()),
        )),
    );
    let ws = app.active_workspace().expect("workspace");
    assert_eq!(ws.unread_total(&ws.channels["C_GENERAL"]), 0);
    assert!(!app.unreads.loaded.contains("C_GENERAL"));
    assert!(!app.unreads.failed.contains("C_GENERAL"));
    assert!(!app.unreads.collapsed.contains("C_GENERAL"));
}
