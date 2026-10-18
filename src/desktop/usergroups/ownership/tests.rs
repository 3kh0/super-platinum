use super::*;
use crate::{media::MediaRegistry, usergroups::hydration::Progress};
use serde_json::json;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::{Error, Transport},
    state::Workspace,
};

const TEAM: &str = "TGROUP001";
const GROUP: &str = "SGROUP001";
const MEMBER: &str = "UMEMBER01";
fn shell() -> ShellState {
    let mut shell = ShellState::login(MediaRegistry::default());
    let owner = WorkspaceSession {
        team_id: TEAM.into(),
        user_id: "USELF0001".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.invalid".into(),
        token: "synthetic-token".into(),
    };
    shell.core.active_account = Some("A".into());
    shell.core.active_team = Some(TEAM.into());
    shell.core.session = Some(Session {
        d_cookie: "synthetic-cookie".into(),
        unified_user_client_enabled: BTreeMap::new(),
        workspaces: BTreeMap::from([(TEAM.into(), owner.clone())]),
    });
    shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap()));
    let mut workspace = Workspace::from_session(&owner);
    workspace.usergroups.insert(GROUP.into(), group("original"));
    workspace.users.insert(
        MEMBER.into(),
        User {
            id: MEMBER.into(),
            name: Some("Original".into()),
            ..Default::default()
        },
    );
    shell.core.workspaces.insert(TEAM.into(), workspace);
    shell
}
fn group(handle: &str) -> UserGroup {
    UserGroup {
        id: GROUP.into(),
        team_id: TEAM.into(),
        handle: handle.into(),
        users: vec![MEMBER.into()],
        members_loaded: true,
        ..Default::default()
    }
}
fn request(shell: &ShellState) -> GroupRead {
    GroupRead::capture(
        shell,
        Arc::new(Context::capture(shell, TEAM).unwrap()),
        &[GROUP.into()],
    )
    .unwrap()
}
fn panel(shell: &mut ShellState) -> Arc<()> {
    let attempt = Arc::new(());
    shell.group_panel = Some(GroupPanel {
        id: GROUP.into(),
        attempt: attempt.clone(),
        loading: true,
        error: false,
        channels: false,
    });
    attempt
}

#[test]
fn original_owner_rejects_same_team_account_epoch_auth_transport_and_socket_changes() {
    for kind in 0..7 {
        let mut shell = shell();
        let request = request(&shell);
        let attempt = panel(&mut shell);
        match kind {
            0 => shell.core.active_account = Some("replacement".into()),
            1 => shell.core.account_epoch += 1,
            2 => shell.core.session.as_mut().unwrap().d_cookie = "replacement-cookie".into(),
            3 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(TEAM)
                    .unwrap()
                    .token = "replacement-token".into()
            }
            4 => shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap())),
            5 => shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1,
            _ => shell
                .core
                .session
                .as_mut()
                .unwrap()
                .unified_user_client_enabled
                .insert(TEAM.into(), true)
                .map(|_| ())
                .unwrap_or(()),
        }
        let before = serde_json::to_value(&shell.core.workspaces[TEAM].usergroups).unwrap();
        let mut progress = Progress::default();
        assert!(!progress.receive(&mut shell, &request, Ok(vec![group("foreign")])));
        assert!(!progress.should_persist());
        assert_eq!(
            serde_json::to_value(&shell.core.workspaces[TEAM].usergroups).unwrap(),
            before
        );
        assert!(!finish(&mut shell, &request.owner, GROUP, &attempt, true));
        assert!(shell.group_panel.as_ref().unwrap().loading);
    }
}

#[test]
fn close_and_same_id_reopen_retire_old_panel_completion() {
    let mut shell = shell();
    let owner = Context::capture(&shell, TEAM).unwrap();
    let first = panel(&mut shell);
    shell.group_panel = None;
    assert!(!finish(&mut shell, &owner, GROUP, &first, true));
    let second = panel(&mut shell);
    assert!(!finish(&mut shell, &owner, GROUP, &first, false));
    assert!(shell.group_panel.as_ref().unwrap().loading);
    assert!(finish(&mut shell, &owner, GROUP, &second, true));
    assert!(!shell.group_panel.as_ref().unwrap().loading);
    assert!(!shell.group_panel.as_ref().unwrap().error);
}

#[test]
fn metadata_membership_and_missing_to_present_changes_refuse_delayed_group_response() {
    for kind in 0..4 {
        let mut shell = shell();
        if kind == 3 {
            shell
                .core
                .workspaces
                .get_mut(TEAM)
                .unwrap()
                .usergroups
                .remove(GROUP);
        }
        let request = request(&shell);
        let workspace = shell.core.workspaces.get_mut(TEAM).unwrap();
        match kind {
            0 => workspace.usergroups.get_mut(GROUP).unwrap().handle = "socket-newer".into(),
            1 => workspace
                .usergroups
                .get_mut(GROUP)
                .unwrap()
                .users
                .push("USOCKET01".into()),
            2 => workspace.usergroups.get_mut(GROUP).unwrap().members_loaded = false,
            _ => {
                workspace.usergroups.insert(GROUP.into(), group("arrived"));
            }
        }
        let before = serde_json::to_value(&workspace.usergroups).unwrap();
        let mut progress = Progress::default();
        assert!(!progress.receive(&mut shell, &request, Ok(vec![group("delayed")])));
        assert_eq!(
            serde_json::to_value(&shell.core.workspaces[TEAM].usergroups).unwrap(),
            before
        );
    }
}

#[test]
fn successful_batches_preserve_membership_and_empty_response_never_reprojects() {
    let mut shell = shell();
    let first = request(&shell);
    let mut progress = Progress::default();
    let empty_projection = shell.channels.as_ptr();
    assert!(progress.receive(&mut shell, &first, Ok(vec![])));
    assert_eq!(shell.channels.as_ptr(), empty_projection);
    assert!(!progress.should_persist());
    let mut incoming = group("resolved");
    incoming.users = vec!["not-authoritative".into()];
    incoming.members_loaded = false;
    assert!(progress.receive(&mut shell, &first, Ok(vec![incoming])));
    assert_eq!(
        shell.core.workspaces[TEAM].usergroups[GROUP].users,
        [MEMBER]
    );
    assert!(shell.core.workspaces[TEAM].usergroups[GROUP].members_loaded);
    assert!(progress.should_persist());
    let next = request(&shell);
    assert!(!progress.receive(&mut shell, &next, Err(Error::Offline("synthetic".into()))));
    assert!(!progress.should_persist());
    assert_eq!(
        shell.core.workspaces[TEAM].usergroups[GROUP].handle,
        "resolved"
    );
}

#[test]
fn unrequested_groups_and_newer_member_rows_are_not_admitted() {
    let mut shell = shell();
    let request = request(&shell);
    let mut foreign = group("foreign");
    foreign.id = "SOTHER001".into();
    assert!(!request.accepts(&[foreign]));
    let users = UsersRead::capture(&shell, request, &[MEMBER.into()]).unwrap();
    assert!(users.current(&shell));
    assert!(!users.accepts(&[User {
        id: "UOTHER001".into(),
        ..Default::default()
    }]));
    shell
        .core
        .workspaces
        .get_mut(TEAM)
        .unwrap()
        .users
        .get_mut(MEMBER)
        .unwrap()
        .name = Some("socket-newer".into());
    assert!(!users.current(&shell));
}

#[test]
fn unrelated_group_changes_do_not_retire_requested_witness() {
    let mut shell = shell();
    let request = request(&shell);
    let mut other = group("other");
    other.id = "SOTHER001".into();
    shell
        .core
        .workspaces
        .get_mut(TEAM)
        .unwrap()
        .usergroups
        .insert(other.id.clone(), other);
    assert!(request.current(&shell));
    // Full serialized values, including subsequently added source metadata, bind the request.
    assert_eq!(
        request.before[GROUP].as_ref().unwrap()["handle"],
        json!("original")
    );
}
