use super::*;
use serde_json::json;
#[test]
fn sharing_requests_use_captured_search_options_and_single_whitespace_mpim_normalization() {
    let rows = queries("Ada,  Bea");
    assert_eq!(
        rows[0],
        (
            "users/search",
            json!({"query":"Ada,  Bea","count":100,"fuzz":1,"enable_workspace_ranking":true,"filter":"NOT deactivated","search_email":true})
        )
    );
    assert_eq!(
        rows[1],
        (
            "channels/search",
            json!({"query":"Ada,  Bea","count":100,"fuzz":1,"filter":"xws","include_record_channels":true})
        )
    );
    assert_eq!(
        rows[2],
        (
            "mpims/search",
            json!({"query":"Ada, Bea","count":100,"fuzz":1,"use_display_name":true})
        )
    );
}
#[test]
fn sharing_selection_requires_exact_current_remote_or_cached_destination_and_query_owner() {
    let (mut s, id) = super::super::tests::setup();
    let remote = Destination::Channel(
        serde_json::from_value(json!({"id":"CCAL","is_channel":true,"name":"Remote"})).unwrap(),
    );
    let c = active_mut(&mut s, id).unwrap();
    c.share_channel = "CCAL".into();
    c.share_selected = Some(remote);
    c.share_generation = 7;
    posting::test_acquire(
        &mut s,
        id,
        vec![json!({"id":"CCAL","is_channel":true,"name":"Remote"})],
    );
    assert_eq!(resolve(&s, id).as_deref(), Some("CCAL"));
    assert!(query_current(&s, id, 7));
    assert!(!query_current(&s, id, 6));
    let team = context(&s, id).unwrap().session.team_id.clone();
    s.core.workspaces.get_mut(&team).unwrap().channels.insert(
        "CCAL".into(),
        serde_json::from_value(json!({"id":"CCAL","is_channel":true,"is_archived":true})).unwrap(),
    );
    assert!(resolve(&s, id).is_none());
    s.channel_generation += 1;
    assert!(!query_current(&s, id, 7));
    assert!(resolve(&s, id).is_none());
}
#[test]
fn posting_restrictions_are_not_replaced_by_membership_or_search_presence() {
    let (mut s, id) = super::super::tests::setup();
    let ctx = context(&s, id).unwrap();
    let allowed = json!({"id":"CCAL","is_channel":true,"is_member":false,"is_ext_shared":false});
    posting::test_acquire(&mut s, id, vec![allowed.clone()]);
    assert!(channel(
        &s,
        &ctx,
        &serde_json::from_value(allowed.clone()).unwrap()
    ));
    for (key, value) in [
        ("is_frozen", json!(true)),
        ("is_read_only", json!(true)),
        ("is_thread_only", json!(true)),
        ("isUnknown", json!(true)),
        ("is_read_only", json!("false")),
        ("properties", json!({"posting_restricted_to":{}})),
        ("properties", json!({"huddles":{"is_huddle_channel":true}})),
    ] {
        let mut raw = allowed.clone();
        raw[key] = value;
        posting::test_acquire(&mut s, id, vec![raw.clone()]);
        assert!(!channel(&s, &ctx, &serde_json::from_value(raw).unwrap()));
    }
    let mut raw = allowed;
    raw["properties"] = json!({"posting_restricted_to":{"role":["all"]}});
    posting::test_acquire(&mut s, id, vec![raw.clone()]);
    assert!(channel(&s, &ctx, &serde_json::from_value(raw).unwrap()));
}
#[test]
fn local_destination_results_are_bounded_and_keep_selected_identity_across_query() {
    let (mut s, id) = super::super::tests::setup();
    let ctx = context(&s, id).unwrap();
    for n in 0..150 {
        let id = format!("UCAL{n}");
        s.core
            .workspaces
            .get_mut(&ctx.session.team_id)
            .unwrap()
            .users
            .insert(
                id.clone(),
                serde_json::from_value(json!({"id":id,"name":format!("Person {n:03}")})).unwrap(),
            );
    }
    let rows = load::share_choices(&s, &ctx, "Person", "UCAL149");
    assert_eq!(rows.len(), 100);
    assert_eq!(rows[0].id, "UCAL149");
    let rows = load::share_choices(&s, &ctx, "no match", "UCAL149");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "UCAL149");
}
