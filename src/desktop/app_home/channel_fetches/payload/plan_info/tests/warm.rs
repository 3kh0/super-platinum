use super::*;
const ACCOUNT: &str = "67c1fd64-2911-4d9a-aa50-fd73155a98f8";
fn retained() -> (ShellState, Owner, Value) {
    let (mut s, _) = prepared();
    s.core.active_account = Some(ACCOUNT.into());
    let team = s.core.active_team.clone().unwrap();
    let context = Arc::new(Context::capture(&s, &team).unwrap());
    let owner = initial::begin(
        &mut s,
        context,
        Arc::new(members::Transformations::decode(&json!({"ok":true,"assignments":{}})).unwrap()),
        true,
        false,
    )
    .unwrap();
    let raw =
        json!({"is_plan_frozen":true,"reconnect_grace_period_start_ts":42,"opaque":"retained"});
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .cfm_payload
        .plan_info_cache =
        source::PlanInfoCache::accepted(ACCOUNT, &owner.context.session, raw.clone());
    (s, owner, raw)
}
#[test]
fn warm_history_does_not_mint_live_or_uncanonical_display_and_empty_is_positive() {
    let (mut s, o, raw) = retained();
    let team = o.context.session.team_id.clone();
    assert!(capture(&s, &team).is_none());
    assert!(display(&s, &team).is_none());
    extras(&mut s, &o, json!({"ok":true})).unwrap();
    assert!(capture(&s, &team).is_none());
    extras(&mut s, &o, json!({"ok":true,"plan_info":{}})).unwrap();
    let first = capture(&s, &team).unwrap();
    assert_eq!(first.raw(), &raw);
    extras(&mut s, &o, json!({"ok":true,"plan_info":{}})).unwrap();
    assert!(!first.current(&s));
    assert_eq!(capture(&s, &team).unwrap().raw(), &raw);
    assert_eq!(
        s.core.workspaces[&team]
            .cfm_payload
            .plan_info_cache
            .as_ref()
            .unwrap()
            .raw(),
        &raw
    );
}
#[test]
fn warm_malformed_foreign_and_late_do_not_replace_retained_data() {
    let (mut s, o, raw) = retained();
    let team = o.context.session.team_id.clone();
    let before = s.core.workspaces[&team].cfm_payload.clone();
    assert!(extras(&mut s, &o, json!({"ok":true,"plan_info":[]})).is_err());
    team_upsert(
        &mut s,
        &o,
        &json!({"id":"TFOREIGN","reconnect_grace_period_start_ts":99}),
    )
    .unwrap();
    assert_eq!(s.core.workspaces[&team].cfm_payload, before);
    s.core.account_epoch += 1;
    assert!(
        extras(
            &mut s,
            &o,
            json!({"ok":true,"plan_info":{"reconnect_grace_period_start_ts":99}})
        )
        .is_err()
    );
    assert_eq!(
        s.core.workspaces[&team]
            .cfm_payload
            .plan_info_cache
            .as_ref()
            .unwrap()
            .raw(),
        &raw
    );
    assert!(capture(&s, &team).is_none());
}
#[test]
fn warm_foreign_cache_is_not_a_merge_base_and_null_replaces_matching_history() {
    let (mut s, o, _) = retained();
    let team = o.context.session.team_id.clone();
    let foreign = source::PlanInfoCache::accepted(
        "c386ef8b-2908-4c2d-817d-9dbb1394866d",
        &o.context.session,
        json!({"opaque":"foreign","reconnect_grace_period_start_ts":99}),
    );
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .cfm_payload
        .plan_info_cache = foreign;
    extras(&mut s, &o, json!({"ok":true,"plan_info":{}})).unwrap();
    assert_eq!(
        capture(&s, &team).unwrap().raw(),
        &json!({"is_plan_frozen":false,"reconnect_grace_period_start_ts":0})
    );
    let (mut s, o, _) = retained();
    extras(
        &mut s,
        &o,
        json!({"ok":true,"plan_info":{"reconnect_grace_period_start_ts":null}}),
    )
    .unwrap();
    let current = capture(&s, &team).unwrap();
    assert!(current.raw()["reconnect_grace_period_start_ts"].is_null());
    assert_eq!(current.raw()["opaque"], "retained");
    assert!(
        s.app_home.channel_fetches.slots[&o.context.key()]
            .payload
            .plan_info
            .dirty
    );
}
