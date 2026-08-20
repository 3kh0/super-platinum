use super::*;
use crate::media::MediaRegistry;

#[test]
fn pin_selection_expands_and_freezes_window() {
    let media = MediaRegistry::default();
    let mut state = ShellState::fixture(media);
    let len = state.messages.len().max(1);
    state.timeline_start = len.saturating_sub(20).min(len);
    state.timeline_end = len;
    let before_start = state.timeline_start;
    state.pin_selection();
    assert!(state.selection_pinned);
    assert!(state.timeline_start <= before_start);
    let frozen_start = state.timeline_start;
    let frozen_end = state.timeline_end;
    state.set_timeline_window(0, 1, None::<(String, f64)>, false);
    assert_eq!(state.timeline_start, frozen_start);
    assert_eq!(state.timeline_end, frozen_end);
    state.unpin_selection();
    assert!(!state.selection_pinned);
    state.set_timeline_window(0, 1, None::<(String, f64)>, false);
    assert_ne!(
        (state.timeline_start, state.timeline_end),
        (frozen_start, frozen_end)
    );
}

#[test]
fn select_channel_records_recent_visit() {
    let media = MediaRegistry::default();
    let mut state = ShellState::fixture(media);
    let index = state
        .channels
        .iter()
        .position(|channel| channel.id == "C1")
        .expect("general");
    state.select_channel(index);
    let workspace = state.core.workspaces.get("T1").expect("fixture workspace");
    assert_eq!(workspace.last_active_channel.as_deref(), Some("C1"));
    assert_eq!(
        workspace.recent_channels.first().map(String::as_str),
        Some("C1")
    );
}

#[test]
fn empty_palette_shows_five_recents_not_every_channel() {
    let media = MediaRegistry::default();
    let state = ShellState::fixture(media);
    assert!(state.palette_query.is_empty());
    let matches = state.palette_matches();
    assert_eq!(matches.len(), 5);
    let names: Vec<_> = matches
        .iter()
        .map(|index| state.channels[*index].name.as_str())
        .collect();
    assert_eq!(names, ["Maya Chen", "Jules", "ship", "general", "design"]);
    assert!(state.channels[matches[0]].unread);
    assert!(state.channels[matches[0]].is_im);
    assert!(!state.channels[matches[1]].unread);
    assert!(state.channels[matches[1]].is_im);
}

#[test]
fn empty_palette_does_not_dump_all_channels_without_recents() {
    let media = MediaRegistry::default();
    let mut state = ShellState::fixture(media);
    if let Some(workspace) = state.core.workspaces.get_mut("T1") {
        workspace.recent_channels.clear();
        workspace.last_active_channel = None;
    }
    assert!(state.palette_matches().is_empty());
    assert!(state.channels.len() > 5);
}

#[test]
fn pending_attachments_lookup_by_message_ts() {
    let media = MediaRegistry::default();
    let state = ShellState::fixture_variant(media, "composer-upload-progress");
    let pending = state
        .core
        .pending_file_messages
        .first()
        .expect("fixture pending upload");
    let found = state
        .pending_attachments_for_message(&pending.message_ts)
        .expect("lookup");
    assert_eq!(found.len(), 1);
    assert!(found[0].uploading);
    assert!(state.pending_attachments_for_message("missing").is_none());
}
