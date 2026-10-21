use super::*;
use serde_json::json;

#[test]
fn picker_filters_do_not_revoke_original_canonical_grants() {
    let mut ctx = context();
    ctx.entities
        .iter_mut()
        .find(|e| e.id == C1)
        .unwrap()
        .eligible = false;
    let mut original = raw("named_entities");
    original["channel_ids"] = json!([C1]);
    let retained = Draft::from_acquired(&original, &ctx).unwrap();
    assert!(retained.desired(&ctx, &policy()).unwrap().is_none());
    let mut added = Draft::from_acquired(&raw("everyone"), &ctx).unwrap();
    added.select_mode(Mode::NamedEntities, &ctx).unwrap();
    added
        .replace_internal(vec![Selection::Resolved(C1.into())], &ctx)
        .unwrap();
    assert_eq!(added.desired(&ctx, &policy()), Err(Error::Ineligible));
    ctx.entities.retain(|e| e.id != C1);
    assert_eq!(retained.desired(&ctx, &policy()), Err(Error::MissingEntity));
}

const U1: &str = "U00000001";
const U2: &str = "W00000002";
const C1: &str = "C00000001";
const T1: &str = "T00000001";
const T2: &str = "T00000002";
const T3: &str = "T00000003";
const E1: &str = "E00000001";
const E3: &str = "E00000003";
fn entity(id: &str, kind: Kind, enterprise: Option<&str>) -> Entity {
    Entity {
        id: id.into(),
        kind,
        enterprise_id: enterprise.map(str::to_owned),
        eligible: true,
    }
}
fn context() -> Context {
    Context {
        current_team_id: T1.into(),
        scope_id: E1.into(),
        unified_user: false,
        entities: vec![
            entity(U1, Kind::User, None),
            entity(U2, Kind::User, None),
            entity(C1, Kind::Channel, None),
            entity(T1, Kind::Team, Some(E1)),
            entity(T2, Kind::Team, Some(E1)),
            entity(T3, Kind::Team, Some(E3)),
            entity(E1, Kind::Org, None),
            entity(E3, Kind::Org, None),
        ],
        collaborators: vec![U1.into()],
        available_external_ids: Some(vec![T3.into(), E3.into()]),
    }
}
fn raw(mode: &str) -> Value {
    json!({"ok":true,"permission_type":mode,"opaque":{"keep":false}})
}
fn policy() -> TargetPolicy {
    TargetPolicy::from_acquired(
        &json!({"ok":true,"trippable_by_slack_connected_teams":{
        "hermes_triggers_trippable_by_slack_connected_teams_mcp":"SAFE","restricted_teams":[]}}),
        Some(&json!({"ok":true,"is_restricted":false,"restricted_by_connector_steps_only":false})),
    )
    .unwrap()
}
fn desired(draft: &Draft, ctx: &Context) -> Command {
    draft.desired(ctx, &policy()).unwrap().unwrap()
}

#[test]
fn manager_union_keeps_manager_first_wire_order_and_original_set_baseline() {
    let mut ctx = context();
    let new = "UMANNEW01";
    ctx.entities.push(entity(new, Kind::User, None));
    let mut initial = raw("named_entities");
    initial["user_ids"] = json!([U1, U2]);
    initial["channel_ids"] = json!([C1]);
    initial["team_ids"] = json!([T3]);
    let mut draft = Draft::from_acquired(&initial, &ctx).unwrap();
    let baseline = draft.original.clone();
    draft
        .union_managers(&[U1.into(), new.into(), U1.into()], &ctx)
        .unwrap();
    assert_eq!(draft.ids(), [U1, new, U2, C1, T3]);
    assert_eq!(draft.original, baseline);
    assert!(draft.external_enabled());
    let command = desired(&draft, &ctx);
    // Exercise the actual existing RequestPlan wire builder, not just ids().
    let mut load = crate::block_actions::workflow_management::EditorLoad::new("WF1").unwrap();
    use crate::block_actions::workflow_management::{MutationPlan, ReadKind};
    for (kind, raw) in [
        (
            ReadKind::Latest,
            json!({"ok":true,"decorated_workflow":{"id":"WF1","app_id":"A1","steps":[]}}),
        ),
        (
            ReadKind::Triggers,
            json!({"ok":true,"triggers":[{"id":"TR1"}]}),
        ),
        (
            ReadKind::TriggerInfo,
            json!({"ok":true,"trigger":{"id":"TR1","workflow":{"workflow_id":"WF1","app_id":"A1"}}}),
        ),
        (ReadKind::Preferences, json!({"ok":true})),
        (
            ReadKind::CopyPermissions,
            json!({"ok":true,"permission_type":"collaborators"}),
        ),
        (ReadKind::RunPermissions, initial),
    ] {
        load.accept(kind, raw).unwrap();
    }
    let plan = MutationPlan::new(&load.editor().unwrap(), command).unwrap();
    let session = crate::config::WorkspaceSession {
        team_id: T1.into(),
        user_id: U1.into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    let request = plan.slots()[0]
        .plan()
        .request(&crate::slack::SlackClient::default(), &session);
    let crate::slack::client::RequestBody::Form(fields) = request.body else {
        panic!("actual form required")
    };
    assert!(fields.contains(&("user_ids".into(), format!("{U1},{new},{U2}"))));
    assert!(fields.contains(&("channel_ids".into(), C1.into())));
    assert!(fields.contains(&("team_ids".into(), T3.into())));
    // Removing a manager never subtracts its prior run target.
    draft.union_managers(&[U1.into()], &ctx).unwrap();
    assert_eq!(draft.ids(), [U1, new, U2, C1, T3]);
}

#[test]
fn manager_order_only_remains_noop_and_general_picker_order_is_unchanged() {
    let ctx = context();
    let mut initial = raw("named_entities");
    initial["user_ids"] = json!([U1, U2]);
    let original = Draft::from_acquired(&initial, &ctx).unwrap();
    let mut managers = original.clone();
    managers
        .union_managers(&[U2.into(), U1.into()], &ctx)
        .unwrap();
    assert_eq!(managers.ids(), [U2, U1]);
    assert_eq!(managers.desired(&ctx, &policy()), Ok(None));
    let mut picker = original;
    picker
        .replace_internal(
            vec![
                Selection::Resolved(U2.into()),
                Selection::Resolved(U1.into()),
            ],
            &ctx,
        )
        .unwrap();
    assert_eq!(picker.ids(), [U1, U2]);
    assert_eq!(picker.desired(&ctx, &policy()), Ok(None));
    for mode in ["everyone", "app_collaborators"] {
        let mut draft = Draft::from_acquired(&raw(mode), &ctx).unwrap();
        let before = draft.clone();
        draft.union_managers(&[U1.into()], &ctx).unwrap();
        assert_eq!(draft, before);
    }
}

#[test]
fn manager_union_retains_unresolved_and_canonical_eligibility_and_count_guards() {
    let mut ctx = context();
    let mut initial = raw("named_entities");
    initial["user_ids"] = json!([U1]);
    let original = Draft::from_acquired(&initial, &ctx).unwrap();
    let mut bad = original.clone();
    assert_eq!(
        bad.union_managers(&["UMISSING1".into()], &ctx),
        Err(Error::MissingEntity)
    );
    assert_eq!(bad, original);
    assert_eq!(bad.union_managers(&[C1.into()], &ctx), Err(Error::Invalid));
    assert_eq!(bad, original);
    ctx.entities
        .iter_mut()
        .find(|e| e.id == U2)
        .unwrap()
        .eligible = false;
    bad.union_managers(&[U1.into(), U2.into()], &ctx).unwrap();
    assert_eq!(bad.desired(&ctx, &policy()), Err(Error::Ineligible));
    let mut unresolved = original.clone();
    unresolved
        .replace_internal(
            vec![
                Selection::Resolved(U1.into()),
                Selection::Unresolved("unfinished".into()),
            ],
            &ctx,
        )
        .unwrap();
    unresolved.union_managers(&[U1.into()], &ctx).unwrap();
    assert_eq!(unresolved.unresolved(), ["unfinished"]);
    assert_eq!(unresolved.desired(&ctx, &policy()), Err(Error::Unresolved));
    let ids: Vec<String> = (100..201).map(|n| format!("U{n:08}")).collect();
    ctx.entities
        .extend(ids.iter().map(|id| entity(id, Kind::User, None)));
    let mut over = original;
    over.union_managers(&ids, &ctx).unwrap();
    assert_eq!(over.desired(&ctx, &policy()), Err(Error::TooMany));
}

#[test]
fn source_mode_transitions_seed_managers_preserve_external_and_clear_unresolved() {
    let ctx = context();
    let mut initial = raw("everyone");
    initial["team_ids"] = json!([T3]);
    let mut d = Draft::from_acquired(&initial, &ctx).unwrap();
    d.select_mode(Mode::NamedEntities, &ctx).unwrap();
    assert_eq!(d.ids(), [U1, T3]);
    d.replace_internal(vec![Selection::Unresolved("unmatched query".into())], &ctx)
        .unwrap();
    assert_eq!(d.ids(), [T3]);
    d.select_mode(Mode::NamedEntities, &ctx).unwrap();
    assert_eq!(d.unresolved(), ["unmatched query"]);
    d.select_mode(Mode::Everyone, &ctx).unwrap();
    assert_eq!(d.ids(), [T3, E3]);
    assert!(d.unresolved().is_empty());
    d.select_mode(Mode::AppCollaborators, &ctx).unwrap();
    assert!(d.ids().is_empty());
    assert!(!d.external_enabled());
    d.select_mode(Mode::NamedEntities, &ctx).unwrap();
    assert_eq!(d.ids(), [U1]);
    d.select_mode(Mode::Everyone, &ctx).unwrap();
    assert!(d.ids().is_empty());
}

#[test]
fn available_external_list_is_observed_not_inferred_and_first99_preserved() {
    let mut ctx = context();
    let mut r = raw("named_entities");
    r["team_ids"] = json!([T3]);
    let mut d = Draft::from_acquired(&r, &ctx).unwrap();
    let before = d.clone();
    ctx.available_external_ids = None;
    assert_eq!(
        d.select_mode(Mode::Everyone, &ctx),
        Err(Error::MissingAvailable)
    );
    assert_eq!(d, before);
    let ids: Vec<String> = (0..105).map(|i| format!("T{i:08}")).collect();
    ctx.available_external_ids = Some(ids.clone());
    d.select_mode(Mode::Everyone, &ctx).unwrap();
    assert_eq!(d.ids(), &ids[..99]);
    ctx.available_external_ids = Some(vec![]);
    d.select_mode(Mode::NamedEntities, &ctx).unwrap_err(); // selected team facts are still unobserved
}

#[test]
fn default_everyone_requires_nonempty_exact_current_internal_partition() {
    let ctx = context();
    for (teams, users, expected) in [
        (vec![T1, T3], vec![], Mode::Everyone),
        (vec![T3], vec![], Mode::NamedEntities),
        (vec![T1, T2], vec![], Mode::NamedEntities),
        (vec![T1], vec![U1], Mode::NamedEntities),
        (vec![], vec![], Mode::NamedEntities),
    ] {
        let mut r = raw("named_entities");
        r["team_ids"] = json!(teams);
        r["user_ids"] = json!(users);
        assert_eq!(Draft::from_acquired(&r, &ctx).unwrap().mode(), expected);
    }
}

#[test]
fn replacement_uses_source_ordered_union_difference_and_preserves_opposite_subset() {
    let ctx = context();
    let mut r = raw("named_entities");
    r["user_ids"] = json!([U1]);
    r["team_ids"] = json!([T3]);
    let mut d = Draft::from_acquired(&r, &ctx).unwrap();
    d.replace_internal(
        vec![
            Selection::Resolved(U2.into()),
            Selection::Resolved(C1.into()),
        ],
        &ctx,
    )
    .unwrap();
    assert_eq!(d.ids(), [T3, U2, C1]);
    d.replace_external(vec![E3.into()], &ctx).unwrap();
    assert_eq!(d.ids(), [U2, C1, E3]);
    let before = d.clone();
    assert_eq!(
        d.replace_internal(vec![Selection::Resolved(T3.into())], &ctx),
        Err(Error::Invalid)
    );
    assert_eq!(d, before);
    assert_eq!(
        d.replace_external(vec![T2.into()], &ctx),
        Err(Error::Invalid)
    );
    assert_eq!(d, before);
    d.toggle_external(false, &ctx).unwrap();
    assert_eq!(d.ids(), [U2, C1]);
    assert!(!d.external_enabled());
}

#[test]
fn external_off_removes_current_row_only_in_everyone_mode() {
    let ctx = context();
    let mut r = raw("named_entities");
    r["team_ids"] = json!([T1, T3]);
    let mut d = Draft::from_acquired(&r, &ctx).unwrap();
    assert_eq!(d.mode(), Mode::Everyone);
    d.toggle_external(false, &ctx).unwrap();
    assert!(d.ids().is_empty());
    r["user_ids"] = json!([U1]);
    let mut d = Draft::from_acquired(&r, &ctx).unwrap();
    d.toggle_external(false, &ctx).unwrap();
    assert_eq!(d.ids(), [U1, T1]);
}

#[test]
fn everyone_external_injects_actual_current_row_kind_not_enterprise_presence() {
    let mut ctx = context();
    let mut d = Draft::from_acquired(&raw("app_collaborators"), &ctx).unwrap();
    d.select_mode(Mode::Everyone, &ctx).unwrap();
    d.replace_external(vec![T3.into()], &ctx).unwrap();
    assert_eq!(
        desired(&d, &ctx),
        Command::RunPermission {
            permission_type: "named_entities".into(),
            user_ids: None,
            channel_ids: None,
            team_ids: Some(vec![T1.into(), T3.into()]),
            org_ids: None
        }
    );
    ctx.current_team_id = E1.into();
    let mut d = Draft::from_acquired(&raw("app_collaborators"), &ctx).unwrap();
    d.select_mode(Mode::Everyone, &ctx).unwrap();
    d.replace_external(vec![T3.into()], &ctx).unwrap();
    assert_eq!(
        desired(&d, &ctx),
        Command::RunPermission {
            permission_type: "named_entities".into(),
            user_ids: None,
            channel_ids: None,
            team_ids: Some(vec![T3.into()]),
            org_ids: Some(vec![E1.into()])
        }
    );
}

#[test]
fn canonical_affiliation_and_unified_org_getter_control_partition() {
    let mut ctx = context();
    assert!(!ctx.is_external(T2).unwrap());
    assert!(ctx.is_external(T3).unwrap());
    ctx.entities
        .iter_mut()
        .find(|e| e.id == E3)
        .unwrap()
        .enterprise_id = Some(E1.into());
    assert!(!ctx.is_external(E3).unwrap());
    ctx.unified_user = true;
    assert!(ctx.is_external(E3).unwrap());
    ctx.entities.retain(|e| e.id != T3);
    assert_eq!(ctx.is_external(T3), Err(Error::MissingEntity));
    assert!(!ctx.is_external(U1).unwrap());
}

#[test]
fn all_four_named_categories_are_retained_and_empty_wire_partitions_omitted() {
    let ctx = context();
    let mut d = Draft::from_acquired(&raw("app_collaborators"), &ctx).unwrap();
    d.select_mode(Mode::NamedEntities, &ctx).unwrap();
    d.replace_internal(
        vec![
            Selection::Resolved(U2.into()),
            Selection::Resolved(C1.into()),
            Selection::Resolved(T2.into()),
        ],
        &ctx,
    )
    .unwrap();
    d.replace_external(vec![E3.into()], &ctx).unwrap();
    assert_eq!(
        desired(&d, &ctx),
        Command::RunPermission {
            permission_type: "named_entities".into(),
            user_ids: Some(vec![U2.into()]),
            channel_ids: Some(vec![C1.into()]),
            team_ids: Some(vec![T2.into()]),
            org_ids: Some(vec![E3.into()])
        }
    );
    d.replace_internal(vec![], &ctx).unwrap();
    d.replace_external(vec![], &ctx).unwrap();
    assert_eq!(d.desired(&ctx, &policy()), Err(Error::EmptyNamed));
}

#[test]
fn count100_101_uses_original_concatenation_without_deduplication() {
    let ctx = context();
    for count in [100, 101] {
        let mut r = raw("named_entities");
        r["user_ids"] = json!(vec![U1; count]);
        let d = Draft::from_acquired(&r, &ctx).unwrap();
        assert_eq!(d.ids().len(), count);
        assert_eq!(
            d.desired(&ctx, &policy()),
            if count == 100 {
                Ok(None)
            } else {
                Err(Error::TooMany)
            }
        );
    }
}

#[test]
fn unresolved_and_missing_ineligible_canonical_candidates_cannot_save() {
    let mut ctx = context();
    let mut d = Draft::from_acquired(&raw("app_collaborators"), &ctx).unwrap();
    d.select_mode(Mode::NamedEntities, &ctx).unwrap();
    d.replace_internal(
        vec![
            Selection::Resolved(U1.into()),
            Selection::Unresolved("not resolved".into()),
        ],
        &ctx,
    )
    .unwrap();
    assert_eq!(d.desired(&ctx, &policy()), Err(Error::Unresolved));
    d.replace_internal(vec![Selection::Resolved(U1.into())], &ctx)
        .unwrap();
    ctx.entities
        .iter_mut()
        .find(|e| e.id == U1)
        .unwrap()
        .eligible = false;
    assert_eq!(d.desired(&ctx, &policy()), Err(Error::Ineligible));
    ctx.entities.retain(|e| e.id != U1);
    assert_eq!(d.desired(&ctx, &policy()), Err(Error::MissingEntity));
}

#[test]
fn lexical_classification_does_not_grant_candidate_eligibility() {
    let mut ctx = context();
    ctx.entities.push(entity("D00000001", Kind::Channel, None));
    ctx.entities.last_mut().unwrap().eligible = false;
    let mut d = Draft::from_acquired(&raw("everyone"), &ctx).unwrap();
    d.select_mode(Mode::NamedEntities, &ctx).unwrap();
    d.replace_internal(vec![Selection::Resolved("D00000001".into())], &ctx)
        .unwrap();
    assert_eq!(d.desired(&ctx, &policy()), Err(Error::Ineligible));
    ctx.entities.last_mut().unwrap().kind = Kind::User;
    assert_eq!(ctx.validate(), Err(Error::Invalid));
    assert_eq!(kind("@U00000001"), Ok(Kind::User));
    assert_eq!(kind("@C00000001"), Err(Error::Invalid));
    assert_eq!(kind("USLACK"), Err(Error::Invalid));
}

#[test]
fn source_noop_compares_mode_and_all_sets_before_everyone_wire_conversion() {
    let ctx = context();
    let mut r = raw("named_entities");
    r["user_ids"] = json!([U1, U2]);
    let mut d = Draft::from_acquired(&r, &ctx).unwrap();
    d.replace_internal(
        vec![
            Selection::Resolved(U2.into()),
            Selection::Resolved(U1.into()),
        ],
        &ctx,
    )
    .unwrap();
    assert_eq!(d.desired(&ctx, &TargetPolicy::unobserved()), Ok(None));
    // Displayed everyone differs from original named even though resulting wire
    // grant matches; the source changed-mode check still dispatches that Save.
    r["user_ids"] = json!([]);
    r["team_ids"] = json!([T1, T3]);
    let d = Draft::from_acquired(&r, &ctx).unwrap();
    assert_eq!(d.mode(), Mode::Everyone);
    assert!(d.desired(&ctx, &policy()).unwrap().is_some());
}

#[test]
fn malformed_acquisition_or_canonical_context_is_never_a_default() {
    let ctx = context();
    for r in [
        json!({}),
        json!({"ok":false,"permission_type":"everyone"}),
        json!({"ok":true,"permission_type":"other"}),
        json!({"ok":true,"permission_type":"named_entities","user_ids":null}),
        json!({"ok":true,"permission_type":"named_entities","user_ids":[T1]}),
    ] {
        assert!(Draft::from_acquired(&r, &ctx).is_err());
    }
    let mut duplicate = ctx.clone();
    duplicate.entities.push(duplicate.entities[0].clone());
    assert_eq!(duplicate.validate(), Err(Error::Invalid));
    let mut missing = ctx;
    missing.entities.retain(|e| e.id != T1);
    assert_eq!(missing.validate(), Err(Error::MissingEntity));
}

#[test]
fn external_target_policy_truth_table_matches_y54t() {
    let ids = ["allowed", "connector", "restricted", "other"];
    for (restricted, connectors, expected) in [
        (
            false,
            false,
            [
                [true, true, false, false],
                [true, true, false, true],
                [true, true, false, true],
            ],
        ),
        (
            true,
            true,
            [
                [false, true, false, false],
                [false, true, false, false],
                [false, true, false, true],
            ],
        ),
        (true, false, [[false; 4]; 3]),
    ] {
        for (index, mode) in ["NONE", "SAFE", "SAFE_AND_3P"].iter().enumerate() {
            let p=TargetPolicy::from_acquired(&json!({"ok":true,"trippable_by_slack_connected_teams":{
                "hermes_triggers_trippable_by_slack_connected_teams_mcp":mode,"allowed_teams":["allowed"],
                "allowed_teams_with_3p_connectors":["connector"],"restricted_teams":["restricted"]}}),
                Some(&json!({"ok":true,"is_restricted":restricted,"restricted_by_connector_steps_only":connectors}))).unwrap();
            for (i, id) in ids.iter().enumerate() {
                assert_eq!(
                    p.allows_external(id),
                    Ok(expected[index][i]),
                    "{restricted}/{connectors}/{mode}/{id}"
                );
            }
        }
    }
}

#[test]
fn observed_missing_preferences_and_missing_observation_are_distinct() {
    let restriction =
        json!({"ok":true,"is_restricted":true,"restricted_by_connector_steps_only":false});
    for prefs in [
        json!({"ok":true}),
        json!({"ok":true,"trippable_by_slack_connected_teams":null}),
    ] {
        let p = TargetPolicy::from_acquired(&prefs, Some(&restriction)).unwrap();
        assert_eq!(p.any_external(), Ok(false));
        assert_eq!(p.allows_external(T3), Ok(true));
        assert_eq!(p.external_checkbox_enabled(), Ok(false));
        assert_eq!(
            TargetPolicy::from_acquired(&prefs, None)
                .unwrap()
                .allows_external(T3),
            Err(Error::MissingPolicy)
        );
    }
    assert_eq!(
        TargetPolicy::unobserved().allows_external(T3),
        Err(Error::MissingPolicy)
    );
}

#[test]
fn unknown_enum_optional_nulls_and_checkbox_follow_source_branches() {
    let restricted =
        json!({"ok":true,"is_restricted":true,"restricted_by_connector_steps_only":true});
    for mode in ["NONE", "SAFE", "SAFE_AND_3P", "FUTURE"] {
        let p = TargetPolicy::from_acquired(
            &json!({"ok":true,"trippable_by_slack_connected_teams":{
            "hermes_triggers_trippable_by_slack_connected_teams_mcp":mode,"allowed_teams":null,
            "restricted_teams":null,"allowed_teams_with_3p_connectors":null}}),
            Some(&restricted),
        )
        .unwrap();
        assert_eq!(
            p.allows_external(T3),
            Ok(matches!(mode, "SAFE_AND_3P" | "FUTURE"))
        );
        assert_eq!(p.external_checkbox_enabled(), Ok(mode == "SAFE_AND_3P"));
        assert_eq!(p.any_external(), Ok(mode != "NONE"));
    }
    assert!(
        TargetPolicy::from_acquired(
            &json!({"ok":true,"trippable_by_slack_connected_teams":false}),
            Some(&restricted)
        )
        .is_err()
    );
    assert!(TargetPolicy::from_acquired(&json!({"ok":true}), Some(&json!({"ok":true}))).is_err());
}

#[test]
fn internal_mutation_does_not_invent_or_require_external_policy() {
    let ctx = context();
    let mut d = Draft::from_acquired(&raw("app_collaborators"), &ctx).unwrap();
    d.select_mode(Mode::NamedEntities, &ctx).unwrap();
    assert!(
        d.desired(&ctx, &TargetPolicy::unobserved())
            .unwrap()
            .is_some()
    );
    d.replace_external(vec![T3.into()], &ctx).unwrap();
    assert_eq!(
        d.desired(&ctx, &TargetPolicy::unobserved()),
        Err(Error::MissingPolicy)
    );
}

#[test]
fn run_ack_checks_all_effective_lists_not_just_mode_or_ok() {
    let plan = super::super::RequestPlan::new(
        "workflows.triggers.permissions.set",
        "set-trigger-run-permission",
        json!({"trigger_id":"TR1","permission_type":"named_entities","user_ids":[U1,U2]}),
    );
    let ack = json!({"ok":true,"permission_type":"named_entities","user_ids":[U2,U1],"opaque":{"keep":7}});
    assert!(acknowledges(&plan, &ack));
    for key in ["channel_ids", "team_ids", "org_ids"] {
        let mut extra = ack.clone();
        extra[key] = json!(["UNEXPECTED"]);
        assert!(!acknowledges(&plan, &extra));
        extra[key] = json!([]);
        assert!(acknowledges(&plan, &extra));
        extra[key] = Value::Null;
        assert!(!acknowledges(&plan, &extra));
    }
    for (key, value) in [
        ("permission_type", json!("everyone")),
        ("ok", json!(false)),
        ("error", json!("denied")),
        ("user_ids", json!([U1])),
    ] {
        let mut bad = ack.clone();
        bad[key] = value;
        assert!(!acknowledges(&plan, &bad));
    }
}
