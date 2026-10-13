//! Integration regressions against real cached messages, not synthetic VMs.
use super::*;
use crate::{model::RichNode, state::ShellState};
use super_platinum_core::{domain::PendingScrollTarget, slack::models::Message};

fn shell() -> ShellState {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    let mut bag = crate::thread_vm::fixture_bag(10_000);
    for (index, message) in bag.messages.iter_mut().enumerate() {
        message.ts = Some(ts(index));
    }
    bag.last_read = Some(ts(4_999));
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .insert("C2".into(), bag);
    shell.core.active_channel = Some("C2".into());
    shell.core.pending_scroll_to = None;
    shell.unread_anchor = Some(("C2".into(), ts(4_999)));
    shell.refresh_from_core();
    shell
}
fn ts(index: usize) -> String {
    format!("{}.000000", 1_719_800_400 + index * 60)
}
fn bounded(shell: &ShellState) {
    assert_eq!(
        shell.messages.len(),
        shell.timeline_end - shell.timeline_start
    );
    assert!(shell.messages.len() <= 100);
    assert_eq!(
        shell
            .messages
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        shell.channel_rows[shell.timeline_start..shell.timeline_end]
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>()
    );
}
fn far(shell: &mut ShellState, index: usize) {
    shell.core.pending_scroll_to = None;
    shell.set_timeline_window(index, index + 6, None::<(String, f64)>, false);
    bounded(shell);
    assert!(shell.timeline_start <= index && shell.timeline_end > index);
}
fn bag(shell: &mut ShellState) -> &mut super_platinum_core::state::ChannelMessages {
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut("C2")
        .unwrap()
}

#[test]
fn far_window_refresh_uses_absolute_rows_and_refreshes_edit_reaction_and_file() {
    let mut shell = shell();
    assert_eq!(shell.channel_rows.len(), 10_001);
    far(&mut shell, 4_200);
    let before = (shell.timeline_start, shell.timeline_end);
    let message = &mut bag(&mut shell).messages[4_200];
    message.text = Some("Edited outside latest 日本語".into());
    message.blocks = vec![];
    message.edited = Some(serde_json::json!({"ts":"1800000000.000001"}));
    message.reactions = vec![
        serde_json::from_value(serde_json::json!({"name":"wave","count":9,"users":["U1"]}))
            .unwrap(),
    ];
    message.files = vec![serde_json::from_value(serde_json::json!({"id":"FFarWindow","name":"before.txt","title":"Before file title","mimetype":"text/plain","url_private":"https://files.slack.com/files-pri/T1-FFarWindow/before.txt"})).unwrap()];
    shell.refresh_from_core();
    assert_eq!((shell.timeline_start, shell.timeline_end), before);
    bounded(&shell);
    let row = shell
        .messages
        .iter()
        .find(|row| row.ts == ts(4_200))
        .unwrap();
    assert!(row.edited);
    assert!(format!("{:?}", row.body).contains("Edited outside latest 日本語"));
    assert_eq!(row.reactions[0].count, 9);
    shell
        .core
        .rename_file("T1", "FFarWindow", "Renamed far file", Some("renamed.txt"));
    shell.refresh_from_core();
    assert!(
        format!(
            "{:?}",
            shell
                .messages
                .iter()
                .find(|row| row.ts == ts(4_200))
                .unwrap()
                .body
        )
        .contains("Renamed far file")
    );
    shell.core.remove_file("T1", "FFarWindow");
    shell.refresh_from_core();
    assert!(
        !format!(
            "{:?}",
            shell
                .messages
                .iter()
                .find(|row| row.ts == ts(4_200))
                .unwrap()
                .body
        )
        .contains("FFarWindow")
    );
    bounded(&shell);
}

#[test]
fn off_window_edits_materialize_only_when_their_absolute_window_is_opened() {
    let mut shell = shell();
    far(&mut shell, 7_500);
    let message = &mut bag(&mut shell).messages[250];
    message.text = Some("New source content not a stale projection".into());
    message.blocks.clear();
    shell.refresh_from_core();
    assert!(!shell.messages.iter().any(|row| row.ts == ts(250)));
    far(&mut shell, 250);
    assert!(shell.messages.iter().any(|row| row.ts == ts(250)
        && row.body
            == vec![RichNode::Text(
                "New source content not a stale projection".into()
            )]));
}

#[test]
fn prepend_and_delete_preserve_surviving_far_window_ids() {
    let mut shell = shell();
    far(&mut shell, 6_000);
    let held = shell
        .messages
        .iter()
        .map(|row| row.id.clone())
        .collect::<Vec<_>>();
    let start = shell.timeline_start;
    let mut older = bag(&mut shell).messages[0].clone();
    older.ts = Some("1700000000.000001".into());
    bag(&mut shell).messages.insert(0, older);
    shell.refresh_from_core();
    assert_eq!(shell.timeline_start, start + 1);
    assert_eq!(
        shell.messages.iter().map(|row| &row.id).collect::<Vec<_>>(),
        held.iter().collect::<Vec<_>>()
    );
    assert!(bag(&mut shell).remove(&held[5]));
    shell.refresh_from_core();
    bounded(&shell);
    assert_eq!(
        shell.messages.iter().map(|row| &row.id).collect::<Vec<_>>(),
        held.iter().filter(|id| *id != &held[5]).collect::<Vec<_>>()
    );
}

#[test]
fn search_unread_and_date_targets_are_not_limited_to_latest_projection() {
    let mut shell = shell();
    shell.core.pending_scroll_to = Some(("C2".into(), PendingScrollTarget::Message(ts(200))));
    shell.reset_timeline_window();
    bounded(&shell);
    assert!(shell.messages.iter().any(|row| row.ts == ts(200)));
    shell.set_timeline_window(9_980, 10_000, None::<(String, f64)>, true);
    assert!(shell.messages.iter().any(|row| row.ts == ts(200)));
    shell.core.pending_scroll_to = Some((
        "C2".into(),
        PendingScrollTarget::FirstUnreadAfter(ts(4_999)),
    ));
    shell.reset_timeline_window();
    assert!(
        shell
            .messages
            .iter()
            .any(|row| row.ts == ts(5_000) && row.show_unread_divider)
    );
    let boundary = shell
        .channel_rows
        .iter()
        .enumerate()
        .find(|(index, row)| *index > 2_000 && row.date_label.is_some())
        .unwrap()
        .0;
    far(&mut shell, boundary);
    let full = project_messages_for_channel(
        &shell.core.workspaces["T1"],
        "C2",
        &shell.media,
        shell.divider_at("C2"),
    );
    assert_eq!(
        shell.messages,
        full[shell.timeline_start..shell.timeline_end]
    );
    assert!(
        shell
            .messages
            .iter()
            .any(|row| row.ts == ts(boundary) && row.date_label.is_some())
    );
}

#[test]
fn pending_confirmation_keeps_one_identity_in_latest_bounded_projection() {
    let mut shell = shell();
    let pending = ts(10_000);
    let source = bag(&mut shell);
    source.pending.push(pending.clone());
    source.messages.last_mut().unwrap().client_msg_id = Some("window-pending-client".into());
    shell.core.pending_scroll_to = None;
    shell.stick_to_bottom = true;
    shell.refresh_from_core();
    assert!(shell.messages.last().unwrap().pending);
    let mut confirmed: Message = bag(&mut shell).messages.last().unwrap().clone();
    confirmed.ts = Some(ts(10_001));
    assert!(bag(&mut shell).confirm("window-pending-client", confirmed));
    shell.refresh_from_core();
    bounded(&shell);
    assert_eq!(shell.channel_rows.len(), 10_001);
    assert!(!shell.channel_rows.iter().any(|row| row.id == pending));
    assert_eq!(shell.messages.last().unwrap().ts, ts(10_001));
    assert!(!shell.messages.last().unwrap().pending);
}

#[test]
fn selection_pin_holds_far_rows_across_refresh_and_releases_to_new_window() {
    let mut shell = shell();
    far(&mut shell, 3_500);
    let held = shell.messages.clone();
    let bounds = (shell.timeline_start, shell.timeline_end);
    shell.pin_selection();
    shell.set_timeline_window(8_000, 8_010, None::<(String, f64)>, false);
    shell.refresh_from_core();
    assert_eq!((shell.timeline_start, shell.timeline_end), bounds);
    assert_eq!(shell.messages, held);
    shell.unpin_selection();
    far(&mut shell, 8_000);
    assert_ne!(shell.messages.first().unwrap().id, held[0].id);
}
