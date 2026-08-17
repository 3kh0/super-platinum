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
