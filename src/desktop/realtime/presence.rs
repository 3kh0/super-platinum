//! Presence does not change conversation membership, ordering or message bodies.
//! Keep the projected dots current without rebuilding those unrelated surfaces.
use crate::{model::PresenceVm, state::ShellState};

pub(super) fn refresh(shell: &mut ShellState, team: &str) {
    if shell.core.active_team.as_deref() != Some(team) {
        return;
    }
    let Some(workspace) = shell.core.workspaces.get(team) else {
        return;
    };
    shell.self_account.presence = PresenceVm::from_core(workspace.self_presence());
    shell.self_account.snoozed = workspace.self_snoozed();
    for row in &mut shell.channels {
        if let Some(channel) = workspace.channels.get(&row.id) {
            row.presence = PresenceVm::from_core(workspace.presence_for_channel(channel));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::MediaRegistry;
    use super_platinum_core::{slack::events::RtEvent, slack::models::Channel};

    fn fixture(count: usize) -> ShellState {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        let workspace = shell.core.workspaces.get_mut("T1").unwrap();
        workspace.set_presence("U0".into(), super_platinum_core::state::Presence::Away);
        for index in 0..count {
            let id = format!("D{index:08}");
            workspace.channels.insert(
                id.clone(),
                Channel {
                    id,
                    is_im: true,
                    user: Some(format!("U{index:08}")),
                    ..Default::default()
                },
            );
        }
        shell.refresh_from_core();
        shell
    }

    fn apply(shell: &mut ShellState, team: &str, generation: u64) {
        super::super::apply_in_view(
            &mut shell.core,
            team,
            generation,
            RtEvent::PresenceChange {
                users: vec!["U00000000".into(), "U00000001".into(), "U0".into()],
                presence: "active".into(),
            },
            true,
        );
        refresh(shell, team);
    }

    #[test]
    fn presence_matches_full_projection_without_replacing_messages_or_geometry() {
        let mut shell = fixture(40);
        let messages = shell.messages.clone();
        let rows = shell.channel_rows.clone();
        let sections = shell.sidebar_sections.clone();
        let bounds = (shell.timeline_start, shell.timeline_end);
        let generation = shell.core.workspaces["T1"].rt_generation;
        apply(&mut shell, "T1", generation);
        let channels = shell.channels.clone();
        let account = shell.self_account.clone();
        assert_eq!(account.presence, PresenceVm::Active);
        assert_eq!(shell.messages, messages);
        assert_eq!(shell.channel_rows, rows);
        assert_eq!(shell.sidebar_sections, sections);
        assert_eq!((shell.timeline_start, shell.timeline_end), bounds);
        assert_eq!(
            channels
                .iter()
                .find(|c| c.id == "D00000000")
                .unwrap()
                .presence,
            PresenceVm::Active
        );
        shell.refresh_from_core();
        assert_eq!(shell.channels, channels);
        assert_eq!(shell.self_account, account);
    }

    #[test]
    fn stale_and_other_workspace_frames_do_not_change_visible_presence() {
        let mut shell = fixture(3);
        let channels = shell.channels.clone();
        let generation = shell.core.workspaces["T1"].rt_generation;
        apply(&mut shell, "T1", generation + 1);
        assert_eq!(shell.channels, channels);
        shell
            .core
            .workspaces
            .insert("OTHER".into(), shell.core.workspaces["T1"].clone());
        apply(&mut shell, "OTHER", generation);
        assert_eq!(shell.channels, channels);
        assert!(
            shell.core.workspaces["OTHER"]
                .presence
                .contains_key("U00000000")
        );
    }

    #[test]
    #[ignore = "manual paired presence projection benchmark"]
    fn presence_projection_cost() {
        let mut shell = fixture(2000);
        for incremental in [false, true] {
            let mut samples = Vec::new();
            for round in 0..101 {
                let started = std::time::Instant::now();
                if incremental {
                    refresh(&mut shell, "T1");
                } else {
                    shell.refresh_from_core();
                }
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64() * 1000.0);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "presence_projection incremental={incremental} channels={} median_ms={:.4} p95_ms={:.4}",
                shell.channels.len(),
                (samples[49] + samples[50]) / 2.0,
                samples[94]
            );
        }
    }
}
