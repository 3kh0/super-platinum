use super::*;
use crate::media::MediaRegistry;
use serde_json::json;
use std::collections::BTreeMap;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::Transport,
    state::Workspace,
};
const TEAM: &str = "TGROUP001";
const A: &str = "SGROUP001";
const B: &str = "SGROUP002";
fn shell() -> ShellState {
    let mut shell = ShellState::login(MediaRegistry::default());
    let owner = WorkspaceSession {
        team_id: TEAM.into(),
        user_id: "USELF0001".into(),
        enterprise_id: None,
        name: "Synthetic".into(),
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
    shell
        .core
        .workspaces
        .insert(TEAM.into(), Workspace::from_session(&owner));
    shell
}
fn row(id: &str, handle: &str) -> UserGroup {
    UserGroup::from_observed(
        &json!({"id":id,"team_id":TEAM,"handle":handle,"name":"Group","opaque":{"retained":true}}),
    )
    .unwrap()
}
fn capture(shell: &ShellState) -> Attempt {
    Attempt::capture(shell, TEAM, &[A.into(), B.into(), A.into()]).unwrap()
}

#[test]
fn actual_partial_rows_land_without_inventing_missing_groups() {
    let mut shell = shell();
    let attempt = capture(&shell);
    assert_eq!(attempt.ids, vec![A, B]);
    let actual = row(A, "  Cafe\u{0301}  ");
    assert!(
        attempt
            .publish(&mut shell, vec![actual.clone()], &|_| true)
            .unwrap()
    );
    let groups = &shell.core.workspaces[TEAM].usergroups;
    assert_eq!(groups[A].label_source, actual.label_source);
    assert!(!groups.contains_key(B));
    let next = Attempt::capture(&shell, TEAM, &[B.into()]).unwrap();
    assert_eq!(next.ids, vec![B]);
}

#[test]
fn raw_legacy_replacement_preserves_loaded_membership_and_cache_witness() {
    let mut shell = shell();
    shell
        .core
        .workspaces
        .get_mut(TEAM)
        .unwrap()
        .usergroups
        .insert(
            A.into(),
            UserGroup {
                id: A.into(),
                handle: "legacy".into(),
                users: vec!["UMEMBER01".into()],
                members_loaded: true,
                ..Default::default()
            },
        );
    let attempt = capture(&shell);
    let mut actual = row(A, "fresh");
    actual.users = vec!["NOTMEMBERSHIP".into()];
    assert!(
        attempt
            .publish(&mut shell, vec![actual.clone()], &|_| true)
            .unwrap()
    );
    let group = &shell.core.workspaces[TEAM].usergroups[A];
    assert_eq!(group.users, vec!["UMEMBER01"]);
    assert!(group.members_loaded);
    assert_eq!(group.label_source, actual.label_source);
    let cached: UserGroup = serde_json::from_value(serde_json::to_value(group).unwrap()).unwrap();
    assert_eq!(cached.label_source, actual.label_source);
}

#[test]
fn empty_or_pending_only_result_has_no_projection_and_no_terminal_sentinel() {
    let mut shell = shell();
    let attempt = capture(&shell);
    let projection = shell.channels.as_ptr();
    assert!(!attempt.publish(&mut shell, vec![], &|_| true).unwrap());
    assert_eq!(shell.channels.as_ptr(), projection);
    assert!(shell.core.workspaces[TEAM].usergroups.is_empty());
}

#[test]
fn owner_and_consumer_retirement_refuse_all_partial_publication() {
    for kind in 0..6 {
        let mut shell = shell();
        let attempt = capture(&shell);
        match kind {
            0 => shell.core.active_account = Some("foreign".into()),
            1 => shell.core.account_epoch += 1,
            2 => shell.core.session.as_mut().unwrap().d_cookie = "foreign".into(),
            3 => shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap())),
            4 => shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1,
            _ => (),
        }
        assert!(
            attempt
                .publish(&mut shell, vec![row(A, "late")], &|_| kind != 5)
                .is_err()
        );
        assert!(shell.core.workspaces[TEAM].usergroups.is_empty());
    }
}

#[test]
fn newer_source_and_unrequested_or_unobserved_rows_are_not_overwritten() {
    for kind in 0..3 {
        let mut shell = shell();
        let attempt = capture(&shell);
        if kind == 0 {
            shell
                .core
                .workspaces
                .get_mut(TEAM)
                .unwrap()
                .usergroups
                .insert(A.into(), row(A, "socket-newer"));
        }
        let before = serde_json::to_value(&shell.core.workspaces[TEAM].usergroups).unwrap();
        let incoming = match kind {
            1 => row("SOTHER001", "foreign"),
            2 => UserGroup {
                id: A.into(),
                handle: "typed-only".into(),
                ..Default::default()
            },
            _ => row(A, "delayed"),
        };
        assert!(
            attempt
                .publish(&mut shell, vec![incoming], &|_| true)
                .is_err()
        );
        assert_eq!(
            serde_json::to_value(&shell.core.workspaces[TEAM].usergroups).unwrap(),
            before
        );
    }
}

#[test]
fn background_progress_retains_original_batch_order_and_unsent_ids() {
    let progress = RefreshProgress::default();
    progress.admitted(&[B.into(), B.into()]);
    assert_eq!(progress.ids(&[A.into(), B.into()]), [B]);
    progress.admitted(&[A.into()]);
    assert_eq!(progress.ids(&[A.into(), B.into()]), [A, B]);
}

#[test]
fn legacy_background_witness_cannot_borrow_later_canonical_lineage() {
    let mut shell = shell();
    let owner = Context::capture(&shell, TEAM).unwrap();
    let source = RefreshSource::capture(&shell, &owner).unwrap();
    assert!(source.current(&shell, &owner));
    let w = shell.core.workspaces.get_mut(TEAM).unwrap();
    let boot = w.directory_usergroups.begin_boot(&w.usergroups).unwrap();
    let commit = w
        .directory_usergroups
        .observe_boot(
            boot,
            &owner.account,
            &owner.session,
            owner.epoch,
            &w.usergroups,
            &json!({"ok":true,"team":{"id":TEAM},"self":{"id":"USELF0001"},"subteams":{"self":[]}}),
        )
        .unwrap();
    (w.directory_usergroups, w.usergroups) = commit.into_parts();
    assert!(!source.current(&shell, &owner));
    let tracked = RefreshSource::capture(&shell, &owner).unwrap();
    assert!(tracked.current(&shell, &owner));
    let w = shell.core.workspaces.get_mut(TEAM).unwrap();
    let _ticket = w
        .directory_usergroups
        .enqueue(
            &owner.account,
            &owner.session,
            owner.epoch,
            "accepted-newer",
            &w.usergroups,
            super_platinum_core::state::directory_usergroups::Operation::Membership { ids: vec![] },
        )
        .unwrap();
    assert!(!tracked.current(&shell, &owner));
    assert!(
        RefreshSource::capture(&shell, &owner).is_err(),
        "pending source is not a legacy fallback"
    );
}
