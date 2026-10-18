use super::*;

#[test]
fn ordered_offline_receipt_keeps_source_ties_and_snapshot_is_immutable() {
    let (mut s, owner, cookie) = setup();
    let cold = s
        .app_home
        .workflow_ranking
        .as_ref()
        .unwrap()
        .offline
        .value
        .clone();
    let token = s
        .app_home
        .workflow_ranking
        .as_mut()
        .unwrap()
        .offline
        .reserve(Instant::now())
        .unwrap();
    complete_observation(
        &mut s,
        &owner,
        &cookie,
        Kind::Offline,
        token,
        Ok(Reply::Offline(
            serde_json::json!({"ok":true,"channel_priority_90d":{"CZ":2,"CA":2,"CM":9}}),
            Some(vec!["CZ".into(), "CA".into(), "CM".into()]),
        )),
    );
    let slot = &s.app_home.workflow_ranking.as_ref().unwrap().offline;
    assert!(!slot.malformed);
    assert!(slot.pending.is_none());
    assert_eq!(
        slot.value.processed_top_channels().unwrap(),
        ["CM", "CZ", "CA"]
    );
    assert!(cold.processed_top_channels().unwrap().is_empty());
}

#[test]
fn stale_ordered_receipt_cannot_publish_after_a_successor_reservation() {
    let (mut s, owner, cookie) = setup();
    let now = Instant::now();
    let slot = &mut s.app_home.workflow_ranking.as_mut().unwrap().offline;
    let old = slot.reserve(now).unwrap();
    let new = slot.reserve(now + PENDING).unwrap();
    complete_observation(
        &mut s,
        &owner,
        &cookie,
        Kind::Offline,
        old,
        Ok(Reply::Offline(
            serde_json::json!({"ok":true,"channel_priority_90d":{"CZ":2}}),
            Some(vec!["CZ".into()]),
        )),
    );
    let slot = &s.app_home.workflow_ranking.as_ref().unwrap().offline;
    assert_eq!(slot.pending.map(|v| v.0), Some(new));
    assert!(slot.value.processed_top_channels().unwrap().is_empty());
}

#[test]
fn malformed_order_is_distinct_from_network_failure_with_source_cold_fallback() {
    let (mut s, owner, cookie) = setup();
    let now = Instant::now();
    let token = s
        .app_home
        .workflow_ranking
        .as_mut()
        .unwrap()
        .offline
        .reserve(now)
        .unwrap();
    complete_observation(
        &mut s,
        &owner,
        &cookie,
        Kind::Offline,
        token,
        Ok(Reply::Offline(
            serde_json::json!({"ok":true,"channel_priority_90d":{"CZ":2,"CA":2}}),
            Some(vec!["CZ".into()]),
        )),
    );
    let slot = &mut s.app_home.workflow_ranking.as_mut().unwrap().offline;
    assert!(slot.malformed);
    assert!(slot.failure.is_none());
    let next = slot.reserve(now + FRESH + Duration::from_secs(1)).unwrap();
    complete_observation(
        &mut s,
        &owner,
        &cookie,
        Kind::Offline,
        next,
        Err(Error::Offline("synthetic outage".into())),
    );
    let slot = &s.app_home.workflow_ranking.as_ref().unwrap().offline;
    assert!(!slot.malformed);
    assert!(matches!(slot.failure, Some(Error::Offline(_))));
    assert!(slot.value.processed_top_channels().unwrap().is_empty());
}

#[test]
fn value_fixture_preserves_scores_without_inventing_original_order() {
    let (mut s, owner, cookie) = setup();
    let token = s
        .app_home
        .workflow_ranking
        .as_mut()
        .unwrap()
        .offline
        .reserve(Instant::now())
        .unwrap();
    complete(
        &mut s,
        &owner,
        &cookie,
        Kind::Offline,
        token,
        Ok(serde_json::json!({"ok":true,"channel_priority_90d":{"CZ":2,"CA":2}})),
    );
    let slot = &s.app_home.workflow_ranking.as_ref().unwrap().offline;
    assert!(!slot.malformed);
    assert_eq!(slot.value.priority("CZ"), 2.0);
    assert!(slot.value.processed_top_channels().is_err());
}

fn setup() -> (ShellState, Account, String) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install(&mut s);
    let owner = Account {
        context: Arc::new(Context::capture(&s).unwrap()),
        account: s.core.active_account.clone(),
        cookie: s.core.session.as_ref().unwrap().d_cookie.clone(),
    };
    let cookie = s.core.session.as_ref().unwrap().d_cookie.clone();
    s.app_home.workflow_ranking = Some(Cache {
        owner: owner.clone(),
        model: Slot::cold(Model::fallback()),
        offline: Slot::cold(observed::Offline::default()),
        similarity: Arc::new(std::sync::Mutex::new(SimilarityCache::default())),
    });
    (s, owner, cookie)
}

#[test]
fn shared_home_reservation_and_consumer_use_one_token_namespace() {
    let (mut s, owner, cookie) = setup();
    let now = Instant::now();
    let first = s
        .app_home
        .workflow_ranking
        .as_mut()
        .unwrap()
        .model
        .reserve(now)
        .unwrap();
    assert!(pending(&s, &owner, &cookie, Kind::Model, first));
    assert!(
        s.app_home
            .workflow_ranking
            .as_mut()
            .unwrap()
            .model
            .reserve(now)
            .is_none()
    );
    let second = s
        .app_home
        .workflow_ranking
        .as_mut()
        .unwrap()
        .model
        .reserve(now + PENDING)
        .unwrap();
    assert!(!pending(&s, &owner, &cookie, Kind::Model, first));
    assert!(pending(&s, &owner, &cookie, Kind::Model, second));
    let cache = s.app_home.workflow_ranking.as_mut().unwrap();
    settle(
        cache,
        Kind::Model,
        first,
        now + PENDING,
        Err(Error::Offline("retired reservation".into())),
    );
    assert!(cache.model.failure.is_none());
    assert_eq!(cache.model.pending.map(|v| v.0), Some(second));
}

#[test]
fn account_cookie_credentials_and_transport_retire_exact_request_owner() {
    let (mut s, owner, cookie) = setup();
    let token = s
        .app_home
        .workflow_ranking
        .as_mut()
        .unwrap()
        .model
        .reserve(Instant::now())
        .unwrap();
    assert!(pending(&s, &owner, &cookie, Kind::Model, token));
    s.core
        .session
        .as_mut()
        .unwrap()
        .d_cookie
        .push_str("changed");
    assert!(!pending(&s, &owner, &cookie, Kind::Model, token));
    s.core.session.as_mut().unwrap().d_cookie = cookie.clone();
    let team = owner.context.session.team_id.clone();
    s.core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .token
        .push_str("changed");
    assert!(!pending(&s, &owner, &cookie, Kind::Model, token));

    let (mut s, owner, cookie) = setup();
    let token = s
        .app_home
        .workflow_ranking
        .as_mut()
        .unwrap()
        .model
        .reserve(Instant::now())
        .unwrap();
    s.core.account_epoch += 1;
    assert!(!pending(&s, &owner, &cookie, Kind::Model, token));
}

#[test]
fn actual_failure_retains_fallback_receipt_but_malformed_success_is_unavailable() {
    let (mut s, _, _) = setup();
    let cache = s.app_home.workflow_ranking.as_mut().unwrap();
    let now = Instant::now();
    let old = cache.model.value.clone();
    let token = cache.model.reserve(now).unwrap();
    settle(
        cache,
        Kind::Model,
        token,
        now,
        Err(Error::RateLimited {
            retry_after_secs: Some(19),
        }),
    );
    assert!(!cache.model.malformed);
    assert!(matches!(
        cache.model.failure,
        Some(Error::RateLimited {
            retry_after_secs: Some(19)
        })
    ));
    assert!(!Arc::ptr_eq(&old, &cache.model.value));
    assert!(cache.model.reserve(now + FRESH).is_none());
    let next = now + FRESH + Duration::from_millis(1);
    let token = cache.model.reserve(next).unwrap();
    settle(
        cache,
        Kind::Model,
        token,
        next,
        Ok(serde_json::json!({"ok":true,"model":"malformed"})),
    );
    assert!(cache.model.malformed);
}

#[test]
fn superseded_rate_limit_accounts_for_owner_without_settling_replacement() {
    let (mut s, owner, cookie) = setup();
    let now = Instant::now();
    let cache = s.app_home.workflow_ranking.as_mut().unwrap();
    let value = cache.model.value.clone();
    let first = cache.model.reserve(now).unwrap();
    let second = cache.model.reserve(now + PENDING).unwrap();
    complete(
        &mut s,
        &owner,
        &cookie,
        Kind::Model,
        first,
        Err(Error::RateLimited {
            retry_after_secs: Some(19),
        }),
    );
    assert!(cooling(&s, &owner.context));
    let cache = s.app_home.workflow_ranking.as_ref().unwrap();
    assert_eq!(cache.model.pending.map(|v| v.0), Some(second));
    assert!(Arc::ptr_eq(&value, &cache.model.value));
    assert!(cache.model.observed.is_none());
    assert!(cache.model.failure.is_none());
}

#[test]
fn retired_owner_or_cookie_cannot_account_for_late_rate_limit() {
    for changed_cookie in [false, true] {
        let (mut s, owner, cookie) = setup();
        let now = Instant::now();
        let cache = s.app_home.workflow_ranking.as_mut().unwrap();
        let value = cache.model.value.clone();
        let first = cache.model.reserve(now).unwrap();
        let second = cache.model.reserve(now + PENDING).unwrap();
        if changed_cookie {
            s.core
                .session
                .as_mut()
                .unwrap()
                .d_cookie
                .push_str("retired");
        } else {
            s.core.account_epoch += 1;
        }
        complete(
            &mut s,
            &owner,
            &cookie,
            Kind::Model,
            first,
            Err(Error::RateLimited {
                retry_after_secs: Some(19),
            }),
        );
        assert!(s.app_home.cooldowns.is_empty());
        let cache = s.app_home.workflow_ranking.as_ref().unwrap();
        assert_eq!(cache.model.pending.map(|v| v.0), Some(second));
        assert!(Arc::ptr_eq(&value, &cache.model.value));
        assert!(cache.model.failure.is_none());
    }
}

#[test]
fn cooled_unsent_job_retires_only_its_reservation_without_freshness_or_fallback() {
    let (mut s, owner, cookie) = setup();
    let now = Instant::now();
    let cache = s.app_home.workflow_ranking.as_mut().unwrap();
    let value = cache.model.value.clone();
    let similarity = cache.similarity.clone();
    let first = cache.model.reserve(now).unwrap();
    let second = cache.model.reserve(now + PENDING).unwrap();
    let offline = cache.offline.reserve(now).unwrap();
    s.app_home.cooldowns.insert(
        (owner.context.epoch, owner.context.session.team_id.clone()),
        None,
    );
    retire_cooled(&mut s, &owner, &cookie, Kind::Model, first);
    assert!(pending(&s, &owner, &cookie, Kind::Model, second));
    retire_cooled(&mut s, &owner, &cookie, Kind::Model, second);
    let cache = s.app_home.workflow_ranking.as_ref().unwrap();
    assert!(cache.model.pending.is_none());
    assert!(cache.model.observed.is_none());
    assert!(cache.model.failure.is_none());
    assert!(!cache.model.malformed);
    assert!(Arc::ptr_eq(&cache.model.value, &value));
    assert!(Arc::ptr_eq(&cache.similarity, &similarity));
    assert_eq!(cache.offline.pending.map(|v| v.0), Some(offline));
    s.app_home.cooldowns.clear();
    assert!(
        s.app_home
            .workflow_ranking
            .as_mut()
            .unwrap()
            .model
            .reserve(now + PENDING)
            .is_some()
    );
}

#[test]
fn retired_cooldown_job_cannot_clear_current_or_successor_pending() {
    let (mut s, owner, cookie) = setup();
    let token = s
        .app_home
        .workflow_ranking
        .as_mut()
        .unwrap()
        .model
        .reserve(Instant::now())
        .unwrap();
    s.app_home.cooldowns.insert(
        (owner.context.epoch, owner.context.session.team_id.clone()),
        None,
    );
    s.core
        .session
        .as_mut()
        .unwrap()
        .d_cookie
        .push_str("retired");
    retire_cooled(&mut s, &owner, &cookie, Kind::Model, token);
    assert_eq!(
        s.app_home
            .workflow_ranking
            .as_ref()
            .unwrap()
            .model
            .pending
            .map(|v| v.0),
        Some(token)
    );
}

#[test]
fn snapshots_are_immutable_and_bind_cookie_account_session_and_transport() {
    for change in 0..7 {
        let (mut s, owner, _) = setup();
        let cache = s.app_home.workflow_ranking.as_ref().unwrap();
        let snapshot = Snapshot {
            owner,
            model: cache.model.value.clone(),
            offline: cache.offline.value.clone(),
            similarity: cache.similarity.clone(),
        };
        assert!(snapshot.current(&s));
        let team = snapshot.owner.context.session.team_id.clone();
        match change {
            0 => s
                .core
                .session
                .as_mut()
                .unwrap()
                .d_cookie
                .push_str("changed"),
            1 => s.core.active_account = Some("different-account".into()),
            2 => s.core.account_epoch += 1,
            3 => s
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut(&team)
                .unwrap()
                .token
                .push_str("changed"),
            4 => s
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut(&team)
                .unwrap()
                .url
                .push_str("changed"),
            5 => {
                s.core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .enterprise_id = Some("EOTHER".into())
            }
            _ => {
                s.core.transport = Some(Arc::new(
                    super_platinum_core::slack::Transport::new("synthetic").unwrap(),
                ))
            }
        }
        assert!(!snapshot.current(&s), "change {change}");
    }
    let (mut s, owner, cookie) = setup();
    let cache = s.app_home.workflow_ranking.as_mut().unwrap();
    let model = cache.model.value.clone();
    let token = cache.model.reserve(Instant::now()).unwrap();
    complete(
        &mut s,
        &owner,
        &cookie,
        Kind::Model,
        token,
        Ok(serde_json::json!({"ok":true})),
    );
    assert!(!Arc::ptr_eq(
        &model,
        &s.app_home.workflow_ranking.as_ref().unwrap().model.value
    ));
    assert!(owner.current(&s));
}
