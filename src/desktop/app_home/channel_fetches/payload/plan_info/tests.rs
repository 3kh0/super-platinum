use super::*;
use crate::app_home::channel_fetches::{fixture, initial, members};
use serde_json::json;
mod warm;

fn prepared() -> (ShellState, Owner) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut shell);
    let team = shell.core.active_team.clone().unwrap();
    let context = Arc::new(Context::capture(&shell, &team).unwrap());
    let owner = initial::begin(
        &mut shell,
        context,
        Arc::new(members::Transformations::decode(&json!({"ok":true,"assignments":{}})).unwrap()),
        true,
        false,
    )
    .unwrap();
    (shell, owner)
}
fn extras(shell: &mut ShellState, owner: &Owner, raw: Value) -> Result<(), Error> {
    let ctx = &owner.context;
    let original = source::Owner::capture(
        &ctx.account,
        &ctx.session,
        ctx.transport.d_cookie(),
        ctx.epoch,
    );
    let payload =
        source::Payload::observed(source::PayloadKind::Extras, original, raw.to_string()).unwrap();
    super::super::dispatch(
        shell,
        owner,
        super::super::Prepared {
            payload,
            member: None,
        },
    )
}

#[test]
fn actual_extras_action_adopts_and_omission_or_null_payload_does_not_reset() {
    let (mut shell, owner) = prepared();
    let team = owner.context.session.team_id.clone();
    assert!(capture(&shell, &team).is_none());
    extras(&mut shell, &owner, json!({"ok":true})).unwrap();
    assert!(capture(&shell, &team).is_none());
    extras(
        &mut shell,
        &owner,
        json!({"ok":true,"plan_info":{
        "reconnect_grace_period_start_ts":42,"opaque":[false,null]}}),
    )
    .unwrap();
    let observed = capture(&shell, &team).unwrap();
    assert_eq!(
        observed.raw(),
        &json!({"is_plan_frozen":false,
        "reconnect_grace_period_start_ts":42,"opaque":[false,null]})
    );
    for skipped in [Value::Null, json!(false), json!(0), json!("")] {
        extras(&mut shell, &owner, json!({"ok":true,"plan_info":skipped})).unwrap();
        assert!(observed.current(&shell));
    }
    extras(
        &mut shell,
        &owner,
        json!({"ok":true,"plan_info":{
        "reconnect_grace_period_start_ts":null}}),
    )
    .unwrap();
    assert!(!observed.current(&shell));
    let reset = capture(&shell, &team).unwrap();
    assert!(reset.raw()["reconnect_grace_period_start_ts"].is_null());
    assert_eq!(reset.raw()["opaque"], json!([false, null]));
}

#[test]
fn malformed_action_preserves_prior_receipt_but_equal_action_retires_it() {
    let (mut shell, owner) = prepared();
    let team = owner.context.session.team_id.clone();
    extras(&mut shell, &owner, json!({"ok":true,"plan_info":{}})).unwrap();
    let original = capture(&shell, &team).unwrap();
    assert!(extras(&mut shell, &owner, json!({"ok":true,"plan_info":[1]})).is_err());
    assert!(original.current(&shell));
    extras(&mut shell, &owner, json!({"ok":true,"plan_info":{}})).unwrap();
    assert!(!original.current(&shell));
    assert_eq!(capture(&shell, &team).unwrap().raw(), original.raw());
}

#[test]
fn warm_value_is_not_authority_and_owner_raw_or_socket_changes_retire() {
    let (mut shell, owner) = prepared();
    let team = owner.context.session.team_id.clone();
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .cfm_payload
        .plan_info = Some(json!({
        "is_plan_frozen":true,"reconnect_grace_period_start_ts":7
    }));
    assert!(capture(&shell, &team).is_none());
    extras(
        &mut shell,
        &owner,
        json!({"ok":true,"plan_info":{"opaque":"actual"}}),
    )
    .unwrap();
    let original = capture(&shell, &team).unwrap();
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .cfm_payload
        .plan_info
        .as_mut()
        .unwrap()["opaque"] = json!("changed");
    assert!(!original.current(&shell));
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .cfm_payload
        .plan_info = Some(original.raw().clone());
    assert!(original.current(&shell));
    shell.core.workspaces.get_mut(&team).unwrap().rt_generation += 1;
    assert!(!original.current(&shell));
    assert!(extras(&mut shell, &owner, json!({"ok":true,"plan_info":{}})).is_err());
    assert!(capture(&shell, &team).is_none());
}

#[test]
fn replaced_slot_cannot_recertify_carried_positive_action() {
    let (mut shell, owner) = prepared();
    let team = owner.context.session.team_id.clone();
    extras(&mut shell, &owner, json!({"ok":true,"plan_info":{}})).unwrap();
    let original = capture(&shell, &team).unwrap();
    initial::retire(&mut shell, &owner);
    let replacement = initial::begin(
        &mut shell,
        owner.context.clone(),
        Arc::new(members::Transformations::decode(&json!({"ok":true,"assignments":{}})).unwrap()),
        false,
        false,
    )
    .unwrap();
    assert!(!original.current(&shell));
    assert!(capture(&shell, &team).is_none());
    extras(&mut shell, &replacement, json!({"ok":true,"plan_info":{}})).unwrap();
    assert!(capture(&shell, &team).unwrap().current(&shell));
    shell.core.account_epoch += 1;
    assert!(capture(&shell, &team).is_none());
}

#[test]
fn actual_team_sink_forwards_only_current_merged_defined_plan_fields() {
    fn upsert(shell: &mut ShellState, owner: &Owner, team: Value) {
        let mut sink = super::super::Sink {
            shell,
            owner,
            member: None,
        };
        source::Adopter::apply(
            &mut sink,
            &source::Effect {
                kind: source::Kind::TeamUpsert,
                payload: json!({"team":team}),
            },
        )
        .unwrap();
    }
    let (mut shell, owner) = prepared();
    let team = owner.context.session.team_id.clone();
    upsert(
        &mut shell,
        &owner,
        json!({"id":team,"name":"Observed team"}),
    );
    assert!(capture(&shell, &team).is_none());
    upsert(
        &mut shell,
        &owner,
        json!({"id":"TOTHER","is_plan_frozen":true,
        "reconnect_grace_period_start_ts":99}),
    );
    assert!(capture(&shell, &team).is_none());
    extras(
        &mut shell,
        &owner,
        json!({"ok":true,"plan_info":{"opaque":{"keep":true}}}),
    )
    .unwrap();
    upsert(
        &mut shell,
        &owner,
        json!({"id":team,"is_plan_frozen":true,
        "reconnect_grace_period_start_ts":42,"team_only":"not a plan field"}),
    );
    let first = capture(&shell, &team).unwrap();
    assert_eq!(
        first.raw(),
        &json!({"is_plan_frozen":true,
        "reconnect_grace_period_start_ts":42,"opaque":{"keep":true}})
    );
    upsert(&mut shell, &owner, json!({"id":team,"name":"Renamed"}));
    let repeated = capture(&shell, &team).unwrap();
    assert!(!first.current(&shell));
    assert_eq!(repeated.raw(), first.raw());
    upsert(
        &mut shell,
        &owner,
        json!({"id":team,"reconnect_grace_period_start_ts":null}),
    );
    assert!(capture(&shell, &team).unwrap().raw()["reconnect_grace_period_start_ts"].is_null());
    let latest = capture(&shell, &team).unwrap();
    shell.core.account_epoch += 1;
    assert!(
        team_upsert(
            &mut shell,
            &owner,
            &json!({"id":team,"is_plan_frozen":false})
        )
        .is_err()
    );
    assert_eq!(
        shell.core.workspaces[&team].cfm_payload.plan_info.as_ref(),
        Some(latest.raw())
    );
}
