use super::*;
use crate::app_home::channel_fetches::{fixture, initial, members};
use serde_json::json;

fn setup() -> (ShellState, Owner) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut shell);
    let team = shell.core.active_team.clone().unwrap();
    let ctx = Arc::new(Context::capture(&shell, &team).unwrap());
    let owner = initial::begin(
        &mut shell,
        ctx,
        Arc::new(members::Transformations::decode(&json!({"ok":true,"assignments":{}})).unwrap()),
        true,
        false,
    )
    .unwrap();
    let ctx = &owner.context;
    let flags = shell.app_home.channel_fetches.slots[&ctx.key()]
        .transformations
        .flags
        .clone();
    let w = shell.core.workspaces.get_mut(&team).unwrap();
    while w
        .directory_members
        .has_pending(&ctx.account, &ctx.session)
        .unwrap()
    {
        let pending = w
            .directory_members
            .prepare(
                &ctx.account,
                &ctx.session,
                ctx.epoch,
                GENERATION,
                flags.clone(),
                &w.users,
            )
            .unwrap();
        assert!(pending.fields().iter().all(|s| s.is_ascii()));
        let reply = super_platinum_core::block_actions::workflow_management::member_hydration::RendererReply {
            generation: GENERATION.into(), operation: "ordinary-toLowerCase".into(),
            fields: pending.fields().to_vec(),
            lowered: pending.fields().iter().map(|v| v.to_ascii_lowercase()).collect(),
        };
        let (_, users) = w
            .directory_members
            .adopt(
                pending,
                reply,
                &flags,
                &ctx.account,
                &ctx.session,
                ctx.epoch,
                GENERATION,
                &w.users,
            )
            .unwrap()
            .into_parts();
        w.users = users;
    }
    (shell, owner)
}
fn init_raw(owner: &Owner) -> Value {
    json!({"ok":true,"self":{"id":owner.context.session.user_id,"name":"fixture-self","team_id":owner.context.session.team_id,"profile":{}},
        "team":{"id":owner.context.session.team_id,"name":"Current team"},
        "workspaces":[{"id":"TTHIRD","name":"Third"},{"id":"TSECOND","name":"Second","allow_external_users":false}],
        "prefs":{"workspace_filter_selected":"TSECOND"}})
}
fn dispatch(
    shell: &mut ShellState,
    owner: &Owner,
    kind: source::PayloadKind,
    raw: Value,
) -> Result<(), Error> {
    let ctx = &owner.context;
    let payload = source::Payload::observed(
        kind,
        source::Owner::capture(
            &ctx.account,
            &ctx.session,
            ctx.transport.d_cookie(),
            ctx.epoch,
        ),
        raw.to_string(),
    )
    .unwrap();
    let member = if kind == source::PayloadKind::Init {
        let w = &shell.core.workspaces[&ctx.session.team_id];
        assert!(
            !w.directory_members
                .has_pending(&ctx.account, &ctx.session)
                .unwrap()
        );
        let rows = json!([raw["self"]]);
        let before = serde_json::to_value(&w.users).unwrap();
        let observed = w
            .directory_members
            .observations(&ctx.account, &ctx.session, ctx.epoch, GENERATION)
            .unwrap();
        let mut store = w.directory_members.clone();
        let native = w.users.clone();
        let flags = shell.app_home.channel_fetches.slots[&ctx.key()]
            .transformations
            .flags
            .clone();
        store
            .enqueue(
                &ctx.account,
                &ctx.session,
                directory_members::Origin::DirectUpsert,
                rows.as_array().unwrap().clone(),
                json!({}),
            )
            .unwrap();
        let pending = store
            .prepare(
                &ctx.account,
                &ctx.session,
                ctx.epoch,
                GENERATION,
                flags.clone(),
                &native,
            )
            .unwrap();
        // Exact ASCII fixture fields only; no renderer/Unicode lowercase claim.
        assert!(pending.fields().iter().all(|s| s.is_ascii()));
        let reply=super_platinum_core::block_actions::workflow_management::member_hydration::RendererReply {
            generation:GENERATION.into(),operation:"ordinary-toLowerCase".into(),fields:pending.fields().to_vec(),
            lowered:pending.fields().iter().map(|v|v.to_ascii_lowercase()).collect(),
        };
        let (_, native) = store
            .adopt(
                pending,
                reply,
                &flags,
                &ctx.account,
                &ctx.session,
                ctx.epoch,
                GENERATION,
                &native,
            )
            .unwrap()
            .into_parts();
        Some(super::super::Member {
            rows,
            before,
            observed,
            store,
            native,
        })
    } else {
        None
    };
    super::super::dispatch(shell, owner, super::super::Prepared { payload, member })
}
fn initialize(shell: &mut ShellState, owner: &Owner) {
    dispatch(shell, owner, source::PayloadKind::Init, init_raw(owner)).unwrap();
    dispatch(
        shell,
        owner,
        source::PayloadKind::Extras,
        json!({"ok":true,"default_workspace":"TTHIRD"}),
    )
    .unwrap();
}
fn capture(shell: &ShellState, owner: &Owner) -> Result<Observation, Error> {
    Observation::capture(shell, &owner.context.session.team_id)
}

#[test]
fn home_context_raw_stores_and_default_empty_slot_are_not_acquisition() {
    let (mut shell, owner) = setup();
    let w = shell
        .core
        .workspaces
        .get_mut(&owner.context.session.team_id)
        .unwrap();
    w.cfm_payload.internal_workspace_ids = Some(json!([]));
    w.cfm_payload
        .user_prefs
        .insert("workspace_filter_selected".into(), json!("TSECOND"));
    w.cfm_payload.default_workspace = Some(json!("TTHIRD"));
    assert!(capture(&shell, &owner).is_err());
}

#[test]
fn home_context_actual_init_and_extras_bind_exact_order_values_and_team_rows() {
    let (mut shell, owner) = setup();
    dispatch(
        &mut shell,
        &owner,
        source::PayloadKind::Init,
        init_raw(&owner),
    )
    .unwrap();
    assert!(
        capture(&shell, &owner).is_err(),
        "Default has not been acquired"
    );
    dispatch(
        &mut shell,
        &owner,
        source::PayloadKind::Extras,
        json!({"ok":true,"default_workspace":"TTHIRD"}),
    )
    .unwrap();
    let observed = capture(&shell, &owner).unwrap();
    assert_eq!(observed.internal_workspace_ids(), ["TTHIRD", "TSECOND"]);
    assert_eq!(
        observed.workspace_filter_selected(),
        Some(&json!("TSECOND"))
    );
    assert_eq!(observed.default_workspace(), Some(&json!("TTHIRD")));
    assert_eq!(observed.rows()["TTHIRD"]["name"], json!("Third"));
    assert_eq!(
        observed.provenance()["internalWorkspaces"]["effect"]["kind"],
        json!("internal_workspaces")
    );
    assert!(observed.current(&shell));
}

#[test]
fn home_context_observed_empty_and_missing_optional_inputs_are_legitimate() {
    let (mut shell, owner) = setup();
    let mut raw = init_raw(&owner);
    raw["workspaces"] = json!([{"id":"TEXTERNAL","allow_external_users":true}]);
    raw.as_object_mut().unwrap().remove("prefs");
    dispatch(&mut shell, &owner, source::PayloadKind::Init, raw).unwrap();
    dispatch(
        &mut shell,
        &owner,
        source::PayloadKind::Extras,
        json!({"ok":true}),
    )
    .unwrap();
    let observed = capture(&shell, &owner).unwrap();
    assert!(observed.internal_workspace_ids().is_empty());
    assert!(observed.workspace_filter_selected().is_none());
    assert!(observed.default_workspace().is_none());
    assert_eq!(
        observed.provenance()["defaultWorkspace"]["observation"],
        json!("accepted-payload")
    );
}

#[test]
fn home_context_init_without_internal_effect_does_not_certify_empty_default() {
    let (mut shell, owner) = setup();
    let mut raw = init_raw(&owner);
    raw["workspaces"] = json!([]);
    dispatch(&mut shell, &owner, source::PayloadKind::Init, raw).unwrap();
    dispatch(
        &mut shell,
        &owner,
        source::PayloadKind::Extras,
        json!({"ok":true}),
    )
    .unwrap();
    assert!(capture(&shell, &owner).is_err());
}

#[test]
fn home_context_null_preference_is_not_missing_and_partial_merge_is_not_provenance() {
    let (mut shell, owner) = setup();
    let team = owner.context.session.team_id.clone();
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .cfm_payload
        .user_prefs
        .insert("workspace_filter_selected".into(), json!("TWARM"));
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .cfm_payload
        .default_workspace = Some(json!("TWARM"));
    let mut raw = init_raw(&owner);
    raw["prefs"] = json!({});
    dispatch(&mut shell, &owner, source::PayloadKind::Init, raw).unwrap();
    dispatch(
        &mut shell,
        &owner,
        source::PayloadKind::Extras,
        json!({"ok":true}),
    )
    .unwrap();
    assert!(capture(&shell, &owner).is_err());
    let mut raw = init_raw(&owner);
    raw["prefs"] = json!({"workspace_filter_selected":null});
    dispatch(&mut shell, &owner, source::PayloadKind::Init, raw).unwrap();
    assert!(
        capture(&shell, &owner).is_err(),
        "Unobserved warm default still refused"
    );
    dispatch(
        &mut shell,
        &owner,
        source::PayloadKind::Extras,
        json!({"ok":true,"default_workspace":"TTHIRD"}),
    )
    .unwrap();
    assert_eq!(
        capture(&shell, &owner).unwrap().workspace_filter_selected(),
        Some(&Value::Null)
    );
}

#[test]
fn home_context_provenance_survives_diagnostic_history_eviction() {
    let (mut shell, owner) = setup();
    initialize(&mut shell, &owner);
    let observed = capture(&shell, &owner).unwrap();
    for _ in 0..40 {
        dispatch(
            &mut shell,
            &owner,
            source::PayloadKind::Extras,
            json!({"ok":true}),
        )
        .unwrap();
    }
    let payload = &shell.app_home.channel_fetches.slots[&owner.context.key()].payload;
    assert_eq!(payload.settlements.len(), 32);
    assert!(
        payload
            .settlements
            .iter()
            .all(|r| r.kind() == source::PayloadKind::Extras)
    );
    assert!(observed.current(&shell));
    assert!(capture(&shell, &owner).is_ok());
}

#[test]
fn home_context_failed_init_does_not_grant_or_retain_complete_observation() {
    let (mut shell, owner) = setup();
    initialize(&mut shell, &owner);
    let observed = capture(&shell, &owner).unwrap();
    let mut raw = init_raw(&owner);
    raw["onboarding"] = json!({"user_data":5});
    assert!(dispatch(&mut shell, &owner, source::PayloadKind::Init, raw).is_err());
    assert!(!observed.current(&shell));
    assert!(capture(&shell, &owner).is_err());
}

#[test]
fn home_context_equal_source_republication_retires_old_witness() {
    let (mut shell, owner) = setup();
    initialize(&mut shell, &owner);
    let observed = capture(&shell, &owner).unwrap();
    dispatch(
        &mut shell,
        &owner,
        source::PayloadKind::Extras,
        json!({"ok":true,"default_workspace":"TTHIRD"}),
    )
    .unwrap();
    assert!(!observed.current(&shell));
    assert!(capture(&shell, &owner).is_ok());
}

#[test]
fn home_context_exact_store_and_team_mutation_retires_observation() {
    for change in 0..5 {
        let (mut shell, owner) = setup();
        initialize(&mut shell, &owner);
        let observed = capture(&shell, &owner).unwrap();
        let team = owner.context.session.team_id.clone();
        match change {
            0 => {
                shell
                    .core
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .cfm_payload
                    .internal_workspace_ids = Some(json!(["TSECOND", "TTHIRD"]))
            }
            1 => {
                shell
                    .core
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .cfm_payload
                    .user_prefs
                    .insert("workspace_filter_selected".into(), json!("TTHIRD"));
            }
            2 => {
                shell
                    .core
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .cfm_payload
                    .default_workspace = None
            }
            3 => {
                shell
                    .app_home
                    .channel_fetches
                    .slots
                    .get_mut(&owner.context.key())
                    .unwrap()
                    .payload
                    .workspace_ids = Some(vec![])
            }
            _ => {
                shell
                    .app_home
                    .channel_fetches
                    .slots
                    .get_mut(&owner.context.key())
                    .unwrap()
                    .payload
                    .teams
                    .insert("TTHIRD".into(), json!({"id":"TTHIRD","name":"Changed"}));
            }
        }
        assert!(!observed.current(&shell));
    }
}

#[test]
fn home_context_account_credentials_transport_and_socket_retire() {
    for change in 0..5 {
        let (mut shell, owner) = setup();
        initialize(&mut shell, &owner);
        let observed = capture(&shell, &owner).unwrap();
        let team = owner.context.session.team_id.clone();
        match change {
            0 => shell.core.active_account = Some("replacement".into()),
            1 => shell.core.account_epoch += 1,
            2 => shell
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut(&team)
                .unwrap()
                .token
                .push_str("-replaced"),
            3 => {
                shell.core.transport = Some(Arc::new(Transport::disconnected_fixture(
                    owner.context.transport.d_cookie(),
                )))
            }
            _ => shell.core.workspaces.get_mut(&team).unwrap().rt_generation += 1,
        }
        assert!(!observed.current(&shell));
        assert!(capture(&shell, &owner).is_err());
    }
}

#[test]
fn home_context_reinit_cannot_recertify_retained_effects() {
    let (mut shell, owner) = setup();
    initialize(&mut shell, &owner);
    let observed = capture(&shell, &owner).unwrap();
    initial::retire(&mut shell, &owner);
    let replacement = initial::begin(
        &mut shell,
        owner.context.clone(),
        Arc::new(members::Transformations::decode(&json!({"ok":true,"assignments":{}})).unwrap()),
        false,
        false,
    )
    .unwrap();
    assert!(!observed.current(&shell));
    assert!(capture(&shell, &replacement).is_err());
    dispatch(
        &mut shell,
        &replacement,
        source::PayloadKind::Init,
        init_raw(&replacement),
    )
    .unwrap();
    assert!(
        capture(&shell, &replacement).is_err(),
        "Old default receipt cannot adopt new owner"
    );
    dispatch(
        &mut shell,
        &replacement,
        source::PayloadKind::Extras,
        json!({"ok":true,"default_workspace":"TTHIRD"}),
    )
    .unwrap();
    assert!(capture(&shell, &replacement).unwrap().current(&shell));
}
