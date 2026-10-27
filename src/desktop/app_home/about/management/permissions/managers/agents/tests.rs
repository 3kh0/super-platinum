use super::*;
fn raw(id: &str, app: &str) -> Value {
    json!({"id":id,"team_id":"TWORKFLOW","is_bot":true,
        "profile":{"real_name":"Manager agent","api_app_id":app}})
}
fn profile(app: &str, agent: bool) -> facts::Profile {
    protocol::manager_settings::eligibility::decode_profile(
        app,
        &json!({"ok":true,
        "app_profile":{"id":app,"is_agent_app":agent}}),
    )
    .unwrap()
}
#[test]
fn acquired_facts_project_only_exact_rows_and_generation() {
    let a = raw("UAGENT001", "AAGENT001");
    let b = raw("UBOT00001", "AOTHER001");
    let facts = Acquired {
        generation: 7,
        rows: vec![a.clone(), b.clone()],
        profiles: BTreeMap::from([("AAGENT001".into(), profile("AAGENT001", true))]),
        access: None,
    };
    assert!(facts.project(8, "TWORKFLOW").is_err());
    let rows = facts.project(7, "TWORKFLOW").unwrap();
    assert!(rows[0].eligible);
    assert!(!rows[1].eligible);
    assert_eq!(rows[0].raw, a);
    assert_eq!(rows[1].raw, b);
    assert!(
        facts
            .project(7, "TOTHER001")
            .unwrap()
            .iter()
            .all(|r| !r.eligible)
    );
}
#[test]
fn latest_raw_row_wins_before_facts_and_failed_batch_is_not_a_grant() {
    let old = raw("UAGENT001", "AOLD00001");
    let latest = raw("UAGENT001", "ANEW00001");
    let rows = distinct(vec![old, raw("UBOT00001", "ABOT00001"), latest.clone()]).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0], latest);
    let a = Acquired {
        generation: 1,
        rows,
        profiles: BTreeMap::from([("AOLD00001".into(), profile("AOLD00001", true))]),
        access: None,
    };
    assert!(!a.project(1, "TWORKFLOW").unwrap()[0].eligible);
}
#[test]
fn retry_and_completion_require_original_generation_and_preserve_raw_evidence() {
    let rows = vec![raw("UAGENT001", "AAGENT001")];
    let reply = json!({"ok":true,"app_profile":{"id":"WRONG","opaque":[1,"kept"]}});
    let mut a = Attempt {
        generation: 7,
        query: "agent".into(),
        rows: rows.clone(),
        hydrate: false,
        receipts: vec![reply.clone()],
        status: "loading",
    };
    assert!(a.loading(7));
    assert!(!a.loading(8));
    assert!(a.retry_rows(7).is_none());
    a.finish(8, false);
    assert!(a.loading(7));
    a.finish(7, false);
    assert_eq!(a.retry_rows(7), Some((rows.clone(), false)));
    assert!(a.retry_rows(8).is_none());
    assert_eq!(a.snapshot()["receipts"][0], reply);
    // A late old completion cannot turn a failed/retired batch into ready.
    a.finish(7, true);
    assert!(a.retry(7));
    assert_eq!(a.rows, rows);
}
#[test]
fn selected_acquired_agent_grant_reaches_named_run_union_only_in_this_context() {
    use protocol::run_settings::{Context, Draft, Entity, Kind, TargetPolicy};
    let mut context = Context {
        current_team_id: "TWORKFLOW".into(),
        scope_id: "TWORKFLOW".into(),
        unified_user: false,
        collaborators: vec!["UWORKFLOW".into()],
        available_external_ids: None,
        entities: vec![
            Entity {
                id: "TWORKFLOW".into(),
                kind: Kind::Team,
                enterprise_id: None,
                eligible: true,
            },
            Entity {
                id: "UWORKFLOW".into(),
                kind: Kind::User,
                enterprise_id: None,
                eligible: true,
            },
        ],
    };
    let mut draft = Draft::from_acquired(
        &json!({"ok":true,"permission_type":"named_entities","user_ids":["UWORKFLOW"],
        "channel_ids":[],"team_ids":[],"org_ids":[]}),
        &context,
    )
    .unwrap();
    let raw = raw("UAGENT001", "AAGENT001");
    let p = profile("AAGENT001", true);
    let row = source::with_facts(&raw, "TWORKFLOW", Some(&p), None).unwrap();
    assert!(!settings::source::row(&raw, &context).unwrap().0.eligible);
    assert!(!source::run_row(&row, &[], &context).unwrap().0.eligible);
    let selected = vec!["UWORKFLOW".into(), "UAGENT001".into()];
    let (entity, _) = source::run_row(&row, &selected, &context).unwrap();
    assert!(entity.eligible);
    context.entities.push(entity);
    context.collaborators = selected.clone();
    draft.union_managers(&selected, &context).unwrap();
    let policy = TargetPolicy::from_acquired(&json!({"ok":true,"prefs":null}), None).unwrap();
    assert!(draft.desired(&context, &policy).unwrap().is_some());
    // Removing the newly selected manager preserves its source-unioned run
    // grant, and does not require the ordinary bot-denying run picker.
    let remaining = vec!["UWORKFLOW".into()];
    let (retained, _) = source::run_row(&row, &remaining, &context).unwrap();
    assert!(retained.eligible);
    context.entities.retain(|e| e.id != retained.id);
    context.entities.push(retained);
    draft.union_managers(&remaining, &context).unwrap();
    assert!(draft.ids().iter().any(|id| id == "UAGENT001"));
    assert!(draft.desired(&context, &policy).unwrap().is_some());
    let denied = source::row(&raw, "TWORKFLOW").unwrap();
    assert!(
        !source::run_row(&denied, &selected, &context)
            .unwrap()
            .0
            .eligible
    );
}
