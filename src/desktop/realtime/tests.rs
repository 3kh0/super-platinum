use super::*;
use super_platinum_core::slack::models::{Message as SlackMessage, Reaction};
use super_platinum_core::state::Workspace;

const TEAM: &str = "T1";
const CHANNEL: &str = "C1";
const ROOT: &str = "1.0";

#[test]
fn clear_unreads_confirmation_pref_obeys_socket_generation_and_stays_independent() {
    use super_platinum_core::state::{ClearUnreadsPreference, ReadMode, ReadPreferences};
    let mut state = core();
    let workspace = state.workspaces.get_mut(TEAM).unwrap();
    workspace.clear_unreads_preference = ClearUnreadsPreference::from_prefs(
        &[("confirm_clear_all_unreads".into(), true.into())].into(),
    );
    workspace.read_preferences = ReadPreferences::from_prefs(&ReadMode::NewestKeepUnseen.patch());
    state.composer.text = "retained draft".into();
    let event = || RtEvent::PreferencesChanged {
        name: "confirm_clear_all_unreads".into(),
        value: false.into(),
    };
    apply(&mut state, TEAM, 1, event());
    assert_eq!(
        state.workspaces[TEAM]
            .clear_unreads_preference
            .requires_confirmation(),
        Ok(true)
    );
    apply(&mut state, TEAM, 0, event());
    assert_eq!(
        state.workspaces[TEAM]
            .clear_unreads_preference
            .requires_confirmation(),
        Ok(false)
    );
    assert_eq!(
        state.workspaces[TEAM].read_preferences.mode(),
        Ok(ReadMode::NewestKeepUnseen)
    );
    assert_eq!(state.composer.text, "retained draft");
}

#[test]
fn reading_preferences_follow_socket_generation_without_touching_drafts() {
    use super_platinum_core::state::{ReadMode, ReadPreferences};
    let mut state = core();
    state.workspaces.get_mut(TEAM).unwrap().read_preferences =
        ReadPreferences::from_prefs(&ReadMode::LeftOffAndRead.patch());
    state.composer.text = "keep channel draft".into();
    state.thread_composer.text = "keep thread draft".into();
    let event = || RtEvent::PreferencesChanged {
        name: "mark_msgs_read_immediately".into(),
        value: false.into(),
    };
    apply(&mut state, TEAM, 1, event());
    assert_eq!(
        state.workspaces[TEAM].read_preferences.mode(),
        Ok(ReadMode::LeftOffAndRead)
    );
    apply(&mut state, TEAM, 0, event());
    assert_eq!(
        state.workspaces[TEAM].read_preferences.mode(),
        Ok(ReadMode::NewestKeepUnseen)
    );
    assert_eq!(state.composer.text, "keep channel draft");
    assert_eq!(state.thread_composer.text, "keep thread draft");
}

#[test]
fn locale_preferences_follow_workspace_socket_generation() {
    let mut core = core();
    let event = || RtEvent::PreferencesChanged {
        name: "locale".into(),
        value: serde_json::json!("de-DE"),
    };
    apply(&mut core, TEAM, 1, event());
    assert_eq!(core.workspaces[TEAM].locale, None);
    apply(&mut core, TEAM, 0, event());
    assert_eq!(core.workspaces[TEAM].locale.as_deref(), Some("de-DE"));
    apply(
        &mut core,
        "MISSING",
        0,
        RtEvent::PreferencesChanged {
            name: "locale".into(),
            value: serde_json::json!("fr-FR"),
        },
    );
    assert_eq!(core.workspaces[TEAM].locale.as_deref(), Some("de-DE"));
}

#[test]
fn remote_message_deletion_records_unloaded_channel_and_existing_thread_tombstones() {
    let mut core = core();
    core.workspaces.get_mut(TEAM).unwrap().rt_generation = 4;
    let channel = "never-loaded-channel";
    let ts = "1234567890.123456";
    let key = (TEAM.into(), channel.into(), ROOT.into());
    core.threads
        .entry(key.clone())
        .or_default()
        .upsert(message(ts, Some(ROOT)));
    let event = || RtEvent::MessageDeleted {
        channel: channel.into(),
        deleted_ts: ts.into(),
    };
    apply(&mut core, TEAM, 3, event());
    assert!(!core.workspaces[TEAM].messages.contains_key(channel));
    assert!(!core.threads[&key].was_deleted(ts));
    apply(&mut core, TEAM, 4, event());
    assert!(core.workspaces[TEAM].messages[channel].was_deleted(ts));
    assert!(core.workspaces[TEAM].messages[channel].messages.is_empty());
    assert!(core.threads[&key].was_deleted(ts));
    assert!(core.threads[&key].messages.is_empty());
    assert!(!core.workspaces[TEAM].messages[CHANNEL].was_deleted(ts));
}

#[test]
fn thread_feed_edits_follow_the_socket_generation_and_workspace() {
    let mut core = core();
    core.active_team = Some(TEAM.into());
    core.workspaces.get_mut(TEAM).unwrap().rt_generation = 4;
    core.threads_view.ensure_scope(core.account_epoch, TEAM);
    core.threads_view
        .upsert(super_platinum_core::slack::models::ThreadViewItem {
            root_msg: message(ROOT, Some(ROOT)),
            ..Default::default()
        });
    let event = || RtEvent::MessageChanged {
        channel: CHANNEL.into(),
        message: SlackMessage {
            ts: Some(ROOT.into()),
            text: Some("updated preview".into()),
            ..Default::default()
        },
    };
    apply(&mut core, TEAM, 3, event());
    assert_eq!(
        core.threads_view.items[0].root_msg.text.as_deref(),
        Some("hello")
    );
    apply(&mut core, TEAM, 4, event());
    assert_eq!(
        core.threads_view.items[0].root_msg.text.as_deref(),
        Some("updated preview")
    );
    core.threads_view.ensure_scope(core.account_epoch, "OTHER");
    apply(&mut core, TEAM, 4, event());
    assert!(core.threads_view.items.is_empty());
}

#[test]
fn global_thread_counts_are_workspace_and_connection_scoped_and_accept_backward_cursors() {
    let mut core = core();
    core.workspaces.get_mut(TEAM).unwrap().rt_generation = 4;
    core.workspaces
        .insert("OTHER".into(), core.workspaces[TEAM].clone());
    core.active_team = Some("OTHER".into());
    let event = |timestamp: &str, unread: bool| {
        super_platinum_core::slack::realtime::parse_event(&serde_json::json!({
            "type": "update_global_thread_state", "has_unreads": unread,
            "mention_count": if unread { 7 } else { 0 }, "vip_count": 0,
            "mention_count_by_channel": {},
            "unread_count_by_channel": if unread { serde_json::json!({"C1":1}) } else { serde_json::json!({}) },
            "timestamp": timestamp
        }).to_string()).unwrap()
    };
    apply(&mut core, TEAM, 3, event("200.0", true));
    apply(&mut core, "MISSING", 4, event("200.0", true));
    assert!(core.workspaces[TEAM].thread_counts.snapshot.is_none());
    apply(&mut core, TEAM, 4, event("200.0", false));
    // A manual mark-unread uses an older cursor as its event timestamp.
    apply(&mut core, TEAM, 4, event("100.0", true));
    let counts = core.workspaces[TEAM]
        .thread_counts
        .snapshot
        .as_ref()
        .unwrap();
    assert!(counts.has_unreads);
    assert_eq!(counts.mention_count, 7);
    assert_eq!(counts.unread_count_by_channel[CHANNEL], 1);
    assert!(core.workspaces["OTHER"].thread_counts.snapshot.is_none());
    apply(&mut core, TEAM, 4, event("201.0", false));
    assert!(
        core.workspaces[TEAM]
            .thread_counts
            .snapshot
            .as_ref()
            .unwrap()
            .unread_count_by_channel
            .is_empty()
    );
}

fn message(ts: &str, thread_ts: Option<&str>) -> SlackMessage {
    SlackMessage {
        ts: Some(ts.into()),
        channel: Some(CHANNEL.into()),
        user: Some("U1".into()),
        text: Some("hello".into()),
        thread_ts: thread_ts.map(str::to_owned),
        ..Default::default()
    }
}

/// A workspace holding one reacted root message, with the thread open on it
/// and carrying its own copy of the root and one reply — the shape the
/// thread pane draws from.
fn core() -> super_platinum_core::CoreAppState {
    let mut core =
        super_platinum_core::CoreAppState::new(super_platinum_core::config::Settings::default());
    let mut workspace = Workspace::from_session(&super_platinum_core::config::WorkspaceSession {
        team_id: TEAM.into(),
        enterprise_id: None,
        user_id: "USELF".into(),
        name: "test".into(),
        url: "https://t".into(),
        token: String::new(),
    });
    let mut root = message(ROOT, Some(ROOT));
    root.reactions = vec![Reaction {
        name: "tada".into(),
        users: vec!["U2".into()],
        count: 1,
        ..Default::default()
    }];
    workspace
        .messages
        .entry(CHANNEL.into())
        .or_default()
        .upsert(root.clone());
    core.workspaces.insert(TEAM.into(), workspace);
    let thread = core
        .threads
        .entry((TEAM.into(), CHANNEL.into(), ROOT.into()))
        .or_default();
    thread.upsert(root);
    thread.upsert(message("2.0", Some(ROOT)));
    core
}

#[test]
fn channel_metadata_is_connection_guarded_and_preserves_other_workspaces_and_drafts() {
    use super_platinum_core::slack::models::Channel;
    let mut core = core();
    let workspace = core.workspaces.get_mut(TEAM).unwrap();
    workspace.rt_generation = 4;
    workspace.channels.insert(
        CHANNEL.into(),
        Channel {
            id: CHANNEL.into(),
            name: Some("before".into()),
            is_private: true,
            ..Default::default()
        },
    );
    let other = workspace.clone();
    core.workspaces.insert("OTHER".into(), other);
    core.active_team = Some("OTHER".into());
    core.composer.text = "unsent channel draft".into();
    core.thread_composer.text = "unsent thread draft".into();
    let event = || {
        super_platinum_core::slack::realtime::parse_event(
            r#"{"type":"group_rename","channel":{"id":"C1","name":"after"},"event_ts":"20.1"}"#,
        )
        .unwrap()
    };
    apply(&mut core, TEAM, 3, event());
    assert_eq!(
        core.workspaces[TEAM].channels[CHANNEL].name.as_deref(),
        Some("before")
    );
    apply(&mut core, "MISSING", 4, event());
    apply(&mut core, TEAM, 4, event());
    assert_eq!(
        core.workspaces[TEAM].channels[CHANNEL].name.as_deref(),
        Some("after")
    );
    assert_eq!(
        core.workspaces["OTHER"].channels[CHANNEL].name.as_deref(),
        Some("before")
    );
    assert_eq!(core.composer.text, "unsent channel draft");
    assert_eq!(core.thread_composer.text, "unsent thread draft");
    assert!(core.workspaces[TEAM].channels[CHANNEL].is_private);
}

#[test]
fn draft_events_are_workspace_and_connection_generation_guarded() {
    let mut core = core();
    let generation = core.workspaces[TEAM].rt_generation;
    let raw = serde_json::from_value(serde_json::json!({
        "type":"draft_update", "draft_id":"DrLive", "event_ts":"10.1",
        "draft":{"id":"DrLive","team_id":TEAM,"last_updated_ts":"10.2",
            "blocks":[],"file_ids":["F1"],"destinations":[{"channel_id":CHANNEL,"thread_ts":ROOT}]}
    }))
    .unwrap();
    let event = RtEvent::Unknown(raw);
    apply(&mut core, TEAM, generation.wrapping_add(1), event.clone());
    assert!(core.scheduled.items.is_empty());
    core.active_team = Some("OTHER".into());
    apply(&mut core, TEAM, generation, event.clone());
    assert!(core.scheduled.items.is_empty());
    core.active_team = Some(TEAM.into());
    apply(&mut core, TEAM, generation, event);
    assert_eq!(core.scheduled.items["DrLive"].file_ids, vec!["F1"]);
    assert_eq!(core.scheduled.items["DrLive"].thread_ts(), Some(ROOT));
    assert_eq!(core.scheduled.items.len(), 1);
}

#[test]
fn pins_reconcile_transcript_and_thread_and_receive_reactions() {
    use super_platinum_core::pins::{PinnedItem, PinsPage};
    let mut core = core();
    core.pins.ensure_scope(core.account_epoch, TEAM);
    let generation = core.pins.begin(CHANNEL);
    core.pins.apply(
        generation,
        PinsPage {
            items: vec![PinnedItem {
                kind: "message".into(),
                message: Some(message(ROOT, Some(ROOT))),
                ..Default::default()
            }],
        },
    );
    core.reconcile_pin_copies(TEAM, CHANNEL);
    assert_eq!(
        core.workspaces[TEAM].messages[CHANNEL].messages[0].extra["pinned_to"],
        serde_json::json!([CHANNEL])
    );
    assert_eq!(
        core.threads[&(TEAM.into(), CHANNEL.into(), ROOT.into())].messages[0].extra["pinned_to"],
        serde_json::json!([CHANNEL])
    );
    apply_reaction(&mut core, TEAM, CHANNEL, ROOT, "U2", "eyes", true);
    assert_eq!(
        core.pins.items[&format!("message:{ROOT}")]
            .message
            .as_ref()
            .unwrap()
            .reactions[0]
            .name,
        "eyes"
    );
    let generation = core.pins.begin(CHANNEL);
    core.pins.apply(generation, PinsPage::default());
    core.reconcile_pin_copies(TEAM, CHANNEL);
    assert_eq!(
        core.workspaces[TEAM].messages[CHANNEL].messages[0].extra["pinned_to"],
        serde_json::json!([])
    );
    assert_eq!(
        core.threads[&(TEAM.into(), CHANNEL.into(), ROOT.into())].messages[0].extra["pinned_to"],
        serde_json::json!([])
    );
}

#[test]
fn saved_events_ignore_old_connections_and_other_workspaces() {
    let mut core = core();
    core.active_team = Some(TEAM.into());
    core.workspaces.get_mut(TEAM).unwrap().rt_generation = 4;
    let event = || {
        super_platinum_core::slack::realtime::parse_event(r#"{"type":"saved_added","saved":{"item_type":"message","item_id":"C1","ts":"1.2"},"counts":{"uncompleted_count":1},"event_ts":"20.0"}"#).unwrap()
    };
    apply(&mut core, TEAM, 3, event());
    assert!(core.later.items.is_empty());
    core.active_team = Some("OTHER".into());
    apply(&mut core, TEAM, 4, event());
    assert!(core.later.items.is_empty());
    core.active_team = Some(TEAM.into());
    apply(&mut core, TEAM, 4, event());
    assert_eq!(core.later.items.len(), 1);
    assert_eq!(core.later.counts.uncompleted_count, 1);
}

fn reactions(core: &super_platinum_core::CoreAppState, ts: &str) -> Vec<String> {
    core.workspaces[TEAM].messages[CHANNEL]
        .messages
        .iter()
        .find(|message| message.ts.as_deref() == Some(ts))
        .map(|message| message.reactions.iter().map(|r| r.name.clone()).collect())
        .unwrap_or_default()
}

fn thread_reactions(core: &super_platinum_core::CoreAppState, ts: &str) -> Vec<String> {
    core.threads[&(TEAM.into(), CHANNEL.into(), ROOT.into())]
        .messages
        .iter()
        .find(|message| message.ts.as_deref() == Some(ts))
        .map(|message| message.reactions.iter().map(|r| r.name.clone()).collect())
        .unwrap_or_default()
}

#[test]
fn user_change_updates_self_status_without_dropping_existing_identity() {
    let mut core = core();
    core.workspaces.get_mut(TEAM).unwrap().users.insert(
        "USELF".into(),
        super_platinum_core::slack::models::User {
            id: "USELF".into(),
            name: Some("Taylor".into()),
            profile: Some(super_platinum_core::slack::models::UserProfile {
                image_72: Some("https://example.test/avatar.png".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    apply(
        &mut core,
        TEAM,
        0,
        RtEvent::UserChanged(super_platinum_core::slack::models::User {
            id: "USELF".into(),
            profile: Some(super_platinum_core::slack::models::UserProfile {
                status_text: Some("Reviewing".into()),
                status_emoji: Some(":ship:".into()),
                ..Default::default()
            }),
            ..Default::default()
        }),
    );

    let user = &core.workspaces[TEAM].users["USELF"];
    assert_eq!(user.name.as_deref(), Some("Taylor"));
    let profile = user.profile.as_ref().unwrap();
    assert_eq!(
        profile.image_72.as_deref(),
        Some("https://example.test/avatar.png")
    );
    assert_eq!(profile.status_text.as_deref(), Some("Reviewing"));
    assert_eq!(profile.status_emoji.as_deref(), Some(":ship:"));
}

#[test]
fn a_thread_reply_does_not_wipe_the_parents_reactions() {
    let mut core = core();
    // What Slack sends the moment someone replies in a thread: the parent,
    // complete except that it names no reactions at all.
    let parent = super_platinum_core::slack::realtime::parse_event(
        r#"{"type":"message","subtype":"message_replied","channel":"C1","message":{"ts":"1.0","thread_ts":"1.0","text":"hello","reply_count":1}}"#
    ).unwrap();
    apply(&mut core, TEAM, 0, parent);
    assert_eq!(reactions(&core, ROOT), ["tada"]);
    assert_eq!(thread_reactions(&core, ROOT), ["tada"]);
    assert_eq!(
        core.threads[&(TEAM.into(), CHANNEL.into(), ROOT.into())].messages[0].reply_count,
        Some(1)
    );
}

#[test]
fn live_notification_preferences_update_without_resetting_sidebar_settings() {
    let mut core = core();
    core.workspaces.get_mut(TEAM).unwrap().sidebar.behavior =
        "hide_read_channels_unless_starred".into();
    let value = serde_json::json!({"global":{"global_desktop_push_enabled":false},"channels":{"C1":{"muted":true}}}).to_string();
    let event = RtEvent::PreferencesChanged {
        name: "all_notifications_prefs".into(),
        value: serde_json::json!(value),
    };
    apply(&mut core, TEAM, 1, event.clone());
    assert!(!core.workspaces[TEAM].notifications.loaded);
    apply(&mut core, TEAM, 0, event);
    assert!(core.workspaces[TEAM].notifications.loaded);
    assert!(!core.workspaces[TEAM].notifications.desktop_enabled(CHANNEL));
    assert!(core.workspaces[TEAM].sidebar.is_muted("C1"));
    assert_eq!(
        core.workspaces[TEAM].sidebar.behavior,
        "hide_read_channels_unless_starred"
    );
}

#[test]
fn unread_group_ping_badges_once_and_ignores_stale_events() {
    let mut core = core();
    let workspace = core.workspaces.get_mut(TEAM).unwrap();
    workspace.usergroups.insert(
        "S1".into(),
        super_platinum_core::slack::models::UserGroup {
            id: "S1".into(),
            users: vec![workspace.self_user_id.clone()],
            ..Default::default()
        },
    );
    let mut ping = message("99.0", None);
    ping.user = Some("another-user".into());
    ping.text = Some("<!subteam^S1> hello".into());
    apply(&mut core, TEAM, 1, RtEvent::Message(ping.clone()));
    assert_eq!(core.workspaces[TEAM].messages[CHANNEL].mention_count, 0);
    apply(&mut core, TEAM, 0, RtEvent::Message(ping.clone()));
    apply(&mut core, TEAM, 0, RtEvent::Message(ping));
    assert_eq!(core.workspaces[TEAM].messages[CHANNEL].mention_count, 1);
}

#[test]
fn resource_panel_does_not_consume_new_channel_mentions() {
    let mut core = core();
    core.active_team = Some(TEAM.into());
    core.active_channel = Some(CHANNEL.into());
    let user = core.workspaces[TEAM].self_user_id.clone();
    let mut ping = message("99.0", None);
    ping.user = Some("another-user".into());
    ping.text = Some(format!("<@{user}> hello"));
    apply_in_view(&mut core, TEAM, 0, RtEvent::Message(ping), false);
    assert_eq!(core.workspaces[TEAM].messages[CHANNEL].mention_count, 1);
}

#[test]
fn background_and_history_readers_keep_incoming_mentions_unread() {
    for (focused, at_bottom, expected) in [(false, true, 1), (true, false, 1), (true, true, 0)] {
        let mut shell = crate::state::ShellState::fixture(crate::media::MediaRegistry::default());
        shell.core = core();
        shell.core.active_team = Some(TEAM.into());
        shell.core.active_channel = Some(CHANNEL.into());
        shell.core.active_account = Some("isolated-read-test".into());
        shell.core.pending_scroll_to = None;
        shell.stick_to_bottom = at_bottom;
        let workspace = shell.core.workspaces.get_mut(TEAM).unwrap();
        workspace.read_preferences = super_platinum_core::state::ReadPreferences::from_prefs(
            &super_platinum_core::state::ReadMode::LeftOffAndRead.patch(),
        );
        workspace.channels.insert(
            CHANNEL.into(),
            serde_json::from_value(serde_json::json!({"id":CHANNEL,"is_member":true})).unwrap(),
        );
        let bag = workspace.messages.get_mut(CHANNEL).unwrap();
        bag.loaded = true;
        bag.last_read = Some("0.0".into());
        bag.latest = Some(ROOT.into());
        bag.unread_count = 1;
        shell.unread_anchor = Some((CHANNEL.into(), "0.0".into()));
        crate::read_preferences::renderer::record_fixture_observation(
            &mut shell, at_bottom, at_bottom,
        );
        let user = shell.core.workspaces[TEAM].self_user_id.clone();
        let mut ping = message("99.0", None);
        ping.user = Some("another-user".into());
        ping.text = Some(format!("<@{user}> hello"));
        let reading = shell.reading_latest(focused);
        apply_in_view(&mut shell.core, TEAM, 0, RtEvent::Message(ping), reading);
        assert_eq!(
            shell.core.workspaces[TEAM].messages[CHANNEL].mention_count,
            expected
        );
    }
}

#[test]
fn a_reaction_reaches_the_thread_copy_of_a_message() {
    let mut core = core();
    apply(
        &mut core,
        TEAM,
        0,
        RtEvent::ReactionAdded {
            channel: CHANNEL.into(),
            ts: "2.0".into(),
            user: "U2".into(),
            reaction: "eyes".into(),
        },
    );
    assert_eq!(thread_reactions(&core, "2.0"), ["eyes"]);
}

#[test]
fn a_reaction_on_a_root_shows_in_both_the_channel_and_the_thread() {
    let mut core = core();
    apply(
        &mut core,
        TEAM,
        0,
        RtEvent::ReactionAdded {
            channel: CHANNEL.into(),
            ts: ROOT.into(),
            user: "U3".into(),
            reaction: "eyes".into(),
        },
    );
    assert_eq!(reactions(&core, ROOT), ["tada", "eyes"]);
    assert_eq!(thread_reactions(&core, ROOT), ["tada", "eyes"]);
    apply(
        &mut core,
        TEAM,
        0,
        RtEvent::ReactionRemoved {
            channel: CHANNEL.into(),
            ts: ROOT.into(),
            user: "U3".into(),
            reaction: "eyes".into(),
        },
    );
    assert_eq!(reactions(&core, ROOT), ["tada"]);
    assert_eq!(thread_reactions(&core, ROOT), ["tada"]);
}

#[test]
fn a_reaction_in_an_unloaded_channel_creates_no_transcript() {
    let mut core = core();
    apply(
        &mut core,
        TEAM,
        0,
        RtEvent::ReactionAdded {
            channel: "C-NEVER-OPENED".into(),
            ts: "9.0".into(),
            user: "U2".into(),
            reaction: "eyes".into(),
        },
    );
    assert!(
        !core.workspaces[TEAM]
            .messages
            .contains_key("C-NEVER-OPENED")
    );
}

#[test]
fn thread_read_events_are_socket_guarded_and_leave_channel_cursor_alone() {
    let mut core = core();
    core.workspaces.get_mut(TEAM).unwrap().rt_generation = 4;
    let key = (TEAM.into(), CHANNEL.into(), ROOT.into());
    for (generation, ts) in [(4, "1.999999"), (3, "2.0")] {
        apply(
            &mut core,
            TEAM,
            generation,
            RtEvent::ThreadMarked {
                channel: CHANNEL.into(),
                root_ts: ROOT.into(),
                last_read: ts.into(),
            },
        );
    }
    assert_eq!(core.thread_cursor(&key), Some("1.999999"));
    assert_eq!(core.threads[&key].unread_count, 1);
    assert!(core.workspaces[TEAM].read_cursor(CHANNEL).is_none());
    apply(
        &mut core,
        TEAM,
        4,
        RtEvent::ThreadMarked {
            channel: CHANNEL.into(),
            root_ts: ROOT.into(),
            last_read: "2.0".into(),
        },
    );
    assert_eq!(core.threads[&key].unread_count, 0);
    apply(
        &mut core,
        TEAM,
        4,
        RtEvent::Message(message("3.0", Some(ROOT))),
    );
    assert_eq!(core.threads[&key].unread_count, 1);
    assert_eq!(core.thread_cursor(&key), Some("2.0"));
}

#[test]
fn thread_subscription_events_update_all_copies_without_moving_read_cursors() {
    let mut core = core();
    core.active_team = Some(TEAM.into());
    core.workspaces.get_mut(TEAM).unwrap().rt_generation = 4;
    let key = (TEAM.into(), CHANNEL.into(), ROOT.into());
    core.apply_thread_mark(&key, "1.999999");
    let event = |active: bool| {
        super_platinum_core::slack::realtime::parse_event(&serde_json::json!({"type":if active {"thread_subscribed"} else {"thread_unsubscribed"},"subscription":{"channel":CHANNEL,"thread_ts":ROOT,"last_read":"2.0"}}).to_string()).unwrap()
    };
    apply(&mut core, TEAM, 3, event(false));
    assert_eq!(
        core.thread_subscriptions.active(core.account_epoch, &key),
        None
    );
    apply(&mut core, TEAM, 4, event(true));
    apply(&mut core, TEAM, 4, event(false));
    assert_eq!(
        core.thread_subscriptions.active(core.account_epoch, &key),
        Some(false)
    );
    assert_eq!(core.thread_cursor(&key), Some("1.999999"));
    assert_eq!(core.threads[&key].messages[0].extra["subscribed"], false);
    assert_eq!(
        core.workspaces[TEAM].messages[CHANNEL].messages[0].extra["subscribed"],
        false
    );
}

#[test]
fn file_deletions_respect_socket_generation_and_mask_late_file_projections() {
    let mut core = core();
    core.active_team = Some(TEAM.into());
    core.workspaces.get_mut(TEAM).unwrap().rt_generation = 4;
    let event = || {
        RtEvent::Unknown(
            serde_json::from_value(serde_json::json!({"type":"file_deleted","file_id":"F1"}))
                .unwrap(),
        )
    };
    apply(&mut core, TEAM, 3, event());
    assert!(!core.workspaces[TEAM].deleted_files.contains("F1"));
    apply(&mut core, TEAM, 4, event());
    assert!(core.workspaces[TEAM].deleted_files.contains("F1"));
    let file = serde_json::from_value(
        serde_json::json!({"id":"F1","title":"old","url_private":"https://files.slack.com/stale"}),
    )
    .unwrap();
    assert!(
        matches!(crate::message_vm::workspace_file_node(&core.workspaces[TEAM],&crate::media::MediaRegistry::default(),&file),crate::state::RichNode::Text(text) if text=="Deleted file")
    );
}

#[test]
fn realtime_worker_rejects_credentials_replaced_before_first_poll() {
    use std::sync::Arc;
    use super_platinum_core::{
        config::{Session, WorkspaceSession},
        slack::Transport,
    };
    let mut core = core();
    core.transport = Some(Arc::new(Transport::new("fixture-cookie").unwrap()));
    core.session = Some(Session {
        d_cookie: "fixture-cookie".into(),
        workspaces: [(
            TEAM.into(),
            WorkspaceSession {
                team_id: TEAM.into(),
                enterprise_id: None,
                user_id: "U1".into(),
                name: "Fixture".into(),
                url: "https://example.invalid".into(),
                token: "fixture-token".into(),
            },
        )]
        .into(),
    });
    let mut params = ConnectParams {
        team: TEAM.into(),
        ws_url: realtime::flannel_url("fixture-token", TEAM),
        d_cookie: "fixture-cookie".into(),
        user_agent: String::new(),
    };
    assert!(worker_source(&core, &params).is_some());
    params.ws_url = realtime::flannel_url("old-token", TEAM);
    assert!(worker_source(&core, &params).is_none());
    params.ws_url = realtime::flannel_url("fixture-token", TEAM);
    params.d_cookie = "old-cookie".into();
    assert!(worker_source(&core, &params).is_none());
}
