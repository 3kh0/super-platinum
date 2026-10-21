use super::*;
use crate::media::MediaRegistry;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::Transport,
    state::Workspace,
};
const TEAM: &str = "TGROUP001";
const GROUP: &str = "SGROUP001";
fn shell() -> ShellState {
    let mut shell = ShellState::login(MediaRegistry::default());
    let session = WorkspaceSession {
        team_id: TEAM.into(),
        user_id: "USELF0001".into(),
        enterprise_id: None,
        name: "Closed source".into(),
        url: "https://example.invalid".into(),
        token: "synthetic-token".into(),
    };
    shell.core.active_account = Some("group-source".into());
    shell.core.active_team = Some(TEAM.into());
    shell.core.session = Some(Session {
        d_cookie: "synthetic-cookie".into(),
        unified_user_client_enabled: BTreeMap::new(),
        workspaces: BTreeMap::from([(TEAM.into(), session.clone())]),
    });
    shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap()));
    shell
        .core
        .workspaces
        .insert(TEAM.into(), Workspace::from_session(&session));
    shell
}
fn observed() -> Observation {
    Observation {
        slow_connection_count: Some(1),
        major_version: Some(1),
        enabled: true,
    }
}
fn wake(shell: &mut ShellState) -> Wake {
    let owner = Arc::new(Context::capture(shell, TEAM).unwrap());
    ensure(shell, owner.clone(), observed()).unwrap();
    let slot = shell.group_freshness.slots.get_mut(&key(&owner)).unwrap();
    let token = Arc::new(());
    slot.worker = Some(token.clone());
    Wake {
        owner,
        identity: slot.identity.clone(),
        worker: token,
    }
}
#[test]
fn account_worker_survives_socket_but_original_read_does_not() {
    let mut shell = shell();
    let wake = wake(&mut shell);
    let original =
        super::super::ownership::GroupRead::capture(&shell, wake.owner.clone(), &[GROUP.into()])
            .unwrap();
    shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1;
    assert!(wake.current(&shell));
    assert!(!original.current(&shell));
    let next = Arc::new(Context::capture(&shell, TEAM).unwrap());
    assert!(next.current(&shell));
    ensure(&mut shell, next, observed()).unwrap();
    assert!(wake.current(&shell), "same account retains one runtime");
}
#[test]
fn replacement_account_cookie_transport_and_worker_cannot_complete_original() {
    for change in 0..5 {
        let mut shell = shell();
        let wake = wake(&mut shell);
        match change {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.active_account = Some("replacement".into()),
            2 => shell.core.session.as_mut().unwrap().d_cookie = "replacement".into(),
            3 => shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap())),
            _ => {
                shell
                    .group_freshness
                    .slots
                    .get_mut(&key(&wake.owner))
                    .unwrap()
                    .worker = Some(Arc::new(()))
            }
        }
        assert!(!wake.current(&shell), "change {change}");
    }
}
#[test]
fn typed_inventory_does_not_seed_freshness_and_snapshot_does_not_reset_it() {
    let mut shell = shell();
    let wake = wake(&mut shell);
    shell
        .core
        .workspaces
        .get_mut(TEAM)
        .unwrap()
        .usergroups
        .insert(
            GROUP.into(),
            super_platinum_core::slack::models::UserGroup::from_observed(
                &json!({"id":GROUP,"handle":"retained"}),
            )
            .unwrap(),
        );
    let before = snapshot_inner(&shell, TEAM);
    assert_eq!(before["freshIds"], json!([]));
    assert_eq!(before["started"], 0);
    let slot = shell
        .group_freshness
        .slots
        .get_mut(&key(&wake.owner))
        .unwrap();
    slot.manager
        .mark_fresh(
            &slot.scope,
            Clock::new(0).unwrap(),
            observed(),
            &[GROUP.into()],
        )
        .unwrap();
    let stamped = snapshot_inner(&shell, TEAM);
    assert_eq!(stamped["freshIds"], json!([GROUP]));
    shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1;
    assert_eq!(
        snapshot_inner(&shell, TEAM),
        stamped,
        "passive evidence must not infer reset from socket generation"
    );
}
