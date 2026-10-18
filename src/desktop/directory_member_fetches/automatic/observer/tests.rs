use super::*;
use serde_json::json;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::Transport,
    state::Workspace,
};

const TEAM: &str = "T00000001";
const MEMBER: &str = "U00000001";
fn ids() -> Vec<String> {
    vec![MEMBER.into()]
}
fn observation() -> model::Observation {
    model::Observation {
        connection_count: 1,
        slow_connection_count: 1,
        major_version: 1,
    }
}
fn shell() -> ShellState {
    let mut shell = ShellState::login(crate::media::MediaRegistry::default());
    let session = WorkspaceSession {
        team_id: TEAM.into(),
        user_id: "U00000999".into(),
        name: "Synthetic".into(),
        url: "https://example.invalid".into(),
        token: "synthetic-token".into(),
        enterprise_id: None,
    };
    shell.core.active_account = Some("synthetic-account".into());
    shell.core.active_team = Some(TEAM.into());
    shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap()));
    shell.core.workspaces = BTreeMap::from([(TEAM.into(), Workspace::from_session(&session))]);
    shell.core.session = Some(Session {
        d_cookie: "synthetic-cookie".into(),
        unified_user_client_enabled: BTreeMap::new(),
        workspaces: BTreeMap::from([(TEAM.into(), session)]),
    });
    mode::Boot::capture(&json!({}))
        .accepted(&mut shell, TEAM)
        .unwrap();
    shell
}
fn prepared() -> (ShellState, Observer) {
    let mut shell = shell();
    let context = Arc::new(Context::capture(&shell, TEAM).unwrap());
    ensure(&mut shell, context.clone()).unwrap();
    let runtime = shell
        .directory_member_fetches
        .automatic
        .slots
        .get_mut(&context.key())
        .unwrap();
    runtime
        .manager
        .hit(&runtime.scope, 10_000, observation(), MEMBER, false)
        .unwrap();
    let observer = Observer::capture(&shell, TEAM).unwrap().unwrap();
    (shell, observer)
}

#[test]
fn pending_without_worker_survives_socket_generation_but_operation_context_does_not() {
    let (mut shell, observer) = prepared();
    assert!(
        shell.directory_member_fetches.automatic.slots[&observer.context.key()]
            .worker
            .is_none()
    );
    assert!(observer.pending(&shell, &ids()).unwrap());
    shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1;
    assert!(!observer.context.current(&shell));
    assert!(observer.pending(&shell, &ids()).unwrap());
    assert!(!observer.pending(&shell, &["U00000002".into()]).unwrap());
}

#[test]
fn retired_cycle_returns_to_pending_until_successor_finishes() {
    let (mut shell, observer) = prepared();
    let runtime = shell
        .directory_member_fetches
        .automatic
        .slots
        .get_mut(&observer.context.key())
        .unwrap();
    let cycle = runtime
        .manager
        .begin_due(&runtime.scope, 10_000, observation())
        .unwrap()
        .unwrap();
    let old = runtime
        .manager
        .next_operation(&runtime.scope, &cycle, 10_000, 0.0)
        .unwrap()
        .unwrap();
    runtime
        .manager
        .started(&runtime.scope, &old, 10_000)
        .unwrap();
    runtime
        .manager
        .return_unsent(&runtime.scope, &cycle)
        .unwrap();
    assert!(observer.pending(&shell, &ids()).unwrap());
    shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1;
    let runtime = shell
        .directory_member_fetches
        .automatic
        .slots
        .get_mut(&observer.context.key())
        .unwrap();
    let next = model::Observation {
        connection_count: 2,
        slow_connection_count: 2,
        major_version: 1,
    };
    runtime
        .manager
        .observe(&runtime.scope, 12_000, next)
        .unwrap();
    assert!(!runtime.manager.operation_current(&runtime.scope, &old));
    let cycle = runtime
        .manager
        .begin_due(&runtime.scope, 12_000, next)
        .unwrap()
        .unwrap();
    let op = runtime
        .manager
        .next_operation(&runtime.scope, &cycle, 12_000, 0.0)
        .unwrap()
        .unwrap();
    runtime
        .manager
        .started(&runtime.scope, &op, 12_000)
        .unwrap();
    runtime
        .manager
        .settle(&runtime.scope, &op, 12_001, model::Outcome::Success)
        .unwrap();
    assert!(
        runtime
            .manager
            .next_operation(&runtime.scope, &cycle, 12_001, 0.0)
            .unwrap()
            .is_none()
    );
    runtime
        .manager
        .finish_parent(
            &runtime.scope,
            &cycle,
            12_001,
            model::ParentOutcome::Completed,
        )
        .unwrap();
    assert!(!observer.pending(&shell, &ids()).unwrap());
}

#[test]
fn same_key_replacement_or_removed_registry_cannot_adopt_old_observer() {
    let (mut shell, observer) = prepared();
    shell
        .directory_member_fetches
        .automatic
        .slots
        .remove(&observer.context.key());
    assert!(observer.pending(&shell, &ids()).is_err());
    assert!(observer.changes.has_changed().is_err());
    let context = Arc::new(Context::capture(&shell, TEAM).unwrap());
    ensure(&mut shell, context).unwrap();
    assert!(Observer::capture(&shell, TEAM).unwrap().is_some());
    assert!(observer.pending(&shell, &ids()).is_err());
}

#[test]
fn absent_initial_queue_is_no_work_but_unobserved_mode_is_unavailable() {
    let mut shell = shell();
    assert!(Observer::capture(&shell, TEAM).unwrap().is_none());
    shell.directory_member_fetches.automatic.mode = mode::Registry::default();
    assert!(Observer::capture(&shell, TEAM).is_err());
}

#[test]
fn account_epoch_auth_and_transport_retire_original_observer() {
    for variant in 0..5 {
        let (mut shell, observer) = prepared();
        match variant {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.active_account = Some("replacement".into()),
            2 => shell.core.session.as_mut().unwrap().d_cookie = "replacement".into(),
            3 => shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap())),
            _ => shell.core.workspaces.get_mut(TEAM).unwrap().self_user_id = "U00000888".into(),
        }
        assert!(
            observer.pending(&shell, &ids()).is_err(),
            "variant {variant}"
        );
    }
}

#[test]
fn accepted_mode_revocation_wakes_a_waiter_without_io_completion() {
    let (mut shell, observer) = prepared();
    assert!(!observer.changes.has_changed().unwrap());
    mode::Boot::capture(&json!({"noSocket": true}))
        .accepted(&mut shell, TEAM)
        .unwrap();
    assert!(observer.changes.has_changed().unwrap());
    assert!(observer.pending(&shell, &ids()).is_err());
    assert!(
        shell.directory_member_fetches.automatic.slots[&observer.context.key()]
            .manager
            .has_work_for(&ids())
    );
}

#[test]
fn accepted_disconnect_wakes_but_does_not_complete_or_increment_connection() {
    let (mut shell, observer) = prepared();
    shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation = 3;
    assert!(!crate::realtime::mark_disconnected(&mut shell, TEAM, 2));
    assert!(!observer.changes.has_changed().unwrap());
    assert!(crate::realtime::mark_disconnected(&mut shell, TEAM, 3));
    assert!(observer.changes.has_changed().unwrap());
    assert_eq!(shell.core.workspaces[TEAM].rt_generation, 3);
    assert!(observer.pending(&shell, &ids()).unwrap());
}
