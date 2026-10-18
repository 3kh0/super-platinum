use super::*;
use serde_json::json;
use std::collections::BTreeSet;
use super_platinum_core::{
    block_actions::workflow_management::member_hydration::Flags,
    config::{Session, WorkspaceSession},
    slack::Transport,
    state::Workspace,
};
const TEAM: &str = "TSEARCH01";
const FIRST: &str = "USEARCH01";
const SECOND: &str = "USEARCH02";
const GENERATION: &str = "profile-lease-17";
fn flags() -> Flags {
    Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap()
}
fn row(id: &str, name: &str, version: u64) -> Value {
    json!({"id":id,"name":name,"updated":version,"profile":{"display_name":name},
        "opaque":{"retained":name}})
}
fn shell() -> ShellState {
    let mut shell = ShellState::login(crate::media::MediaRegistry::default());
    let owner = WorkspaceSession {
        team_id: TEAM.into(),
        enterprise_id: None,
        user_id: "USELF0001".into(),
        name: "Fixture".into(),
        url: "https://fixture.invalid".into(),
        token: "synthetic".into(),
    };
    shell.core.active_account = Some("A".into());
    shell.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        unified_user_client_enabled: BTreeMap::new(),
        workspaces: BTreeMap::from([(TEAM.into(), owner.clone())]),
    });
    shell.core.transport = Some(Arc::new(Transport::new("synthetic").unwrap()));
    let mut workspace = Workspace::from_session(&owner);
    workspace
        .directory_members
        .begin("A", &owner, &BTreeSet::new())
        .unwrap();
    shell.core.workspaces.insert(TEAM.into(), workspace);
    shell
}
fn witness(shell: &ShellState) -> Witness {
    Witness::capture(shell, TEAM, GENERATION).unwrap()
}
fn drain(stage: &mut Stage, witness: &Witness) {
    let ctx = &witness.context;
    let pending = stage
        .store
        .prepare(
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            &witness.generation,
            flags(),
            &stage.native,
        )
        .unwrap();
    assert_eq!(pending.origin(), Origin::Directory);
    let reply = RendererReply {
        generation: pending.generation().into(),
        operation: "ordinary-toLowerCase".into(),
        fields: pending.fields().to_vec(),
        lowered: pending
            .fields()
            .iter()
            .map(|field| {
                assert!(field.is_ascii());
                field.to_ascii_lowercase()
            })
            .collect(),
    };
    stage.native = stage
        .store
        .adopt(
            pending,
            reply,
            &flags(),
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            &witness.generation,
            &stage.native,
        )
        .unwrap()
        .into_parts()
        .1;
}
fn install(shell: &mut ShellState, rows: &[Value]) -> Ready {
    let witness = witness(shell);
    let mut stage = Stage::capture(shell, &witness, rows).unwrap();
    drain(&mut stage, &witness);
    stage.install(shell, &witness, &|_| true).unwrap()
}

#[test]
fn actual_directory_order_opaque_fields_and_stranger_interaction_publish_as_one_pair() {
    let mut shell = shell();
    let before = shell.core.workspaces[TEAM].directory_members.cached();
    let witness = witness(&shell);
    let mut stranger = row(SECOND, "Second", 101);
    stranger["is_stranger"] = json!(true);
    let incoming = vec![
        row(FIRST, "First", 101),
        stranger,
        row(FIRST, "Latest", 102),
    ];
    let mut stage = Stage::capture(&shell, &witness, &incoming).unwrap();
    drain(&mut stage, &witness);
    assert_eq!(
        shell.core.workspaces[TEAM].directory_members.cached(),
        before
    );
    assert!(shell.core.workspaces[TEAM].users.is_empty());
    let ready = stage.install(&mut shell, &witness, &|_| true).unwrap();
    let rows = ready.rows();
    assert_eq!(
        rows.iter()
            .map(|r| r.source["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [FIRST, SECOND]
    );
    assert_eq!(rows[0].source["opaque"], json!({"retained":"Latest"}));
    assert_eq!(rows[0].source["updated"], 102);
    assert_eq!(rows[0].interaction, Some(true));
    assert_eq!(rows[1].interaction, Some(false));
    let typed = serde_json::to_value(&shell.core.workspaces[TEAM].users[FIRST]).unwrap();
    assert_eq!(typed["opaque"], json!({"retained":"Latest"}));
    assert_eq!(typed["profile"]["display_name"], "Latest");
}

#[test]
fn original_account_session_transport_socket_source_and_native_fence_every_admission() {
    let changes: Vec<Box<dyn Fn(&mut ShellState)>> = vec![
        Box::new(|s| s.core.active_account = Some("B".into())),
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
        Box::new(|s| s.core.transport = Some(Arc::new(Transport::new("synthetic").unwrap()))),
        Box::new(|s| s.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1),
        Box::new(|s| {
            s.core
                .workspaces
                .get_mut(TEAM)
                .unwrap()
                .directory_members
                .invalidate(FIRST)
        }),
        Box::new(|s| {
            s.core
                .workspaces
                .get_mut(TEAM)
                .unwrap()
                .users
                .get_mut(FIRST)
                .unwrap()
                .name = Some("Socket".into())
        }),
    ];
    for change in changes {
        let mut shell = shell();
        install(&mut shell, &[row(FIRST, "Original", 101)]);
        let witness = witness(&shell);
        let mut stage = Stage::capture(&shell, &witness, &[row(FIRST, "Delayed", 102)]).unwrap();
        drain(&mut stage, &witness);
        change(&mut shell);
        let native = context::native_snapshot(&shell.core.workspaces[TEAM].users).unwrap();
        let source = shell.core.workspaces[TEAM].directory_members.cached();
        assert!(!witness.current(&shell));
        assert!(Stage::capture(&shell, &witness, &[]).is_err());
        assert!(stage.install(&mut shell, &witness, &|_| true).is_err());
        assert_eq!(
            context::native_snapshot(&shell.core.workspaces[TEAM].users).unwrap(),
            native
        );
        assert_eq!(
            shell.core.workspaces[TEAM].directory_members.cached(),
            source
        );
    }
}

#[test]
fn query_retirement_during_renderer_does_not_install_private_success() {
    let mut shell = shell();
    let witness = witness(&shell);
    let mut stage = Stage::capture(&shell, &witness, &[row(FIRST, "Private", 101)]).unwrap();
    drain(&mut stage, &witness);
    let before = shell.core.workspaces[TEAM].directory_members.cached();
    assert!(stage.install(&mut shell, &witness, &|_| false).is_err());
    assert_eq!(
        shell.core.workspaces[TEAM].directory_members.cached(),
        before
    );
    assert!(shell.core.workspaces[TEAM].users.is_empty());
}

#[test]
fn equal_version_uses_existing_scqy_skip_and_does_not_repair_native_drift() {
    let mut shell = shell();
    install(&mut shell, &[row(FIRST, "Original", 101)]);
    let ready = install(&mut shell, &[row(FIRST, "SameVersion", 101)]);
    assert_eq!(ready.rows()[0].source["name"], "Original");
    shell
        .core
        .workspaces
        .get_mut(TEAM)
        .unwrap()
        .users
        .get_mut(FIRST)
        .unwrap()
        .name = Some("TypedDrift".into());
    let witness = witness(&shell);
    let mut stage =
        Stage::capture(&shell, &witness, &[row(FIRST, "FreshSameVersion", 101)]).unwrap();
    drain(&mut stage, &witness);
    assert!(stage.install(&mut shell, &witness, &|_| true).is_err());
    assert_eq!(
        shell.core.workspaces[TEAM].users[FIRST].name.as_deref(),
        Some("TypedDrift")
    );
}

#[test]
fn unknown_history_and_legacy_native_rows_never_become_ready_from_empty_search() {
    let mut shell = shell();
    shell
        .core
        .workspaces
        .get_mut(TEAM)
        .unwrap()
        .directory_members = Store::default();
    assert!(Witness::capture(&shell, TEAM, GENERATION).is_err());
    let owner = shell.core.session.as_ref().unwrap().workspaces[TEAM].clone();
    let w = shell.core.workspaces.get_mut(TEAM).unwrap();
    w.users.insert(
        FIRST.into(),
        serde_json::from_value(row(FIRST, "Warm", 101)).unwrap(),
    );
    w.directory_members
        .begin("A", &owner, &BTreeSet::from([FIRST.into()]))
        .unwrap();
    let witness = witness(&shell);
    let mut stage = Stage::capture(&shell, &witness, &[]).unwrap();
    drain(&mut stage, &witness);
    assert!(stage.install(&mut shell, &witness, &|_| true).is_err());
    assert_eq!(shell.core.workspaces[TEAM].users.len(), 1);
    assert!(
        shell.core.workspaces[TEAM]
            .directory_members
            .required_observations(&shell.core.workspaces[TEAM].users)
            .contains(FIRST)
    );
    // Only actual returned data for the missing native observation can close it.
    let ready = install(&mut shell, &[row(FIRST, "Observed", 102)]);
    assert_eq!(ready.rows()[0].source["name"], "Observed");
}

#[test]
fn successful_empty_search_preserves_full_inventory_and_ready_retires_on_next_source_change() {
    let mut shell = shell();
    install(
        &mut shell,
        &[row(FIRST, "First", 101), row(SECOND, "Second", 101)],
    );
    let ready = install(&mut shell, &[]);
    assert_eq!(ready.rows().len(), 2);
    let ctx = Context::capture(&shell, TEAM).unwrap();
    let w = &shell.core.workspaces[TEAM];
    assert!(ready.current(
        &w.directory_members,
        &ctx.account,
        &ctx.session,
        ctx.epoch,
        GENERATION,
        &w.users
    ));
    install(&mut shell, &[row(SECOND, "Renamed", 102)]);
    let w = &shell.core.workspaces[TEAM];
    assert!(!ready.current(
        &w.directory_members,
        &ctx.account,
        &ctx.session,
        ctx.epoch,
        GENERATION,
        &w.users
    ));
    assert_eq!(ready.rows()[1].source["name"], "Second");
}

#[test]
fn malformed_directory_profile_and_non_member_identity_never_change_live_store() {
    let shell = shell();
    let witness = witness(&shell);
    assert!(Stage::capture(&shell, &witness, &[json!({"id":"CSEARCH01","profile":{}})]).is_err());
    let stage = Stage::capture(&shell, &witness, &[json!({"id":FIRST,"profile":false})]).unwrap();
    let ctx = &witness.context;
    assert!(
        stage
            .store
            .prepare(
                &ctx.account,
                &ctx.session,
                ctx.epoch,
                GENERATION,
                flags(),
                &stage.native
            )
            .is_err()
    );
    assert!(shell.core.workspaces[TEAM].users.is_empty());
    assert!(
        !shell.core.workspaces[TEAM]
            .directory_members
            .has_pending(&ctx.account, &ctx.session)
            .unwrap()
    );
}

#[test]
fn source_history_without_actual_assignment_observation_cannot_capture_read_or_snapshot() {
    let mut shell = shell();
    install(&mut shell, &[row(FIRST, "First", 101)]);
    assert!(Read::capture(&shell, TEAM, GENERATION).is_err());
    assert!(snapshot(&shell, TEAM, GENERATION).is_err());
}
