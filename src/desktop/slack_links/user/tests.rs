use super::*;
use crate::state::{ChannelOpen, MainView, Overlay};
use std::sync::Arc;
use super_platinum_core::{
    ComposerState,
    config::{Session, WorkspaceSession},
    slack::{Transport, models::User},
};

fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    let session = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: shell.core.workspaces["T1"].self_user_id.clone(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic-only".into(),
    };
    shell.core.session = Some(Session {
        d_cookie: "synthetic-only".into(),
        workspaces: [("T1".into(), session)].into(),
    });
    shell.core.workspaces.get_mut("T1").unwrap().users.insert(
        "ULINK".into(),
        User {
            id: "ULINK".into(),
            name: Some("Link recipient".into()),
            ..Default::default()
        },
    );
    shell
}

fn begin(shell: &mut ShellState) -> Intent {
    Intent::begin(shell, "T1".into(), "ULINK".into())
        .unwrap()
        .unwrap()
}

fn channel() -> Channel {
    Channel {
        id: "DLINK".into(),
        is_im: true,
        user: Some("ULINK".into()),
        ..Default::default()
    }
}

#[test]
fn unknown_bot_app_and_deleted_users_are_refused_without_navigation() {
    for change in 0..6 {
        let mut shell = shell();
        let before = (
            shell.core.active_team.clone(),
            shell.core.active_channel.clone(),
            shell.slack_link_generation,
            shell.profile_dm_generation,
        );
        let member = shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .users
            .get_mut("ULINK")
            .unwrap();
        match change {
            0 => member.is_bot = true,
            1 => member.deleted = true,
            2 => {
                member.extra.insert("is_app_user".into(), true.into());
            }
            3 => {
                member.extra.insert("is_app_user".into(), "false".into());
            }
            4 => member.id = "UOTHER".into(),
            _ => {
                shell
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .users
                    .remove("ULINK");
            }
        }
        assert!(Intent::begin(&mut shell, "T1".into(), "ULINK".into()).is_err());
        assert_eq!(
            before,
            (
                shell.core.active_team.clone(),
                shell.core.active_channel.clone(),
                shell.slack_link_generation,
                shell.profile_dm_generation,
            )
        );
    }
}

#[test]
fn cached_and_created_routes_retain_exact_profile_and_outgoing_draft() {
    for cached in [false, true] {
        let mut shell = shell();
        let original = shell.core.active_channel.clone().unwrap();
        shell.core.composer = ComposerState::with_text("Unsent Unicode 日本語");
        if cached {
            shell
                .core
                .workspaces
                .get_mut("T1")
                .unwrap()
                .channels
                .insert("DLINK".into(), channel());
        }
        let intent = begin(&mut shell);
        assert_eq!(intent.cached(&shell).unwrap().is_some(), cached);
        assert!(intent.commit(&mut shell, channel()).unwrap());
        assert_eq!(shell.core.active_channel.as_deref(), Some("DLINK"));
        assert_eq!(shell.main_view, MainView::Home);
        assert_eq!(shell.profile_user.as_deref(), Some("ULINK"));
        assert_eq!(shell.core.profile_pane.as_ref().unwrap().user, "ULINK");
        assert!(shell.profile_dm_opening.is_none());
        assert!(shell.core.composer.text.is_empty());
        let index = shell
            .channels
            .iter()
            .position(|row| row.id == original)
            .unwrap();
        shell.select_channel(index, ChannelOpen::Global);
        assert_eq!(shell.core.composer.text, "Unsent Unicode 日本語");
    }
}

#[test]
fn stale_completions_never_insert_metadata_or_replace_navigation() {
    let mutations: [fn(&mut ShellState); 17] = [
        |s| s.core.active_account = Some("replacement-account".into()),
        |s| s.core.account_epoch += 1,
        |s| s.core.transport = Some(Arc::new(Transport::new("synthetic-only").unwrap())),
        |s| {
            s.core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("T1")
                .unwrap()
                .token = "changed".into()
        },
        |s| {
            s.core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("T1")
                .unwrap()
                .url = "https://different.slack.com".into()
        },
        |s| {
            s.core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("T1")
                .unwrap()
                .enterprise_id = Some("EOTHER".into())
        },
        |s| s.core.workspaces.get_mut("T1").unwrap().self_user_id = "OTHER".into(),
        |s| s.select_channel(s.active_channel, ChannelOpen::Global),
        |s| s.close_thread(),
        |s| s.close_profile(),
        |s| s.slack_link_generation += 1,
        |s| s.palette_generation += 1,
        |s| s.search_open_generation += 1,
        |s| s.shortcut_generation += 1,
        |s| s.overlay = Some(Overlay::AppModal),
        |s| {
            s.core
                .workspaces
                .get_mut("T1")
                .unwrap()
                .users
                .get_mut("ULINK")
                .unwrap()
                .is_bot = true
        },
        |s| s.main_view = MainView::Dms,
    ];
    for mutate in mutations {
        let mut shell = shell();
        let intent = begin(&mut shell);
        mutate(&mut shell);
        let active = shell.core.active_channel.clone();
        assert!(!intent.current(&shell));
        assert!(!intent.commit(&mut shell, channel()).unwrap());
        assert!(!shell.core.workspaces["T1"].channels.contains_key("DLINK"));
        assert_eq!(shell.core.active_channel, active);
    }
}

#[test]
fn duplicate_admission_and_old_cleanup_do_not_replace_current_owner() {
    let mut shell = shell();
    let old = begin(&mut shell);
    let generation = shell.slack_link_generation;
    assert!(
        Intent::begin(&mut shell, "T1".into(), "ULINK".into())
            .unwrap()
            .is_none()
    );
    assert_eq!(shell.slack_link_generation, generation);
    shell.cancel_profile_dm();
    let current = begin(&mut shell);
    old.release(&mut shell);
    assert!(current.current(&shell));
    assert!(!old.current(&shell));
    current.release(&mut shell);
    assert!(shell.profile_dm_opening.is_none());
}

#[test]
fn a_user_link_supersedes_the_profiles_message_intent_for_the_same_person() {
    let mut shell = shell();
    shell.cancel_profile_dm();
    shell.profile_dm_opening = Some("ULINK".into());
    let message_request = shell.profile_dm_generation;
    assert!(shell.profile_dm_link_generation.is_none());
    let link = begin(&mut shell);
    assert_ne!(shell.profile_dm_generation, message_request);
    assert_eq!(
        shell.profile_dm_link_generation,
        Some(shell.slack_link_generation)
    );
    assert!(link.commit(&mut shell, channel()).unwrap());
    assert_eq!(shell.profile_user.as_deref(), Some("ULINK"));
    assert!(shell.profile_dm_link_generation.is_none());
}

#[test]
fn wrong_recipient_channel_kind_and_cached_collisions_never_commit() {
    for change in 0..7 {
        let mut shell = shell();
        let intent = begin(&mut shell);
        let mut returned = channel();
        match change {
            0 => returned.user = Some("UOTHER".into()),
            1 => returned.user = None,
            2 => returned.is_im = false,
            3 => returned.is_mpim = true,
            4 => returned.is_archived = true,
            5 => returned.id = "CWRONG".into(),
            _ => {
                shell
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .channels
                    .insert(
                        "DLINK".into(),
                        Channel {
                            user: Some("UOTHER".into()),
                            ..channel()
                        },
                    );
            }
        }
        let active = shell.core.active_channel.clone();
        assert_eq!(intent.commit(&mut shell, returned), Err(WRONG_DM));
        assert_eq!(shell.core.active_channel, active);
        assert_ne!(shell.profile_user.as_deref(), Some("ULINK"));
    }
}

#[test]
fn duplicate_cached_membership_is_not_resolved_by_hashmap_iteration_order() {
    let mut shell = shell();
    let workspace = shell.core.workspaces.get_mut("T1").unwrap();
    workspace.channels.insert("DLINK".into(), channel());
    workspace.channels.insert(
        "DSECOND".into(),
        Channel {
            id: "DSECOND".into(),
            ..channel()
        },
    );
    let intent = begin(&mut shell);
    assert!(intent.cached(&shell).is_err());
}

#[test]
fn same_owner_rotation_releases_only_the_exact_pending_request() {
    for transport in [false, true] {
        let mut shell = shell();
        let intent = begin(&mut shell);
        if transport {
            shell.core.transport = Some(Arc::new(Transport::new("synthetic-only").unwrap()));
        } else {
            shell
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("T1")
                .unwrap()
                .token = "rotated".into();
        }
        assert!(!intent.current(&shell));
        intent.release(&mut shell);
        assert!(shell.profile_dm_opening.is_none());
        let newer = begin(&mut shell);
        intent.release(&mut shell);
        assert!(newer.current(&shell));
    }
}

#[test]
fn replaced_account_cannot_release_a_same_numbered_reservation() {
    let mut shell = shell();
    let old = begin(&mut shell);
    shell.core.active_account = Some("another-account".into());
    old.release(&mut shell);
    assert_eq!(shell.profile_dm_opening.as_deref(), Some("ULINK"));
}

#[test]
fn cross_workspace_open_preserves_both_drafts_and_opens_exact_profile() {
    let mut shell = shell();
    let mut workspace = shell.core.workspaces["T1"].clone();
    workspace.team_id = "T2".into();
    workspace.url = "https://second.slack.com".into();
    shell.core.workspaces.insert("T2".into(), workspace);
    let mut vm = shell
        .workspaces
        .iter()
        .find(|workspace| workspace.id == "T1")
        .unwrap()
        .clone();
    vm.id = "T2".into();
    shell.workspaces.push(vm);
    let sessions = &mut shell.core.session.as_mut().unwrap().workspaces;
    let mut session = sessions["T1"].clone();
    session.team_id = "T2".into();
    session.url = "https://second.slack.com".into();
    sessions.insert("T2".into(), session);
    let first_channel = shell.core.active_channel.clone().unwrap();
    shell.core.composer = ComposerState::with_text("Workspace one unsent");
    let second = shell.workspaces.iter().position(|w| w.id == "T2").unwrap();
    assert!(shell.select_workspace(second));
    let second_channel = shell.core.active_channel.clone().unwrap();
    shell.core.composer = ComposerState::with_text("Workspace two unsent");
    let intent = Intent::begin(&mut shell, "T1".into(), "ULINK".into())
        .unwrap()
        .unwrap();
    assert!(intent.commit(&mut shell, channel()).unwrap());
    assert_eq!(shell.core.active_team.as_deref(), Some("T1"));
    assert_eq!(shell.core.active_channel.as_deref(), Some("DLINK"));
    assert_eq!(shell.profile_user.as_deref(), Some("ULINK"));
    let original = shell
        .channels
        .iter()
        .position(|row| row.id == first_channel)
        .unwrap();
    shell.select_channel(original, ChannelOpen::Global);
    assert_eq!(shell.core.composer.text, "Workspace one unsent");
    assert!(shell.select_workspace(second));
    let original = shell
        .channels
        .iter()
        .position(|row| row.id == second_channel)
        .unwrap();
    shell.select_channel(original, ChannelOpen::Global);
    assert_eq!(shell.core.composer.text, "Workspace two unsent");
}
