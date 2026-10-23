use super::*;
use crate::{media::MediaRegistry, model::RichNode};
use serde_json::json;
use super_platinum_core::{
    composer_emoji::standard::Version, slack_lists::comments::edit_seed::prepare::StandardEmoji,
    state::ChannelMessages,
};

const ROOT: &str = "1719800400.000000";

fn shell() -> ShellState {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    let mut bag = ChannelMessages::default();
    for i in 0..4 {
        bag.messages.push(
            serde_json::from_value(json!({
                "ts":format!("{}.000000",1719800400+i), "user":"U1",
                "text":"unchanged fallback", "blocks":[{"type":"rich_text","elements":[{
                    "type":"rich_text_section","elements":[
                        {"type":"emoji","name":"conflicting","unicode":"1f44d-1f3fd",
                         "display_url":"https://fixture.invalid/selected.png"},
                        {"type":"emoji","name":"conflicting","unicode":"0041",
                         "display_url":"https://fixture.invalid/unknown.png"},
                        {"type":"emoji","name":"conflicting","unicode":"1F44D",
                         "display_url":"https://fixture.invalid/invalid.png"}
                    ]}]}]
            }))
            .unwrap(),
        );
    }
    bag.pending.push("1719800403.000000".into());
    bag.last_read = Some(ROOT.into());
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .insert("C2".into(), bag.clone());
    shell
        .core
        .threads
        .insert(("T1".into(), "C2".into(), ROOT.into()), bag);
    shell.core.active_channel = Some("C2".into());
    shell.core.pending_scroll_to = None;
    shell.thread_root = Some(ROOT.into());
    shell.refresh_from_core();
    shell
}

fn emoji_leaves(nodes: &[RichNode]) -> Vec<&RichNode> {
    nodes
        .iter()
        .flat_map(|node| match node {
            RichNode::Paragraph(children) | RichNode::Quote(children) => emoji_leaves(children),
            RichNode::Emoji { .. } | RichNode::EmojiImage { .. } => vec![node],
            _ => vec![],
        })
        .collect()
}

#[test]
fn channel_thread_emoji_selected_projection_keeps_raw_and_window_annotations() {
    let shell = shell();
    let workspace = &shell.core.workspaces["T1"];
    let bag = &shell.core.threads[&("T1".into(), "C2".into(), ROOT.into())];
    let original = serde_json::to_value(&bag.messages).unwrap();
    let standard = StandardEmoji::build(Version::V17).unwrap();
    let channel = crate::channel_vm::project_channel_window_with_standard(
        workspace,
        "C2",
        &shell.media,
        &shell.channel_rows,
        1..4,
        Some(&standard),
    );
    let ordinary = crate::channel_vm::project_channel_window(
        workspace,
        "C2",
        &shell.media,
        &shell.channel_rows,
        1..4,
    );
    let thread = crate::thread_vm::project_window_with_standard(
        workspace,
        bag,
        &shell.thread_history,
        ROOT,
        &shell.media,
        &shell.thread_window,
        Some(&standard),
    );
    assert_eq!(channel.len(), 3);
    assert_eq!(thread.len(), 4);
    for (selected, original) in channel.iter().zip(&ordinary) {
        assert_eq!(
            (
                &selected.id,
                selected.pending,
                selected.compact,
                &selected.date_label,
                selected.show_unread_divider
            ),
            (
                &original.id,
                original.pending,
                original.compact,
                &original.date_label,
                original.show_unread_divider
            )
        );
    }
    for row in channel.iter().chain(&thread) {
        let leaves = emoji_leaves(&row.body);
        assert_eq!(leaves.len(), 2, "invalid original Unicode remains absent");
        assert!(matches!(leaves[0], RichNode::EmojiImage {name,..} if name=="+1::skin-tone-4"));
        assert!(matches!(leaves[1], RichNode::Emoji {glyph,..} if glyph=="A"));
    }
    assert!(channel.last().unwrap().pending);
    assert!(thread.last().unwrap().pending);
    assert_eq!(serde_json::to_value(&bag.messages).unwrap(), original);
}

#[test]
fn channel_thread_emoji_notification_removes_retired_cached_artwork_without_draft_changes() {
    let mut shell = shell();
    let standard = StandardEmoji::build(Version::V17).unwrap();
    // Retain a previous selected projection, then notify with no current source.
    // This exercises VM retirement, not installation of assignment authority.
    shell.messages = crate::channel_vm::project_channel_window_with_standard(
        &shell.core.workspaces["T1"],
        "C2",
        &shell.media,
        &shell.channel_rows,
        shell.timeline_start..shell.timeline_end,
        Some(&standard),
    );
    shell.thread_messages = crate::thread_vm::project_window_with_standard(
        &shell.core.workspaces["T1"],
        &shell.core.threads[&("T1".into(), "C2".into(), ROOT.into())],
        &shell.thread_history,
        ROOT,
        &shell.media,
        &shell.thread_window,
        Some(&standard),
    );
    shell.core.composer.text = "Channel draft kept".into();
    shell.core.thread_composer.text = "Thread draft kept".into();
    shell.selection_pinned = true;
    shell.thread_window.selection_pinned = true;
    let scope = shell.timeline_scope();
    let thread_scope = shell.thread_scope();
    let nav = shell.channel_generation;
    let bounds = (shell.timeline_start, shell.timeline_end);
    let indices = shell.thread_window.selected_indices().to_vec();
    let channel_stamp = shell.channel_projection_generation;
    let thread_stamp = shell.thread_window.projection_generation;
    let raw = serde_json::to_value(&shell.core.workspaces["T1"].messages["C2"].messages).unwrap();
    shell.emoji_standard_changed("T1");
    assert_eq!(shell.timeline_scope(), scope);
    assert_eq!(shell.thread_scope(), thread_scope);
    assert_eq!(shell.channel_generation, nav);
    assert_eq!((shell.timeline_start, shell.timeline_end), bounds);
    assert_eq!(shell.thread_window.selected_indices(), indices);
    assert!(shell.selection_pinned && shell.thread_window.selection_pinned);
    assert_eq!(shell.core.composer.text, "Channel draft kept");
    assert_eq!(shell.core.thread_composer.text, "Thread draft kept");
    assert!(shell.channel_projection_generation > channel_stamp);
    assert!(shell.thread_window.projection_generation > thread_stamp);
    for row in shell.messages.iter().chain(&shell.thread_messages) {
        assert!(
            matches!(emoji_leaves(&row.body)[0],RichNode::Emoji {glyph,..}
            if glyph=="\u{1f44d}\u{1f3fd}")
        );
    }
    assert_eq!(
        serde_json::to_value(&shell.core.workspaces["T1"].messages["C2"].messages).unwrap(),
        raw
    );
}

#[test]
fn channel_thread_emoji_foreign_publication_leaves_caches_and_stamps_untouched() {
    let mut shell = shell();
    let channel = shell.messages.clone();
    let thread = shell.thread_messages.clone();
    let stamps = (
        shell.channel_projection_generation,
        shell.thread_window.projection_generation,
    );
    shell.emoji_standard_changed("TFOREIGN");
    assert_eq!(shell.messages, channel);
    assert_eq!(shell.thread_messages, thread);
    assert_eq!(
        (
            shell.channel_projection_generation,
            shell.thread_window.projection_generation
        ),
        stamps
    );
}

#[test]
fn channel_thread_emoji_same_row_ids_do_not_authorize_old_measurement_or_old_dom() {
    let mut shell = shell();
    let old = shell.channel_projection_generation;
    let old_dom = old.to_string();
    let ids = shell
        .messages
        .iter()
        .map(|m| m.id.clone())
        .collect::<Vec<_>>();
    assert!(shell.channel_projection_receipt_is_current(old, Some(&old_dom)));
    shell.emoji_standard_changed("T1");
    assert_eq!(
        shell
            .messages
            .iter()
            .map(|m| m.id.clone())
            .collect::<Vec<_>>(),
        ids
    );
    let current = shell.channel_projection_generation;
    let current_dom = current.to_string();
    assert!(!shell.channel_projection_receipt_is_current(old, Some(&old_dom)));
    assert!(!shell.channel_projection_receipt_is_current(current, Some(&old_dom)));
    assert!(!shell.channel_projection_receipt_is_current(old, Some(&current_dom)));
    assert!(!shell.channel_projection_receipt_is_current(current, None));
    assert!(!shell.channel_projection_receipt_is_current(current, Some("invalid")));
    assert!(shell.channel_projection_receipt_is_current(current, Some(&current_dom)));
}

#[test]
fn channel_thread_emoji_legacy_fixture_rows_are_not_replaced_by_notification() {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    let messages = shell.messages.clone();
    shell.install_fixture_messages(messages);
    let before = shell.messages.clone();
    let generation = shell.channel_projection_generation;
    shell.emoji_standard_changed("T1");
    assert_eq!(shell.messages, before);
    assert_eq!(shell.channel_projection_generation, generation);
}
