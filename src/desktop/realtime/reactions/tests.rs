use super::*;
use crate::{media::MediaRegistry, model::MainView};
use super_platinum_core::{
    pins::{PinnedItem, PinsPage},
    slack::models::{ActivityItem, Channel, Message},
    state::ChannelMessages,
};

const TEAM: &str = "T1";
const CHANNEL: &str = "C2";
const ROOT: &str = "1719800400.000000";
const REPLY: &str = "1719800401.000000";

fn fixture() -> ShellState {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    let mut bag = crate::thread_vm::fixture_bag(8);
    bag.last_read = Some(ROOT.into());
    bag.unread_count = 8;
    bag.mention_count = 2;
    let workspace = shell.core.workspaces.get_mut(TEAM).unwrap();
    workspace.messages.insert(CHANNEL.into(), bag.clone());
    workspace.sidebar.channel_sort = "recent".into();
    shell.core.active_channel = Some(CHANNEL.into());
    shell.core.pending_scroll_to = None;
    shell.unread_anchor = Some((CHANNEL.into(), ROOT.into()));
    shell.thread_root = Some(ROOT.into());
    shell
        .core
        .threads
        .insert((TEAM.into(), CHANNEL.into(), ROOT.into()), bag.clone());
    shell.core.pins.ensure_scope(shell.core.account_epoch, TEAM);
    let generation = shell.core.pins.begin(CHANNEL);
    shell.core.pins.apply(
        generation,
        PinsPage {
            items: vec![PinnedItem {
                kind: "message".into(),
                message: Some(bag.messages[0].clone()),
                ..Default::default()
            }],
        },
    );
    shell
        .core
        .activity
        .ensure_scope(shell.core.account_epoch, TEAM);
    shell.core.activity.upsert(ActivityItem {
        key: "reaction-activity".into(),
        is_unread: true,
        ..Default::default()
    });
    shell.core.activity.selected = Some("reaction-activity".into());
    shell.refresh_from_core();
    shell
}

fn event(ts: &str, added: bool) -> RtEvent {
    if added {
        RtEvent::ReactionAdded {
            channel: CHANNEL.into(),
            ts: ts.into(),
            user: "U1".into(),
            reaction: "wave".into(),
        }
    } else {
        RtEvent::ReactionRemoved {
            channel: CHANNEL.into(),
            ts: ts.into(),
            user: "U1".into(),
            reaction: "wave".into(),
        }
    }
}

fn apply(shell: &mut ShellState, team: &str, generation: u64, ts: &str, added: bool) {
    let event = event(ts, added);
    let channel = channel(&event).unwrap().to_owned();
    super::super::apply_in_view(&mut shell.core, team, generation, event, true);
    refresh(shell, team, generation, &channel);
}

#[test]
fn reactions_match_full_projection_on_home_dm_and_activity_without_rebuilding_sidebar() {
    for surface in [MainView::Home, MainView::Dms, MainView::Activity] {
        let mut shell = fixture();
        shell.main_view = surface;
        shell.remember_surface();
        let channels = shell.channels.clone();
        let sections = shell.sidebar_sections.clone();
        let rows = shell.channel_rows.clone();
        let bounds = (shell.timeline_start, shell.timeline_end);
        let activity = serde_json::to_value(&shell.core.activity.items).unwrap();
        let generation = shell.core.workspaces[TEAM].rt_generation;
        for (ts, added) in [(ROOT, true), (REPLY, true), (ROOT, false), (REPLY, false)] {
            let channel_allocation = shell.channels.as_ptr();
            let old_thread_generation = shell.thread_window.projection_generation;
            apply(&mut shell, TEAM, generation, ts, added);
            assert_ne!(
                shell.thread_window.projection_generation,
                old_thread_generation
            );
            assert_eq!(
                shell.channels.as_ptr(),
                channel_allocation,
                "sidebar allocation must be untouched"
            );
            assert_eq!(shell.channels, channels);
            assert_eq!(shell.sidebar_sections, sections);
            assert_eq!(shell.channel_rows, rows);
            assert_eq!((shell.timeline_start, shell.timeline_end), bounds);
            for messages in [&shell.messages, &shell.thread_messages] {
                let row = messages.iter().find(|m| m.ts == ts).unwrap();
                assert_eq!(row.reactions.iter().any(|r| r.name == "wave"), added);
                assert!(
                    messages
                        .iter()
                        .any(|m| m.ts == REPLY && m.show_unread_divider)
                );
            }
            assert_eq!(
                shell.core.workspaces[TEAM].messages[CHANNEL].unread_count,
                8
            );
            assert_eq!(
                shell.core.workspaces[TEAM].messages[CHANNEL].mention_count,
                2
            );
            assert_eq!(
                serde_json::to_value(&shell.core.activity.items).unwrap(),
                activity
            );
            assert_eq!(
                shell.core.activity.selected.as_deref(),
                Some("reaction-activity")
            );
            let messages = shell.messages.clone();
            let thread = shell.thread_messages.clone();
            // Compare on the same media registry: opaque media IDs from separate
            // fixtures are intentionally different even for equivalent content.
            shell.refresh_from_core();
            assert_eq!(shell.messages, messages);
            assert_eq!(shell.thread_messages, thread);
            assert_eq!(shell.channels, channels);
        }
    }
}

#[test]
fn add_remove_preserve_pins_and_thread_reactions() {
    let mut shell = fixture();
    let generation = shell.core.workspaces[TEAM].rt_generation;
    for added in [true, false] {
        let channels = shell.channels.as_ptr();
        apply(&mut shell, TEAM, generation, ROOT, added);
        assert_eq!(shell.channels.as_ptr(), channels);
        let pinned = shell.core.pins.items[&format!("message:{ROOT}")]
            .message
            .as_ref()
            .unwrap();
        assert_eq!(pinned.reactions.iter().any(|r| r.name == "wave"), added);
        for rows in [&shell.messages, &shell.thread_messages] {
            assert_eq!(
                rows.iter()
                    .find(|r| r.ts == ROOT)
                    .unwrap()
                    .reactions
                    .iter()
                    .any(|r| r.name == "wave"),
                added
            );
        }
    }
}

#[test]
fn stale_or_other_workspace_and_hidden_channel_do_not_reproject_visible_rows() {
    let mut shell = fixture();
    let generation = shell.core.workspaces[TEAM].rt_generation;
    let messages = shell.messages.clone();
    let thread_generation = shell.thread_window.projection_generation;
    apply(&mut shell, TEAM, generation + 1, ROOT, true);
    shell
        .core
        .workspaces
        .insert("OTHER".into(), shell.core.workspaces[TEAM].clone());
    apply(&mut shell, "OTHER", generation, ROOT, true);
    refresh(&mut shell, TEAM, generation, "HIDDEN");
    assert_eq!(shell.messages, messages);
    assert_eq!(shell.thread_window.projection_generation, thread_generation);
    assert!(
        shell.core.workspaces["OTHER"].messages[CHANNEL].messages[0]
            .reactions
            .iter()
            .any(|r| r.name == "wave")
    );
}

#[test]
fn source_index_drift_falls_back_to_canonical_projection() {
    let mut shell = fixture();
    let generation = shell.core.workspaces[TEAM].rt_generation;
    shell.channel_rows[0].source_index = usize::MAX;
    apply(&mut shell, TEAM, generation, ROOT, true);
    assert_eq!(shell.channel_rows[0].source_index, 0);
    assert_eq!(shell.messages.len(), 9);
    assert!(shell.messages[0].reactions.iter().any(|r| r.name == "wave"));
}

#[test]
fn selection_pinned_window_keeps_all_selected_rows_and_activity_events_remain_full_refresh() {
    let mut shell = fixture();
    shell
        .core
        .workspaces
        .get_mut(TEAM)
        .unwrap()
        .messages
        .insert(CHANNEL.into(), crate::thread_vm::fixture_bag(150));
    shell.refresh_from_core();
    shell.selection_pinned = true;
    shell.timeline_start = 0;
    shell.timeline_end = shell.channel_rows.len();
    shell.reproject_channel_window();
    let generation = shell.core.workspaces[TEAM].rt_generation;
    let channels = shell.channels.as_ptr();
    apply(&mut shell, TEAM, generation, ROOT, true);
    assert_eq!(shell.messages.len(), 151);
    assert_eq!(shell.channels.as_ptr(), channels);
    assert!(channel(&RtEvent::ActivityUpdated(ActivityItem::default())).is_none());
}

#[test]
#[ignore = "manual paired actual desktop reaction projection benchmark"]
fn reaction_projection_cost() {
    for count in [100, 1000] {
        let mut shell = fixture();
        let workspace = shell.core.workspaces.get_mut(TEAM).unwrap();
        for index in 0..count {
            let id = format!("CBENCH{index:05}");
            workspace.channels.insert(
                id.clone(),
                Channel {
                    id: id.clone(),
                    name: Some(id.clone()),
                    is_channel: true,
                    ..Default::default()
                },
            );
            workspace.messages.insert(
                id,
                ChannelMessages {
                    unread_count: 1,
                    messages: (0..200)
                        .map(|i| Message {
                            ts: Some(format!("170000{i:04}.000001")),
                            ..Default::default()
                        })
                        .collect(),
                    ..Default::default()
                },
            );
        }
        shell.refresh_from_core();
        let generation = shell.core.workspaces[TEAM].rt_generation;
        let mut times = [Vec::new(), Vec::new()];
        for round in 0..31 {
            // Alternate ordering; both paths receive the same add/remove event.
            let mut projected = None;
            for mode in if round % 2 == 0 { [0, 1] } else { [1, 0] } {
                // Reset canonical reaction outside timing so neither measured
                // reducer gets an already-applied duplicate of the other path.
                super::super::apply_in_view(
                    &mut shell.core,
                    TEAM,
                    generation,
                    event(ROOT, round % 2 != 0),
                    true,
                );
                let started = std::time::Instant::now();
                super::super::apply_in_view(
                    &mut shell.core,
                    TEAM,
                    generation,
                    event(ROOT, round % 2 == 0),
                    true,
                );
                if mode == 0 {
                    shell.refresh_from_core();
                } else {
                    refresh(&mut shell, TEAM, generation, CHANNEL);
                }
                if round > 0 {
                    times[mode].push(started.elapsed().as_secs_f64() * 1000.0);
                }
                let actual = (&shell.messages, &shell.thread_messages);
                if let Some((messages, thread)) = &projected {
                    assert_eq!(actual, (messages, thread));
                } else {
                    projected = Some((shell.messages.clone(), shell.thread_messages.clone()));
                }
                std::hint::black_box(actual);
            }
        }
        for (mode, mut samples) in times.into_iter().enumerate() {
            samples.sort_by(f64::total_cmp);
            println!(
                "reaction_projection incremental={} additional_channels={count} additional_messages={} runs=30 median_ms={:.4} p95_ms={:.4}",
                mode == 1,
                count * 200,
                (samples[14] + samples[15]) / 2.0,
                samples[28]
            );
        }
    }
}
