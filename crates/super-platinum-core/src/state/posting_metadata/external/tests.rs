use super::*;
use serde_json::json;

fn store(free: bool) -> Store {
    let mut out = Store::default();
    out.bind("T1", "U1", Ok("E1".into())).unwrap();
    out.set_free(Ok(free));
    out
}
#[test]
fn explicit_connections_remove_only_top_and_never_delete_positives() {
    let mut s = store(false);
    s.channel(&json!({"connected_team_ids":["E1","T1","T2"],"connected_limited_team_ids":["T9"]}))
        .unwrap();
    assert!(!s.contains("E1").unwrap());
    for id in ["T1", "T2", "T9"] {
        assert!(s.contains(id).unwrap());
    }
    s.channel(&json!({"connected_team_ids":[]})).unwrap();
    s.channel(&json!({})).unwrap();
    assert!(s.contains("T2").unwrap());
    s.positive("E1").unwrap();
    assert!(s.contains("E1").unwrap());
}
#[test]
fn cached_history_is_unbound_owner_scoped_and_corruption_stays_unknown() {
    let mut s = store(false);
    s.positive("T9").unwrap();
    let raw = s.export();
    let mut restored = Store::restore(Some(&raw));
    assert!(restored.contains("T9").is_err());
    restored.bind("T1", "U1", Ok("E2".into())).unwrap();
    restored.set_free(Ok(false));
    assert!(restored.contains("T9").unwrap());
    restored.bind("T1", "U2", Ok("E2".into())).unwrap();
    restored.set_free(Ok(false));
    assert!(!restored.contains("T9").unwrap());
    for raw in [
        Value::Null,
        json!({"version":2,"saved":{"state":"uninitialized"}}),
        json!({"version":1,"saved":{"state":"positive","owner":{"team":"T1","user":"U1"},"ids":[""]}}),
    ] {
        assert!(
            Store::restore(Some(&raw))
                .bind("T1", "U1", Ok("E1".into()))
                .is_err()
        );
    }
    let mut legacy = Store::restore(None);
    legacy.bind("T1", "U1", Ok("E1".into())).unwrap();
    legacy.set_free(Ok(false));
    assert!(!legacy.contains("T9").unwrap());
}
#[test]
fn free_validation_is_once_throttled_and_only_first_org_adds() {
    let mut s = store(true);
    let now = Instant::now();
    assert_eq!(s.status(now).unwrap(), Validation::Required);
    assert!(s.contains("T9").is_err());
    let nonce = s.begin_validation(now).unwrap();
    assert_eq!(s.status(now).unwrap(), Validation::Pending);
    assert!(s.begin_validation(now).is_err());
    s.finish_validation(
        &nonce,
        Some(
            &json!({"ok":true,"orgs":[{"team_summary":{"id":"E1"}},{"team_summary":{"id":"T9"}}]}),
        ),
        now,
    )
    .unwrap();
    assert!(s.contains("E1").unwrap());
    assert!(!s.contains("T9").unwrap());
    assert!(
        s.finish_validation(&nonce, Some(&json!({"ok":true,"orgs":[]})), now)
            .is_err()
    );
    assert_eq!(s.status(now + INTERVAL).unwrap(), Validation::Required);
}
#[test]
fn failed_validation_releases_only_own_nonce_without_fabricating_negative() {
    let mut s = store(true);
    let now = Instant::now();
    let old = s.begin_validation(now).unwrap();
    assert!(s.finish_validation(&old, None, now).is_err());
    assert!(matches!(s.status(now).unwrap(), Validation::Throttled(_)));
    assert!(s.contains("T9").is_err());
    let next = s.begin_validation(now + INTERVAL).unwrap();
    assert!(
        s.finish_validation(&old, Some(&json!({"ok":true,"orgs":[]})), now + INTERVAL)
            .is_err()
    );
    assert!(s.validation_current(&next));
    s.finish_validation(&next, Some(&json!({"ok":true,"orgs":[]})), now + INTERVAL)
        .unwrap();
    assert!(!s.contains("T9").unwrap());
}
#[test]
fn malformed_channel_retires_readiness_but_preserves_saved_positive_history() {
    let mut s = store(false);
    s.positive("T9").unwrap();
    let old = s.revision();
    assert!(s.channel(&json!({"connected_team_ids":["T8",5]})).is_err());
    assert!(!s.current(&old));
    assert!(s.contains("T9").is_err());
    s.bind("T1", "U1", Ok("E1".into())).unwrap();
    s.set_free(Ok(false));
    assert!(s.contains("T9").unwrap());
    assert!(!s.contains("T8").unwrap());
}
#[test]
fn configuration_branch_and_free_plan_follow_actual_source_inputs() {
    let mut c = Configuration {
        team: "T1".into(),
        user: "U1".into(),
        enterprise: None,
        unified: None,
    };
    let raw = json!({"team":{"id":"T1"},"enterprise_id":"E1"});
    assert_eq!(top(&c, &raw).unwrap(), "E1");
    assert!(free(&c, &raw).unwrap());
    assert!(!free(&c, &json!({"team":{"id":"T1","enterprise_id":"E1"}})).unwrap());
    assert!(!free(&c, &json!({"team":{"id":"T1","plan":"std"}})).unwrap());
    c.team = "E2".into();
    assert_eq!(top(&c, &raw), Err(Unknown::Configuration));
    assert!(!free(&c, &raw).unwrap());
    c.unified = Some(false);
    assert_eq!(top(&c, &raw).unwrap(), "E2");
    c.unified = Some(true);
    assert_eq!(top(&c, &raw).unwrap(), "E1");
    assert!(top(&c, &json!({"enterprise_id":7})).is_err());
}
#[test]
fn metadata_cache_and_invalidation_keep_only_unbound_history() {
    let mut metadata = Metadata::default();
    metadata.external = store(false);
    metadata.external.positive("T9").unwrap();
    metadata.team_id = "T1".into();
    let snapshot = metadata.external_cache_snapshot();
    assert_eq!(
        snapshot.cached_external_teams(),
        metadata.cached_external_teams()
    );
    assert!(snapshot.external.contains("T9").is_err());
    assert!(snapshot.team_id.is_empty());
    metadata.invalidate();
    assert_eq!(
        snapshot.cached_external_teams(),
        metadata.cached_external_teams()
    );
    assert!(metadata.external.contains("T9").is_err());
}

#[test]
fn contradictory_success_never_initializes_absence_or_consumes_a_newer_attempt() {
    for error in [json!("ratelimited"), json!(false), json!({}), json!("")] {
        let mut s = store(true);
        let now = Instant::now();
        let old = s.begin_validation(now).unwrap();
        assert!(
            s.finish_validation(&old, Some(&json!({"ok":true,"error":error,"orgs":[]})), now)
                .is_err()
        );
        assert!(s.contains("T9").is_err());
        assert!(matches!(s.status(now).unwrap(), Validation::Throttled(_)));
        let next = s.begin_validation(now + INTERVAL).unwrap();
        assert!(
            s.finish_validation(&old, Some(&json!({"ok":true,"orgs":[]})), now + INTERVAL)
                .is_err()
        );
        assert!(s.validation_current(&next));
        s.finish_validation(
            &next,
            Some(&json!({"ok":true,"error":null,"orgs":[]})),
            now + INTERVAL,
        )
        .unwrap();
        assert!(!s.contains("T9").unwrap());
    }
}
