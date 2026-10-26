use super::*;
use crate::media::MediaRegistry;
use std::collections::BTreeMap;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::Transport,
    state::{
        Workspace,
        directory_usergroups::{NativeMembers, Operation, Origin, RendererReply},
    },
};
const TEAM: &str = "TGROUP001";
const GROUP: &str = "SGROUP001";
fn shell() -> ShellState {
    let mut shell = ShellState::login(MediaRegistry::default());
    let session = WorkspaceSession {
        team_id: TEAM.into(),
        user_id: "USELF0001".into(),
        enterprise_id: None,
        name: "Closed fixture".into(),
        url: "https://example.invalid".into(),
        token: "synthetic-token".into(),
    };
    shell.core.active_account = Some("unknown-fixture".into());
    shell.core.active_team = Some(TEAM.into());
    shell.core.session = Some(Session {
        d_cookie: "synthetic-cookie".into(),
        unified_user_client_enabled: BTreeMap::new(),
        workspaces: BTreeMap::from([(TEAM.into(), session.clone())]),
    });
    shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap()));
    let mut w = Workspace::from_session(&session);
    let lease = w.directory_usergroups.begin_boot(&w.usergroups).unwrap();
    let commit = w
        .directory_usergroups
        .observe_boot(
            lease,
            "unknown-fixture",
            &session,
            shell.core.account_epoch,
            &w.usergroups,
            &json!({"ok":true,"team":{"id":TEAM},"self":{"id":"USELF0001"},"subteams":{"self":[]}}),
        )
        .unwrap();
    (w.directory_usergroups, w.usergroups) = commit.into_parts();
    shell.core.workspaces.insert(TEAM.into(), w);
    shell
}

fn put(s: &mut ShellState, row: Value) {
    let o = Context::capture(s, TEAM).unwrap();
    let w = s.core.workspaces.get_mut(TEAM).unwrap();
    let t = w
        .directory_usergroups
        .enqueue(
            &o.account,
            &o.session,
            o.epoch,
            "test",
            &w.usergroups,
            Operation::Metadata {
                origin: Origin::Upsert,
                rows: vec![row],
                native_members: NativeMembers::Preserve,
            },
        )
        .unwrap();
    let p = w
        .directory_usergroups
        .next_for(&t, &o.account, &o.session, o.epoch, &w.usergroups)
        .unwrap()
        .unwrap();
    assert!(p.fields().iter().all(|v| v.is_ascii()));
    let r = RendererReply {
        generation: p.generation().into(),
        fields: p.fields().to_vec(),
        lowered: p.fields().iter().map(|v| v.to_ascii_lowercase()).collect(),
        locale: (!p.fields().is_empty()).then(|| "en-US".into()),
    };
    let c = w
        .directory_usergroups
        .commit(p, r, &o.account, &o.session, o.epoch, &w.usergroups)
        .unwrap();
    (w.directory_usergroups, w.usergroups) = c.into_parts();
}
#[test]
fn dispatch_retains_existing_markers_and_membership_only_is_not_metadata() {
    let mut s = shell();
    let o = Context::capture(&s, TEAM).unwrap();
    let missing = "SMISSING1".to_string();
    let ids = vec![GROUP.into(), missing.clone()];
    s.core.workspaces.get_mut(TEAM).unwrap().usergroups.insert(
        missing.clone(),
        super_platinum_core::slack::models::UserGroup {
            id: missing.clone(),
            ..Default::default()
        },
    );
    // A forged native mutation does not establish a current source snapshot.
    assert!(snapshot(&s, &o).is_err());
    s.core
        .workspaces
        .get_mut(TEAM)
        .unwrap()
        .usergroups
        .remove(&missing);
    put(
        &mut s,
        json!({"id":GROUP,"handle":"old","isNonExistent":true}),
    );
    let source = snapshot(&s, &o).unwrap();
    let (requested, retained) = worker::partition(&source, &ids);
    assert_eq!(requested, vec![missing]);
    assert_eq!(retained.into_iter().collect::<Vec<_>>(), vec![GROUP]);
}
#[test]
fn absent_selection_retires_after_exact_source_successor_and_socket() {
    let mut s = shell();
    let old = select(&mut s, TEAM, GROUP).unwrap();
    assert!(old.lookup().is_transient());
    assert!(old.current(&s));
    put(&mut s, json!({"id":GROUP,"handle":"found"}));
    assert!(!old.current(&s));
    let known = observed(&s, TEAM, GROUP).unwrap();
    assert!(known.lookup().is_source_hit());
    assert_eq!(known.lookup().effective()["handle"], "found");
    s.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1;
    assert!(!known.current(&s));
}
