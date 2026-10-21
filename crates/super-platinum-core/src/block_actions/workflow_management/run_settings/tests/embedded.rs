use super::*;

fn command(draft: &Draft, ctx: &Context) -> Command {
    draft.desired_embedded(ctx, &policy()).unwrap().unwrap()
}

#[test]
fn acquired_everyone_normalization_reaches_concat_duplicate_without_fabricated_draft() {
    let ctx = context();
    let mut initial = raw("named_entities");
    initial["team_ids"] = json!([T1, T3]);
    initial["org_ids"] = json!([E3]);
    let draft = Draft::from_acquired(&initial, &ctx).unwrap();
    assert_eq!(draft.mode(), Mode::Everyone);
    assert_eq!(draft.ids(), [T1, T3, E3]);
    let before = draft.clone();
    let embedded = command(&draft, &ctx);
    assert_eq!(
        embedded,
        Command::RunPermission {
            permission_type: "named_entities".into(),
            user_ids: None,
            channel_ids: None,
            team_ids: Some(vec![T1.into(), T1.into(), T3.into()]),
            org_ids: Some(vec![E3.into()]),
        }
    );
    assert_eq!(
        desired(&draft, &ctx),
        Command::RunPermission {
            permission_type: "named_entities".into(),
            user_ids: None,
            channel_ids: None,
            team_ids: Some(vec![T1.into(), T3.into()]),
            org_ids: Some(vec![E3.into()]),
        }
    );
    assert_eq!(draft, before);
    // Actual existing mutation/request planning must preserve the concat array.
    use crate::block_actions::workflow_management::{EditorLoad, MutationPlan, ReadKind};
    let mut load = EditorLoad::new("WF1").unwrap();
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
    let plan = MutationPlan::new(&load.editor().unwrap(), embedded).unwrap();
    assert_eq!(
        plan.slots()[0].plan().arguments()["team_ids"],
        json!([T1, T1, T3])
    );
    assert_eq!(plan.slots()[0].plan().arguments()["org_ids"], json!([E3]));
}

#[test]
fn embedded_scope_kind_and_external_partition_use_canonical_context() {
    let mut ctx = context();
    ctx.current_team_id = E1.into();
    ctx.unified_user = true;
    let mut initial = raw("named_entities");
    initial["team_ids"] = json!([T3]);
    initial["org_ids"] = json!([E1, E3]);
    let draft = Draft::from_acquired(&initial, &ctx).unwrap();
    assert_eq!(draft.mode(), Mode::Everyone);
    assert_eq!(
        command(&draft, &ctx),
        Command::RunPermission {
            permission_type: "named_entities".into(),
            user_ids: None,
            channel_ids: None,
            team_ids: Some(vec![T3.into()]),
            org_ids: Some(vec![E1.into(), E1.into(), E3.into()]),
        }
    );
    let mut fresh = Draft::from_acquired(&raw("app_collaborators"), &ctx).unwrap();
    fresh.select_mode(Mode::Everyone, &ctx).unwrap();
    fresh
        .replace_external(vec![T3.into(), E3.into()], &ctx)
        .unwrap();
    // No existing current scope means union and concat have the same result.
    assert_eq!(
        fresh.desired_embedded(&ctx, &policy()),
        fresh.desired(&ctx, &policy())
    );
    assert_eq!(
        command(&fresh, &ctx),
        Command::RunPermission {
            permission_type: "named_entities".into(),
            user_ids: None,
            channel_ids: None,
            team_ids: Some(vec![T3.into()]),
            org_ids: Some(vec![E1.into(), E3.into()]),
        }
    );
    assert_eq!(
        fresh.replace_external(vec![T2.into()], &ctx),
        Err(Error::Invalid)
    );
    let mut malformed = ctx.clone();
    malformed
        .entities
        .iter_mut()
        .find(|e| e.id == E1)
        .unwrap()
        .kind = Kind::Team;
    assert_eq!(
        draft.desired_embedded(&malformed, &policy()),
        Err(Error::Invalid)
    );
}

#[test]
fn embedded_preserves_changed_only_and_named_manager_union() {
    let ctx = context();
    for mode in ["everyone", "app_collaborators"] {
        let draft = Draft::from_acquired(&raw(mode), &ctx).unwrap();
        assert_eq!(draft.desired_embedded(&ctx, &policy()), Ok(None));
    }
    let mut initial = raw("named_entities");
    initial["user_ids"] = json!([U1, U2]);
    initial["channel_ids"] = json!([C1]);
    initial["team_ids"] = json!([T2, T3]);
    let mut draft = Draft::from_acquired(&initial, &ctx).unwrap();
    draft.union_managers(&[U2.into(), U1.into()], &ctx).unwrap();
    assert_eq!(draft.ids(), [U2, U1, C1, T2, T3]);
    assert_eq!(draft.desired_embedded(&ctx, &policy()), Ok(None));
    let mut ctx = ctx;
    let new = "UMANNEW01";
    ctx.entities.push(entity(new, Kind::User, None));
    draft
        .union_managers(&[new.into(), U2.into()], &ctx)
        .unwrap();
    assert_eq!(
        draft.desired_embedded(&ctx, &policy()),
        draft.desired(&ctx, &policy())
    );
    assert_eq!(
        command(&draft, &ctx),
        Command::RunPermission {
            permission_type: "named_entities".into(),
            user_ids: Some(vec![new.into(), U2.into(), U1.into()]),
            channel_ids: Some(vec![C1.into()]),
            team_ids: Some(vec![T2.into(), T3.into()]),
            org_ids: None,
        }
    );
}

#[test]
fn embedded_count_is_source_selection_count_before_scope_concat() {
    let ctx = context();
    for count in [100, 101] {
        let mut initial = raw("named_entities");
        initial["team_ids"] = json!(
            std::iter::once(T1)
                .chain(std::iter::repeat_n(T3, count - 1))
                .collect::<Vec<_>>()
        );
        let draft = Draft::from_acquired(&initial, &ctx).unwrap();
        assert_eq!(draft.ids().len(), count);
        if count == 101 {
            assert_eq!(draft.desired_embedded(&ctx, &policy()), Err(Error::TooMany));
        } else {
            let Command::RunPermission {
                team_ids: Some(ids),
                ..
            } = command(&draft, &ctx)
            else {
                panic!("team grant")
            };
            // fkuW validates en.length before adding current scope, no post-concat cap.
            assert_eq!(ids.len(), 101);
            assert_eq!(&ids[..2], &[T1, T1]);
            assert!(ids[2..].iter().all(|id| id == T3));
        }
    }
}

#[test]
fn embedded_does_not_bypass_unresolved_missing_ineligible_or_external_policy() {
    let mut ctx = context();
    let mut draft = Draft::from_acquired(&raw("app_collaborators"), &ctx).unwrap();
    draft.select_mode(Mode::NamedEntities, &ctx).unwrap();
    draft
        .replace_internal(vec![Selection::Unresolved("unresolved".into())], &ctx)
        .unwrap();
    assert_eq!(
        draft.desired_embedded(&ctx, &policy()),
        Err(Error::Unresolved)
    );
    draft
        .replace_internal(vec![Selection::Resolved(U2.into())], &ctx)
        .unwrap();
    ctx.entities
        .iter_mut()
        .find(|e| e.id == U2)
        .unwrap()
        .eligible = false;
    assert_eq!(
        draft.desired_embedded(&ctx, &policy()),
        Err(Error::Ineligible)
    );
    ctx.entities.retain(|e| e.id != U2);
    assert_eq!(
        draft.desired_embedded(&ctx, &policy()),
        Err(Error::MissingEntity)
    );
    let ctx = context();
    let mut draft = Draft::from_acquired(&raw("everyone"), &ctx).unwrap();
    draft.replace_external(vec![T3.into()], &ctx).unwrap();
    let restricted=TargetPolicy::from_acquired(&json!({"ok":true,"trippable_by_slack_connected_teams":{"hermes_triggers_trippable_by_slack_connected_teams_mcp":"NONE","allowed_teams":[]}}),Some(&json!({"ok":true,"is_restricted":false,"restricted_by_connector_steps_only":false}))).unwrap();
    assert_eq!(
        draft.desired_embedded(&ctx, &restricted),
        Err(Error::Restricted)
    );
}
