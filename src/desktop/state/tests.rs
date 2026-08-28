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
    state.select_channel(index, ChannelOpen::Global);
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
fn self_menu_fixture_matches_the_away_status_card() {
    let media = MediaRegistry::default();
    let state = ShellState::fixture_variant(media, "self-menu");
    assert_eq!(state.overlay, Some(Overlay::SelfMenu));
    assert_eq!(state.self_account.presence, PresenceVm::Away);
    assert_eq!(state.self_account.user_id, "U0");
    let profile = state
        .core
        .workspaces
        .get("T1")
        .and_then(|workspace| workspace.users.get("U0"))
        .and_then(|user| user.profile.as_ref())
        .expect("self profile");
    assert_eq!(
        profile.status_text.as_deref(),
        Some("You Could Be - MII...")
    );
}

#[test]
fn self_menu_actions_open_profile_and_preferences() {
    let media = MediaRegistry::default();
    let mut state = ShellState::fixture_variant(media, "self-menu");
    state.open_preferences();
    assert_eq!(state.overlay, Some(Overlay::Settings));
    assert_eq!(state.settings_section, SettingsSection::Appearance);

    let mut state = ShellState::fixture_variant(MediaRegistry::default(), "self-menu");
    state.apply_self_presence(PresenceVm::Active);
    assert_eq!(state.self_account.presence, PresenceVm::Active);
    state.apply_self_snooze_minutes(Some(60));
    assert!(state.self_account.snoozed);
    state.clear_self_status();
    let profile = state
        .core
        .workspaces
        .get("T1")
        .and_then(|workspace| workspace.users.get("U0"))
        .and_then(|user| user.profile.as_ref())
        .expect("self profile");
    assert_eq!(profile.status_text.as_deref(), Some(""));
}

#[test]
fn settings_storage_fixture_locks_usage() {
    let media = MediaRegistry::default();
    let state = ShellState::fixture_variant(media, "settings-storage");
    assert_eq!(state.overlay, Some(Overlay::Settings));
    assert_eq!(state.settings_section, SettingsSection::Storage);
    assert!(state.storage.fixture);
    assert!(state.storage.usage.total() > 0);
    assert_eq!(
        state.storage.selected_picture_bytes(),
        state.storage.usage.pictures()
    );
}

#[test]
fn trim_cached_history_keeps_the_open_conversation() {
    let media = MediaRegistry::default();
    let mut state = ShellState::fixture(media);
    let active = state.core.active_channel.clone().expect("active channel");
    let other = state
        .channels
        .iter()
        .map(|channel| channel.id.clone())
        .find(|id| id != &active)
        .expect("another channel");
    if let Some(workspace) = state.core.workspaces.get_mut("T1") {
        let mut messages = super_platinum_core::state::ChannelMessages::default();
        messages.loaded = true;
        messages.upsert(super_platinum_core::slack::models::Message {
            ts: Some("9.0".into()),
            text: Some("stale cache".into()),
            channel: Some(other.clone()),
            ..Default::default()
        });
        workspace.messages.insert(other.clone(), messages);
    }
    state
        .messages_by_channel
        .insert(other.clone(), vec![MessageVm::default()]);
    state.trim_cached_history();

    let workspace = state.core.workspaces.get("T1").expect("fixture workspace");
    let other_messages = workspace.messages.get(&other).expect("other channel");
    assert!(!other_messages.loaded);
    assert!(other_messages.messages.is_empty());
    assert!(!state.messages_by_channel.contains_key(&other));
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

#[test]
fn activity_channel_item_closes_a_stale_thread_pane() {
    let media = MediaRegistry::default();
    let mut state = ShellState::fixture(media);
    let channel = state.channels.first().expect("fixture channel").id.clone();

    // A thread item opens the thread pane and keeps it.
    assert!(state.select_activity_item(
        "thread-1".into(),
        Some(&channel),
        Some("1.000100"),
        Some("1.000000"),
    ));
    state.thread_root = Some("1.000000".into());
    state.thread_at_bottom = false;
    assert!(state.select_activity_item(
        "thread-1".into(),
        Some(&channel),
        Some("1.000100"),
        Some("1.000000"),
    ));
    assert_eq!(state.thread_root.as_deref(), Some("1.000000"));

    // A channel item takes the pane over: Activity shows one surface, never a
    // channel next to the previous item's replies.
    assert!(
        state.select_activity_item("mention-1".into(), Some(&channel), Some("1.000200"), None,)
    );
    assert!(state.thread_root.is_none());
    assert!(state.thread_messages.is_empty());
    assert!(state.thread_at_bottom);
    assert!(state.detail_open());
    assert_eq!(state.core.activity.selected.as_deref(), Some("mention-1"));
}

#[test]
fn activity_item_without_a_message_still_selects() {
    let media = MediaRegistry::default();
    let mut state = ShellState::fixture(media);
    assert!(!state.select_activity_item("orphan-1".into(), None, None, None));
    assert!(state.detail_open());
    assert_eq!(state.core.activity.selected.as_deref(), Some("orphan-1"));
}

#[test]
fn reaching_the_newest_message_drops_a_pending_anchor() {
    let media = MediaRegistry::default();
    let mut state = ShellState::fixture(media);
    state.core.pending_scroll_to = Some((
        "C1".into(),
        super_platinum_core::domain::PendingScrollTarget::FirstUnreadAfter("1.000100".into()),
    ));

    // Still reading history: the anchor is what put them there.
    state.set_timeline_window(0, 1, None::<(String, f64)>, false);
    assert!(state.core.pending_scroll_to.is_some());

    // Once the reader is at the newest message, a late anchor would teleport
    // them back up to the unread divider.
    state.set_timeline_window(0, 1, None::<(String, f64)>, true);
    assert!(state.core.pending_scroll_to.is_none());
}

#[test]
fn a_read_channel_opens_on_its_newest_message() {
    let media = MediaRegistry::default();
    let mut state = ShellState::fixture(media);
    let read = state
        .channels
        .iter()
        .position(|channel| !channel.unread)
        .expect("a read fixture channel");
    // Left the previous conversation scrolled up.
    state.stick_to_bottom = false;

    state.select_channel(read, ChannelOpen::Global);

    assert!(state.stick_to_bottom);
    // The anchor is what actually moves the shared scroll container off the
    // offset the previous conversation left in it.
    assert!(matches!(
        state.core.pending_scroll_to.as_ref(),
        Some((_, super_platinum_core::domain::PendingScrollTarget::Latest))
    ));
}

#[test]
fn the_unread_divider_survives_the_read_mark() {
    let media = MediaRegistry::default();
    let mut state = ShellState::fixture(media);
    let ship = state
        .channels
        .iter()
        .position(|channel| channel.id == "C2")
        .expect("ship");

    state.select_channel(ship, ChannelOpen::Global);
    let divider = state
        .messages
        .iter()
        .find(|message| message.show_unread_divider)
        .map(|message| message.id.clone())
        .expect("fixture ship has unreads");

    // Opening the channel marks it read; the line stays where the reader left
    // off for the rest of the visit.
    if let Some(messages) = state
        .core
        .workspaces
        .get_mut("T1")
        .and_then(|workspace| workspace.messages.get_mut("C2"))
    {
        messages.last_read = messages
            .messages
            .last()
            .and_then(|message| message.ts.clone());
        messages.unread_count = 0;
    }
    state.refresh_from_core();
    assert_eq!(
        state
            .messages
            .iter()
            .find(|message| message.show_unread_divider)
            .map(|message| message.id.as_str()),
        Some(divider.as_str())
    );

    // Coming back later opens a read channel with no line at all.
    let general = state
        .channels
        .iter()
        .position(|channel| channel.id == "C1")
        .expect("general");
    state.select_channel(general, ChannelOpen::Global);
    state.select_channel(ship, ChannelOpen::Global);
    assert!(
        !state
            .messages
            .iter()
            .any(|message| message.show_unread_divider)
    );
}

#[test]
fn a_toast_retires_after_its_five_seconds() {
    let mut state = ShellState::fixture(MediaRegistry::default());
    state.show_toast("Reaction failed");
    let shown = state.toast.as_ref().expect("toast").shown_at;
    state.expire_toast(shown + TOAST_LIFE - std::time::Duration::from_millis(1));
    assert!(state.toast.is_some(), "still readable inside its window");
    state.expire_toast(shown + TOAST_LIFE);
    assert!(state.toast.is_none());
}

#[test]
fn a_dropped_link_never_reaches_the_toast_strip() {
    let mut state = ShellState::fixture(MediaRegistry::default());
    let offline = super_platinum_core::slack::Error::Offline(
        "error sending request for uri (https://example.test/api/client.dms?token=secret)".into(),
    );

    // Background refresh: the rail is already saying it.
    state.report_failure(&offline, "", || {
        format!("Direct messages failed: {offline}")
    });
    assert!(state.toast.is_none());

    // Something the reader asked for gets a short line, never the raw URL.
    state.report_failure(&offline, "Reaction not saved — you are offline", || {
        format!("Reaction failed: {offline}")
    });
    let text = state.toast.as_ref().map(|toast| toast.text.clone());
    assert_eq!(
        text.as_deref(),
        Some("Reaction not saved — you are offline")
    );

    // Anything Slack actually said still reaches the reader verbatim.
    let refused = super_platinum_core::slack::Error::Api("channel_not_found".into());
    state.report_failure(&refused, "You are offline", || {
        format!("Thread failed: {refused}")
    });
    assert_eq!(
        state.toast.as_ref().map(|toast| toast.text.as_str()),
        Some("Thread failed: Slack API returned error: channel_not_found")
    );
}

#[test]
fn the_quick_switcher_lands_in_home_not_inside_a_list_surface() {
    let media = MediaRegistry::default();
    let mut state = ShellState::fixture(media);
    let channel = state.channels.first().expect("fixture channel").id.clone();
    let ship = state
        .channels
        .iter()
        .position(|candidate| candidate.id == "C2")
        .expect("ship");

    // Reading a notification: Activity has a thread open beside its list.
    state.main_view = MainView::Activity;
    assert!(state.select_activity_item(
        "thread-1".into(),
        Some(&channel),
        Some("1.000100"),
        Some("1.000000"),
    ));
    state.thread_root = Some("1.000000".into());

    // Jumping somewhere from the quick switcher is global navigation: Slack
    // opens it in Home with the sidebar, rather than dropping the channel into
    // the Activity pane where it would have no composer and leave a highlighted
    // row pointing at something else.
    state.select_channel(ship, ChannelOpen::Global);

    assert_eq!(state.main_view, MainView::Home);
    assert!(state.thread_root.is_none());
    assert!(state.detail_open());
    assert_eq!(state.core.active_channel.as_deref(), Some("C2"));

    // Activity kept its own conversation for when the reader goes back to it.
    let remembered = state
        .surfaces
        .get(&MainView::Activity)
        .expect("activity keeps what it was showing");
    assert_eq!(remembered.channel, channel);
    assert_eq!(remembered.thread_root.as_deref(), Some("1.000000"));
}

#[test]
fn a_list_surface_opens_beside_its_list_and_stays_there() {
    let media = MediaRegistry::default();
    let mut state = ShellState::fixture(media);
    let dm = state
        .channels
        .iter()
        .position(|channel| channel.is_im)
        .expect("fixture dm");

    state.main_view = MainView::Dms;
    // The DMs list starts on its empty state: a conversation another surface
    // opened must not leak into this column.
    assert!(!state.detail_open());

    state.select_channel(dm, ChannelOpen::InSurface);
    assert_eq!(state.main_view, MainView::Dms);
    assert!(state.detail_open());
}

#[test]
fn an_activity_item_with_no_message_leaves_the_pane_empty() {
    let media = MediaRegistry::default();
    let mut state = ShellState::fixture(media);
    let channel = state.channels.first().expect("fixture channel").id.clone();
    state.main_view = MainView::Activity;
    assert!(state.select_activity_item("mention-1".into(), Some(&channel), Some("1.000200"), None));
    assert!(state.detail_open());

    // An item Slack gave us no message for selects the row and returns the pane
    // to its empty state, rather than leaving the previous item's conversation
    // under a highlight that no longer points at it.
    assert!(!state.select_activity_item("orphan-1".into(), None, None, None));
    assert!(!state.detail_open());
    assert_eq!(state.core.activity.selected.as_deref(), Some("orphan-1"));
}
