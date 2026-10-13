use super::*;
use serde_json::json;

fn profile(id: &str, app: &str, updated: Option<u64>) -> BotProfile {
    let mut value = json!({"id":id,"app_id":app,"team_id":"TREMOTE"});
    if let Some(updated) = updated {
        value["updated"] = updated.into();
    }
    serde_json::from_value(value).unwrap()
}

#[test]
fn lookup_deduplicates_keys_and_scopes_negative_cooldown() {
    let mut state = BotLookupState::default();
    let first = state.begin("B1", Some("A1"), Duration::ZERO).unwrap();
    assert!(state.begin("B1", Some("A1"), Duration::ZERO).is_none());
    let other_hint = state.begin("B1", Some("A2"), Duration::ZERO).unwrap();
    assert!(state.complete(&first, None, Duration::ZERO));
    assert!(state.complete(&other_hint, Some(profile("B1", "A2", None)), Duration::ZERO));
    assert!(
        state
            .begin("B1", Some("A1"), NEGATIVE_COOLDOWN - Duration::from_secs(1))
            .is_none()
    );
    assert!(state.get("B1", Some("A1")).is_none());
    assert_eq!(
        state.get("B1", Some("A2")).unwrap().extra["team_id"],
        "TREMOTE"
    );
    assert!(state.begin("B1", Some("A1"), NEGATIVE_COOLDOWN).is_none());
    assert!(
        state
            .begin_retry("B1", Some("A1"), NEGATIVE_COOLDOWN)
            .is_some()
    );
}

#[test]
fn stale_expired_and_cleared_completions_cannot_replace_current_work() {
    let mut state = BotLookupState::default();
    let expired = state.begin("B1", None, Duration::ZERO).unwrap();
    assert!(state.is_pending("B1", None, Duration::ZERO));
    assert!(!state.is_pending("B1", Some("A1"), Duration::ZERO));
    assert!(!state.is_pending("B1", None, REQUEST_LIFETIME));
    assert!(state.begin("B1", None, REQUEST_LIFETIME).is_none());
    let later = REQUEST_LIFETIME + NEGATIVE_COOLDOWN;
    let replacement = state.begin("B1", None, later).unwrap();
    assert!(!state.complete(&expired, Some(profile("B1", "A1", None)), later));
    assert!(!state.fail(&expired, later, Duration::ZERO));
    assert!(state.complete(&replacement, Some(profile("B1", "A1", None)), later));
    let old = state.begin_retry("B1", None, later).unwrap();
    state.clear();
    let current = state.begin("B1", None, later).unwrap();
    assert!(!state.complete(&old, Some(profile("B1", "A2", None)), later));
    assert!(state.complete(&current, Some(profile("B1", "A1", None)), later));
    assert!(!state.complete(&current, Some(profile("B1", "A2", None)), later));
}

#[test]
fn malformed_identity_is_never_cached_and_missing_metadata_is_not_inferred() {
    let mut state = BotLookupState::default();
    let wrong_bot = state.begin("B1", Some("A1"), Duration::ZERO).unwrap();
    assert!(!state.complete(&wrong_bot, Some(profile("B2", "A1", None)), Duration::ZERO));
    assert!(state.get("B1", None).is_none());
    assert!(state.get("B2", None).is_none());
    assert!(state.begin("B1", Some("A1"), Duration::ZERO).is_none());
    let wrong_app = state.begin("B1", Some("A2"), Duration::ZERO).unwrap();
    assert!(!state.complete(&wrong_app, Some(profile("B1", "A1", None)), Duration::ZERO));
    let missing = state.begin("B1", None, Duration::ZERO).unwrap();
    assert!(state.complete(
        &missing,
        Some(serde_json::from_value(json!({"id":"B1"})).unwrap()),
        Duration::ZERO
    ));
    let found = state.get("B1", Some("A1")).unwrap();
    assert!(!found.extra.contains_key("app_id"));
    assert!(!found.extra.contains_key("team_id"));
    for id in [
        "BSLACKBOT",
        "B01",
        "B03",
        "BSLACKSECURITY",
        "BCHANNELEXPERT",
        "B1,B2",
        "",
    ] {
        assert!(state.begin(id, None, Duration::ZERO).is_none());
    }
}

#[test]
fn updated_timestamp_and_request_order_prevent_out_of_order_downgrades() {
    let mut state = BotLookupState::default();
    let old = state.begin_retry("B1", None, Duration::ZERO).unwrap();
    let new = state.begin_retry("B1", Some("A1"), Duration::ZERO).unwrap();
    assert!(state.complete(&new, Some(profile("B1", "A1", Some(20))), Duration::ZERO));
    assert!(state.complete(&old, Some(profile("B1", "A1", Some(10))), Duration::ZERO));
    assert_eq!(state.get("B1", None).unwrap().extra["updated"], 20);
    let old = state.begin_retry("B1", None, Duration::ZERO).unwrap();
    let new = state.begin_retry("B1", Some("A1"), Duration::ZERO).unwrap();
    assert!(state.complete(&new, Some(profile("B1", "A1", None)), Duration::ZERO));
    assert!(state.complete(&old, Some(profile("B1", "A2", None)), Duration::ZERO));
    assert_eq!(state.get("B1", None).unwrap().extra["app_id"], "A1");
    // A reliable newer server timestamp wins even if its request started first.
    let old = state.begin_retry("B1", None, Duration::ZERO).unwrap();
    let new = state.begin_retry("B1", Some("A1"), Duration::ZERO).unwrap();
    assert!(state.complete(&new, Some(profile("B1", "A1", Some(30))), Duration::ZERO));
    assert!(state.complete(&old, Some(profile("B1", "A1", Some(40))), Duration::ZERO));
    assert_eq!(state.get("B1", None).unwrap().extra["updated"], 40);
}

#[test]
fn state_is_bounded_and_retry_after_is_honored() {
    let mut state = BotLookupState::default();
    for id in 0..MAX_IN_FLIGHT {
        assert!(
            state
                .begin(&format!("B{id}"), None, Duration::ZERO)
                .is_some()
        );
    }
    assert!(state.begin("BLIMIT", None, Duration::ZERO).is_none());
    state.clear();
    let ticket = state.begin("B1", None, Duration::ZERO).unwrap();
    assert!(state.fail(&ticket, Duration::ZERO, Duration::from_secs(120)));
    assert!(state.begin("B1", None, Duration::from_secs(119)).is_none());
    assert!(state.begin("B1", None, Duration::from_secs(120)).is_some());
    state.clear();
    for id in 0..MAX_NEGATIVE + 1 {
        let ticket = state
            .begin(&format!("B{id}"), None, Duration::ZERO)
            .unwrap();
        assert!(state.complete(&ticket, None, Duration::ZERO));
    }
    assert_eq!(state.negative.len(), MAX_NEGATIVE);
    assert!(state.begin("BNEW", None, Duration::ZERO).is_none());
    assert!(state.begin_retry("BNEW", None, Duration::ZERO).is_none());
    assert!(state.begin_retry("BNEW", None, NEGATIVE_COOLDOWN).is_some());
}

#[test]
fn automatic_positive_sweeps_stop_at_capacity_and_explicit_retry_can_replace() {
    let mut state = BotLookupState::default();
    let mut requests = 0;
    for _ in 0..3 {
        for id in 0..MAX_PROFILES + 20 {
            let id = format!("BC{id}");
            if let Some(ticket) = state.begin(&id, None, Duration::ZERO) {
                requests += 1;
                assert!(state.complete(&ticket, Some(profile(&id, "A1", None)), Duration::ZERO));
            }
        }
    }
    assert_eq!(requests, MAX_PROFILES);
    assert_eq!(state.profiles.len(), MAX_PROFILES);
    assert!(state.get("BC0", None).is_some());
    let retry = state.begin_retry("BNEW", None, Duration::ZERO).unwrap();
    assert!(state.complete(&retry, Some(profile("BNEW", "A1", None)), Duration::ZERO));
    assert!(state.get("BC0", None).is_none());
    assert!(state.get("BNEW", None).is_some());
    assert!(state.begin("BC0", None, Duration::from_secs(999)).is_none());
    assert!(state.begin("BNEW", Some("A2"), Duration::ZERO).is_none());
    let retry = state
        .begin_retry("BNEW", Some("A2"), Duration::ZERO)
        .unwrap();
    assert!(state.complete(&retry, Some(profile("BNEW", "A2", None)), Duration::ZERO));
    assert_eq!(state.get("BNEW", None).unwrap().extra["app_id"], "A2");
}

#[test]
fn negative_sweep_overflow_cannot_refetch_evicted_failures() {
    let mut state = BotLookupState::default();
    let mut requests = 0;
    for _ in 0..3 {
        for id in 0..MAX_NEGATIVE + 30 {
            if let Some(ticket) = state.begin(&format!("BF{id}"), None, Duration::ZERO) {
                requests += 1;
                assert!(state.complete(&ticket, None, Duration::ZERO));
            }
        }
    }
    assert_eq!(requests, MAX_NEGATIVE + 1);
    assert_eq!(state.negative.len(), MAX_NEGATIVE);
    assert!(state.begin_retry("BF0", None, NEGATIVE_COOLDOWN).is_some());
}

#[test]
fn automatic_pending_slots_cannot_evict_after_explicit_completion() {
    let mut state = BotLookupState::default();
    for id in 0..MAX_PROFILES - 1 {
        let id = format!("BC{id}");
        let ticket = state.begin(&id, None, Duration::ZERO).unwrap();
        assert!(state.complete(&ticket, Some(profile(&id, "A1", None)), Duration::ZERO));
    }
    let auto = state.begin("BAUTO", None, Duration::ZERO).unwrap();
    assert!(state.begin("BAUTO2", None, Duration::ZERO).is_none());
    let explicit = state.begin_retry("BRETRY", None, Duration::ZERO).unwrap();
    assert!(state.complete(
        &explicit,
        Some(profile("BRETRY", "A1", None)),
        Duration::ZERO
    ));
    assert!(!state.complete(&auto, Some(profile("BAUTO", "A1", None)), Duration::ZERO));
    assert!(state.get("BC0", None).is_some());
    assert!(state.get("BRETRY", None).is_some());
    assert!(state.get("BAUTO", None).is_none());
    assert_eq!(state.profiles.len(), MAX_PROFILES);
}
