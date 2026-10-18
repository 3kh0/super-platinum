use super::*;
use serde_json::json;
use super_platinum_core::{
    block_actions::workflow_management::member_hydration::Flags,
    config::Session,
    state::{Workspace, directory_members::Origin},
};
const ID: &str = "UKNOWN001";
fn flags() -> Flags {
    Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap()
}
fn row(name: &str, updated: u64) -> Value {
    json!({"id":ID,"name":name,"updated":updated,"is_stranger":false,"profile":{"display_name":name}})
}
fn context() -> Context {
    let mut shell = ShellState::login(crate::media::MediaRegistry::default());
    let owner = WorkspaceSession {
        team_id: "TKNOWN001".into(),
        enterprise_id: None,
        user_id: "USELF0001".into(),
        name: "Test".into(),
        url: "https://example.invalid".into(),
        token: "synthetic".into(),
    };
    shell.core.active_account = Some("A".into());
    shell.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        unified_user_client_enabled: BTreeMap::new(),
        workspaces: BTreeMap::from([(owner.team_id.clone(), owner.clone())]),
    });
    shell.core.transport = Some(Arc::new(Transport::new("synthetic").unwrap()));
    shell
        .core
        .workspaces
        .insert(owner.team_id.clone(), Workspace::from_session(&owner));
    Context::capture(&shell, &owner.team_id).unwrap()
}
fn reply(fields: &[String]) -> RendererReply {
    assert!(fields.iter().all(|value| value.is_ascii()));
    RendererReply {
        generation: GENERATION.into(),
        operation: "ordinary-toLowerCase".into(),
        fields: fields.to_vec(),
        lowered: fields
            .iter()
            .map(|value| value.to_ascii_lowercase())
            .collect(),
    }
}
fn drain(stage: &mut Stage, ctx: &Context) {
    while stage.store.has_pending(&ctx.account, &ctx.session).unwrap() {
        let pending = stage
            .store
            .prepare(
                &ctx.account,
                &ctx.session,
                ctx.epoch,
                GENERATION,
                flags(),
                &stage.native,
            )
            .unwrap();
        let response = reply(pending.fields());
        let commit = stage
            .store
            .adopt(
                pending,
                response,
                &flags(),
                &ctx.account,
                &ctx.session,
                ctx.epoch,
                GENERATION,
                &stage.native,
            )
            .unwrap();
        stage.native = commit.into_parts().1;
    }
}
fn seeded(wire: Value) -> (Context, Store, HashMap<String, User>) {
    let ctx = context();
    let mut store = Store::default();
    store
        .begin(&ctx.account, &ctx.session, &BTreeSet::new())
        .unwrap();
    store
        .enqueue(
            &ctx.account,
            &ctx.session,
            Origin::DirectUpsert,
            vec![wire],
            json!({}),
        )
        .unwrap();
    let mut stage = Stage {
        store,
        native: HashMap::new(),
        fresh: vec![],
    };
    drain(&mut stage, &ctx);
    (ctx, stage.store, stage.native)
}
fn received(rows: Vec<Value>, disconnected: Vec<String>) -> read::Received {
    read::Received {
        requested: vec![ID.into()],
        rows,
        interactions: BTreeMap::from([(ID.into(), true)]),
        disconnected,
        pages: vec![],
        errors: vec![],
    }
}
fn source(store: &Store, native: &HashMap<String, User>, ctx: &Context) -> Value {
    store
        .subset(
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
            &BTreeSet::from([ID.into()]),
            native,
        )
        .unwrap()
        .rows()[0]
        .source
        .clone()
}
fn repair(stage: &mut Stage, ctx: &Context) -> Result<(), Error> {
    let rows = stage.repair_rows()?;
    if rows.is_empty() {
        return Ok(());
    }
    let pending = stage.store.prepare_native_repair(
        &ctx.account,
        &ctx.session,
        ctx.epoch,
        GENERATION,
        flags(),
        rows,
        &stage.native,
    )?;
    let response = reply(pending.fields());
    stage.native = stage
        .store
        .adopt_native_repair(
            pending,
            response,
            &flags(),
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
            &stage.native,
        )?
        .into_native();
    Ok(())
}

#[test]
fn new_version_stages_exact_requested_ledger_and_keeps_live_pair_untouched() {
    let (ctx, mut store, native) = seeded(row("Original", 101));
    store.invalidate(ID);
    let before = store.cached();
    let received = received(vec![row("Renamed", 102)], vec![ID.into()]);
    let mut stage = Stage::from_parts(store.clone(), native.clone(), &ctx, &received).unwrap();
    drain(&mut stage, &ctx);
    assert_eq!(store.cached(), before);
    assert_eq!(native[ID].name.as_deref(), Some("Original"));
    assert_eq!(source(&stage.store, &stage.native, &ctx)["name"], "Renamed");
    assert!(stage.repair_rows().unwrap().is_empty());
    assert_eq!(
        stage
            .store
            .observations(&ctx.account, &ctx.session, ctx.epoch, GENERATION)
            .unwrap()
            .disconnected()[ID],
        true
    );
}

#[test]
fn fresh_same_version_repairs_only_native_drift_after_source_skip() {
    let (ctx, store, mut native) = seeded(row("Original", 101));
    let before = source(&store, &native, &ctx);
    native.get_mut(ID).unwrap().name = Some("Drift".into());
    let received = received(vec![row("Original", 101)], vec![]);
    let mut stage = Stage::from_parts(store, native, &ctx, &received).unwrap();
    drain(&mut stage, &ctx);
    assert_eq!(stage.native[ID].name.as_deref(), Some("Drift"));
    assert_eq!(stage.repair_rows().unwrap(), received.rows);
    repair(&mut stage, &ctx).unwrap();
    assert_eq!(stage.native[ID].name.as_deref(), Some("Original"));
    assert_eq!(source(&stage.store, &stage.native, &ctx), before);
}

#[test]
fn same_version_uncovered_drift_and_conflicting_fresh_source_are_refused() {
    for conflict in [false, true] {
        let (ctx, store, mut native) = seeded(row("Original", 101));
        if conflict {
            native.get_mut(ID).unwrap().name = Some("Drift".into());
        } else {
            let mut raw = serde_json::to_value(&native[ID]).unwrap();
            raw["is_admin"] = json!(true);
            native.insert(ID.into(), serde_json::from_value(raw).unwrap());
        }
        let fresh = row(if conflict { "Conflicting" } else { "Original" }, 101);
        let mut stage =
            Stage::from_parts(store, native, &ctx, &received(vec![fresh], vec![])).unwrap();
        drain(&mut stage, &ctx);
        let before = stage.store.cached();
        assert!(repair(&mut stage, &ctx).is_err());
        assert_eq!(stage.store.cached(), before);
    }
}

#[test]
fn terminal_zero_to_positive_zero_is_not_same_version_skipped() {
    let mut terminal = row("", 0);
    terminal["isNonExistent"] = json!(true);
    let (ctx, store, native) = seeded(terminal);
    let mut stage = Stage::from_parts(
        store,
        native,
        &ctx,
        &received(vec![row("Resolved", 0)], vec![]),
    )
    .unwrap();
    drain(&mut stage, &ctx);
    let observed = source(&stage.store, &stage.native, &ctx);
    assert_eq!(observed["name"], "Resolved");
    assert_ne!(observed["isNonExistent"], true);
    assert!(stage.repair_rows().unwrap().is_empty());
}

#[test]
fn no_row_terminal_keeps_known_source_and_does_not_synthesize_placeholder() {
    let mut terminal = row("", 0);
    terminal["isNonExistent"] = json!(true);
    let (ctx, store, native) = seeded(terminal);
    let before = source(&store, &native, &ctx);
    let native_before = context::native_snapshot(&native).unwrap();
    let mut stage = Stage::from_parts(store, native, &ctx, &received(vec![], vec![])).unwrap();
    drain(&mut stage, &ctx);
    assert_eq!(source(&stage.store, &stage.native, &ctx), before);
    assert_eq!(
        context::native_snapshot(&stage.native).unwrap(),
        native_before
    );
    assert!(
        !stage
            .store
            .observations(&ctx.account, &ctx.session, ctx.epoch, GENERATION)
            .unwrap()
            .disconnected()[ID]
    );
    assert!(stage.repair_rows().unwrap().is_empty());
}

#[test]
fn required_repair_rejects_conflicting_duplicates_but_collapses_identical_rows() {
    for conflict in [false, true] {
        let (ctx, store, mut native) = seeded(row("Original", 101));
        native.get_mut(ID).unwrap().name = Some("Drift".into());
        let rows = vec![
            row("Original", 101),
            row(if conflict { "Different" } else { "Original" }, 101),
        ];
        let mut stage = Stage::from_parts(store, native, &ctx, &received(rows, vec![])).unwrap();
        drain(&mut stage, &ctx);
        if conflict {
            assert!(stage.repair_rows().is_err());
        } else {
            assert_eq!(stage.repair_rows().unwrap(), vec![row("Original", 101)]);
        }
    }
}
