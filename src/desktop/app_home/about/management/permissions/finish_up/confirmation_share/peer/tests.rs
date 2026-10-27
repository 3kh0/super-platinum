use super::*;
use super_platinum_core::{
    block_actions::workflow_management::member_hydration::{Flags as MemberFlags, RendererReply},
    config::{Session, WorkspaceSession},
    slack::Transport,
    state::{Workspace, directory_members::Origin},
};

const TEAM: &str = "TDMTEST01";
const PEER: &str = "UDMPEER01";
const ACCOUNT: &str = "share-peer-test";

fn row(name: &str) -> Value {
    json!({"id":PEER,"team_id":TEAM,"name":name,
        "profile":{"display_name":name},"opaque":{"source":name}})
}
fn setup() -> ShellState {
    let mut shell = ShellState::login(crate::media::MediaRegistry::default());
    let session = WorkspaceSession {
        team_id: TEAM.into(),
        user_id: "UDMSELF01".into(),
        enterprise_id: None,
        name: "Synthetic".into(),
        url: "https://fixture.invalid".into(),
        token: "synthetic".into(),
    };
    shell.core.active_account = Some(ACCOUNT.into());
    shell.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        unified_user_client_enabled: BTreeMap::new(),
        workspaces: BTreeMap::from([(TEAM.into(), session.clone())]),
    });
    shell.core.transport = Some(Arc::new(Transport::disconnected_fixture("synthetic")));
    let mut workspace = Workspace::from_session(&session);
    workspace
        .directory_members
        .begin(ACCOUNT, &session, &BTreeSet::new())
        .unwrap();
    shell.core.workspaces.insert(TEAM.into(), workspace);
    adopt(&mut shell, row("Original"));
    shell
}

// Exercise the real ordered source/native adoption. This acquired empty member
// feature assignment is only hydration context, never a posting permission.
fn adopt(shell: &mut ShellState, raw: Value) {
    let session = shell.core.session.as_ref().unwrap().workspaces[TEAM].clone();
    let epoch = shell.core.account_epoch;
    let id = raw["id"].as_str().unwrap().to_owned();
    let w = shell.core.workspaces.get_mut(TEAM).unwrap();
    w.directory_members
        .enqueue(
            ACCOUNT,
            &session,
            Origin::Directory,
            vec![raw],
            json!({(id):true}),
        )
        .unwrap();
    let flags = MemberFlags::decode(&json!({"ok":true,"assignments":{}})).unwrap();
    let pending = w
        .directory_members
        .prepare(
            ACCOUNT,
            &session,
            epoch,
            GENERATION,
            flags.clone(),
            &w.users,
        )
        .unwrap();
    assert!(pending.fields().iter().all(|value| value.is_ascii()));
    let reply = RendererReply {
        generation: pending.generation().into(),
        operation: "ordinary-toLowerCase".into(),
        fields: pending.fields().to_vec(),
        lowered: pending
            .fields()
            .iter()
            .map(|value| value.to_ascii_lowercase())
            .collect(),
    };
    w.users = w
        .directory_members
        .adopt(
            pending, reply, &flags, ACCOUNT, &session, epoch, GENERATION, &w.users,
        )
        .unwrap()
        .into_parts()
        .1;
}
fn witness(shell: &ShellState) -> Witness {
    let saved = Witness::capture(shell, TEAM, PEER).unwrap();
    assert!(saved.current(shell));
    assert_eq!(saved.prior.get(PEER), Some(&Some(true)));
    saved
}

#[test]
fn original_source_and_typed_peer_are_both_required() {
    let mut shell = setup();
    let saved = witness(&shell);
    let original = shell.core.workspaces[TEAM].users[PEER].clone();
    shell
        .core
        .workspaces
        .get_mut(TEAM)
        .unwrap()
        .users
        .get_mut(PEER)
        .unwrap()
        .name = Some("Typed drift".into());
    assert!(!saved.current(&shell));
    assert!(Witness::capture(&shell, TEAM, PEER).is_err());
    shell
        .core
        .workspaces
        .get_mut(TEAM)
        .unwrap()
        .users
        .insert(PEER.into(), original);
    assert!(saved.current(&shell));
    shell.core.workspaces.get_mut(TEAM).unwrap().users.insert(
        "UTYPED001".into(),
        serde_json::from_value(json!({"id":"UTYPED001","name":"Typed only"})).unwrap(),
    );
    assert!(Witness::capture(&shell, TEAM, "UTYPED001").is_err());
}

#[test]
fn actual_source_a_b_a_never_revives_the_original_witness() {
    let mut shell = setup();
    let saved = witness(&shell);
    adopt(&mut shell, row("Changed"));
    assert!(!saved.current(&shell));
    adopt(&mut shell, row("Original"));
    let w = &shell.core.workspaces[TEAM];
    // Exact content equality alone would admit this ABA. The original revision
    // fence is the additional requirement the production Witness must retain.
    assert!(saved.subset.current_rows(
        &w.directory_members,
        ACCOUNT,
        &saved.ctx.session,
        saved.ctx.epoch,
        GENERATION,
        &w.users
    ));
    assert!(!saved.current(&shell));
    assert!(
        Witness::capture(&shell, TEAM, PEER)
            .unwrap()
            .current(&shell)
    );
}

#[test]
fn selected_source_invalidation_and_pending_ingress_retire_before_adoption() {
    for invalidate in [true, false] {
        let mut shell = setup();
        let saved = witness(&shell);
        let w = shell.core.workspaces.get_mut(TEAM).unwrap();
        if invalidate {
            w.directory_members.invalidate(PEER);
        } else {
            w.directory_members
                .enqueue(
                    ACCOUNT,
                    &saved.ctx.session,
                    Origin::Directory,
                    vec![row("Pending")],
                    json!({(PEER):false}),
                )
                .unwrap();
        }
        assert!(!saved.current(&shell));
        assert!(Witness::capture(&shell, TEAM, PEER).is_err());
    }
}

#[test]
fn unrelated_member_intake_conservatively_retires_original_revision() {
    let mut shell = setup();
    let saved = witness(&shell);
    adopt(
        &mut shell,
        json!({"id":"UOTHER001","team_id":TEAM,"name":"Other","profile":{}}),
    );
    let w = &shell.core.workspaces[TEAM];
    assert!(saved.subset.current_rows(
        &w.directory_members,
        ACCOUNT,
        &saved.ctx.session,
        saved.ctx.epoch,
        GENERATION,
        &w.users
    ));
    assert!(!saved.current(&shell));
}

#[test]
fn original_account_session_transport_and_socket_retirement_refuse_peer() {
    let changes: Vec<Box<dyn Fn(&mut ShellState)>> = vec![
        Box::new(|s| s.core.active_account = Some("successor".into())),
        Box::new(|s| s.core.account_epoch += 1),
        Box::new(|s| s.core.session.as_mut().unwrap().d_cookie = "replacement".into()),
        Box::new(|s| {
            s.core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut(TEAM)
                .unwrap()
                .token = "replacement".into()
        }),
        Box::new(|s| {
            s.core.transport = Some(Arc::new(Transport::disconnected_fixture("synthetic")))
        }),
        Box::new(|s| s.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1),
        Box::new(|s| s.core.workspaces.get_mut(TEAM).unwrap().self_user_id = "USUCCESS1".into()),
    ];
    for change in changes {
        let mut shell = setup();
        let saved = witness(&shell);
        change(&mut shell);
        assert!(!saved.current(&shell));
    }
}

#[test]
fn replacement_source_history_does_not_revive_equal_peer_bytes() {
    let mut shell = setup();
    let saved = witness(&shell);
    let replacement = setup();
    shell
        .core
        .workspaces
        .get_mut(TEAM)
        .unwrap()
        .directory_members = replacement.core.workspaces[TEAM].directory_members.clone();
    assert!(!saved.current(&shell));
}
