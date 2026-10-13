use super::*;
use super_platinum_core::{config::Session, domain::PendingScrollTarget};

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
    shell
}

fn add_workspace(shell: &mut ShellState, id: &str, host: &str) {
    let mut workspace = shell.core.workspaces["T1"].clone();
    workspace.team_id = id.into();
    workspace.url = format!("https://{host}");
    shell.core.workspaces.insert(id.into(), workspace);
    let mut vm = shell
        .workspaces
        .iter()
        .find(|workspace| workspace.id == "T1")
        .unwrap()
        .clone();
    vm.id = id.into();
    shell.workspaces.push(vm);
    let sessions = &mut shell.core.session.as_mut().unwrap().workspaces;
    let mut session = sessions["T1"].clone();
    session.team_id = id.into();
    session.url = format!("https://{host}");
    sessions.insert(id.into(), session);
}

#[test]
fn ownership_requires_exact_signed_in_team_or_unique_host() {
    let mut shell = shell();
    assert_eq!(
        resolve_team(&shell, &WorkspaceRef::Host("fixture.slack.com".into())),
        Ok("T1".into())
    );
    assert!(resolve_team(&shell, &WorkspaceRef::Host("other.slack.com".into())).is_err());
    assert!(resolve_team(&shell, &WorkspaceRef::TeamId("TUNKNOWN".into())).is_err());
    add_workspace(&mut shell, "T2", "fixture.slack.com");
    assert!(resolve_team(&shell, &WorkspaceRef::Host("fixture.slack.com".into())).is_err());
    assert_eq!(
        resolve_team(&shell, &WorkspaceRef::TeamId("T2".into())),
        Ok("T2".into())
    );
    shell.core.workspaces.get_mut("T2").unwrap().self_user_id = "OTHER".into();
    assert!(resolve_team(&shell, &WorkspaceRef::TeamId("T2".into())).is_err());
    shell.core.session = None;
    assert!(resolve_team(&shell, &WorkspaceRef::TeamId("T1".into())).is_err());
}

#[test]
fn enterprise_session_resolves_own_canonical_permalink_without_ambiguous_aliases() {
    let mut shell = shell();
    shell
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T1")
        .unwrap()
        .url = "https://fixture.enterprise.slack.com".into();
    let link = "https://fixture.slack.com/archives/C1/p1719800000000100";
    assert_eq!(target_team(&shell, link), Ok(Some("T1".into())));
    assert_eq!(
        resolve_team(
            &shell,
            &WorkspaceRef::Host("fixture.enterprise.slack.com".into())
        ),
        Ok("T1".into())
    );
    // Different signed-in team records can share the canonical host. The host
    // alone is then insufficient; an explicit team ID remains unambiguous.
    add_workspace(&mut shell, "T2", "fixture.slack.com");
    assert!(target_team(&shell, link).is_err());
    assert!(
        resolve_team(
            &shell,
            &WorkspaceRef::Host("fixture.enterprise.slack.com".into())
        )
        .is_err()
    );
    assert_eq!(
        resolve_team(&shell, &WorkspaceRef::TeamId("T1".into())),
        Ok("T1".into())
    );
}

#[test]
fn button_classification_never_changes_workspace_and_rejects_unsupported_routes() {
    let mut shell = shell();
    add_workspace(&mut shell, "T2", "other.slack.com");
    assert!(button_url_allowed(&shell, "https://example.com/page").is_ok());
    assert!(button_url_allowed(&shell, "slack://channel?team=T1&id=C1").is_ok());
    assert!(button_url_allowed(&shell, "slack://channel?team=T2&id=C1").is_err());
    assert!(button_url_allowed(&shell, "slack://user?team=T1&id=U1").is_err());
    assert!(button_url_allowed(&shell, "slack://settings?team=T1").is_err());
    assert!(button_url_allowed(&shell, "file:///tmp/example").is_err());
    assert_eq!(shell.core.active_team.as_deref(), Some("T1"));
}

#[test]
fn delayed_resolution_rejects_account_transport_and_away_back_navigation() {
    for change in 0..13 {
        let mut shell = shell();
        let visit = Visit::begin(&mut shell, &WorkspaceRef::TeamId("T1".into()), false).unwrap();
        assert!(visit.current(&shell));
        match change {
            0 => {
                Visit::begin(&mut shell, &WorkspaceRef::TeamId("T1".into()), false).unwrap();
            }
            1 => {
                let index = shell.active_channel;
                shell.select_channel(index, ChannelOpen::Global);
            }
            2 => shell.core.account_epoch += 1,
            3 => {
                shell.core.transport = Some(Arc::new(Transport::new("synthetic-only").unwrap()));
            }
            4 => {
                shell.close_thread();
            }
            5 => shell.profile_generation += 1,
            6 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .token = "replacement".into()
            }
            7 => {
                add_workspace(&mut shell, "T2", "other.slack.com");
                let original = shell.active_workspace;
                let other = shell.workspaces.iter().position(|w| w.id == "T2").unwrap();
                shell.select_workspace(other);
                shell.select_workspace(original);
            }
            8 => shell.overlay = Some(Overlay::AppModal),
            9 => {
                shell.open_search(false);
                shell.dismiss_overlay();
            }
            10 => shell.palette_generation += 1,
            11 => shell.shortcut_generation += 1,
            _ => {
                shell.app_modal = Some(crate::app_modal::Session::fixture(serde_json::json!({
                    "id":"VLINK", "type":"modal", "title":{"type":"plain_text","text":"App"}, "blocks":[]
                })));
            }
        }
        assert!(!visit.current(&shell));
        assert!(!visit.resolve(
            &mut shell,
            "CNEW",
            Channel {
                id: "CNEW".into(),
                ..Default::default()
            },
            0
        ));
        assert!(!shell.core.workspaces["T1"].channels.contains_key("CNEW"));
    }
}

#[test]
fn failed_link_intent_preserves_foreground_history_and_paging_generation() {
    let mut shell = shell();
    shell.loading_older = true;
    let channel = shell.channel_generation;
    let thread = shell.thread_generation;
    let first = Visit::begin(&mut shell, &WorkspaceRef::TeamId("T1".into()), false).unwrap();
    let second = Visit::begin(&mut shell, &WorkspaceRef::TeamId("T1".into()), false).unwrap();
    assert!(!first.current(&shell));
    assert!(second.current(&shell));
    assert_eq!(shell.channel_generation, channel);
    assert_eq!(shell.thread_generation, thread);
    assert!(shell.loading_older);
    assert!(!second.resolve(
        &mut shell,
        "CNEW",
        Channel {
            id: "COTHER".into(),
            ..Default::default()
        },
        0
    ));
    assert_eq!(shell.channel_generation, channel);
    assert!(shell.loading_older);
}

#[test]
fn channel_resolution_validates_identity_and_message_target_reuses_global_navigation() {
    let mut shell = shell();
    let visit = Visit::begin(&mut shell, &WorkspaceRef::TeamId("T1".into()), false).unwrap();
    let revision = shell.core.workspaces["T1"].channel_lifecycle.revision();
    assert!(!visit.resolve(
        &mut shell,
        "CNEW",
        Channel {
            id: "COTHER".into(),
            ..Default::default()
        },
        revision
    ));
    assert!(visit.resolve(
        &mut shell,
        "CNEW",
        Channel {
            id: "CNEW".into(),
            name: Some("archived".into()),
            is_archived: true,
            ..Default::default()
        },
        revision
    ));
    assert!(select_target(&mut shell, "CNEW", Some("1234567890.123456")));
    assert_eq!(shell.core.active_channel.as_deref(), Some("CNEW"));
    assert_eq!(shell.main_view, MainView::Home);
    assert!(
        matches!(shell.core.pending_scroll_to.as_ref(), Some((channel, PendingScrollTarget::Message(ts))) if channel == "CNEW" && ts == "1234567890.123456")
    );
    assert!(!visit.current(&shell));
    assert!(select_target(&mut shell, "CNEW", None));
    assert!(!matches!(
        shell.core.pending_scroll_to.as_ref(),
        Some((_, PendingScrollTarget::Message(_)))
    ));
}
