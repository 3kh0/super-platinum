use super::tests::{Database, local, scope};
use super::*;
use crate::drafts::{
    codec,
    sync::{DraftObservation, ObservationKind, RemoteDraft},
};

fn adopted() -> DraftSync {
    DraftSync::adopt_remote(
        &scope(),
        RemoteDraft {
            scope: scope(),
            id: "DrObserved".into(),
            client_msg_id: "00000000-0000-4000-8000-000000000002".into(),
            last_updated_ts: "1000".into(),
            snapshot: codec::encode(&scope().key, "Original").unwrap(),
        },
    )
    .unwrap()
}
fn event(state: &DraftSync) -> DraftObservation {
    let mut remote = state.acknowledged().unwrap().clone();
    remote.last_updated_ts = "2000".into();
    remote.snapshot = codec::encode(&scope().key, "Remote").unwrap();
    DraftObservation {
        scope: scope(),
        draft_id: remote.id.clone(),
        client_msg_id: remote.client_msg_id.clone(),
        event_ts: "10.000001".into(),
        kind: ObservationKind::Active(remote),
    }
}

#[test]
fn formats_one_two_three_load_and_upgrade_without_inventing_observation_evidence() {
    for format in [1, 2, 3] {
        let db = Database::new();
        let mut store = db.open();
        let scope = scope();
        let state = adopted();
        store
            .save([(scope.key.clone(), local("Original"))])
            .unwrap();
        let Commit::Applied(initial) = store
            .commit_synced(&scope, None, &local("Original"), &state, None)
            .unwrap()
        else {
            panic!("enrollment")
        };
        let mut legacy = serde_json::to_value(&state).unwrap();
        legacy.as_object_mut().unwrap().remove("observed_events");
        legacy
            .as_object_mut()
            .unwrap()
            .remove("blocked_observation");
        store
            .0
            .execute(
                "UPDATE composer_draft_sync SET format_version=?1,state=?2",
                params![format, serde_json::to_string(&legacy).unwrap()],
            )
            .unwrap();
        let restored = store
            .load_synced(&scope.account_id, LoadMode::AfterRestart)
            .unwrap()
            .synced
            .remove(&scope.key)
            .unwrap();
        assert_eq!(restored.state.observed_event_ts("DrObserved"), None);
        assert!(restored.state.blocked_observation().is_none());
        let mut observed = restored.state.clone();
        observed.observe_event(&event(&observed.clone())).unwrap();
        let Commit::Applied(saved) = store
            .commit_synced(
                &scope,
                Some(initial.revision),
                &local("Remote"),
                &observed,
                None,
            )
            .unwrap()
        else {
            panic!("observation CAS")
        };
        let actual: i64 = store
            .0
            .query_row(
                "SELECT format_version FROM composer_draft_sync",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(actual, 4);
        // The previous binary's decoder accepted only 1, 2 and 3.
        assert!(!matches!(actual, 1 | 2 | 3));
        drop(store);
        let loaded = db
            .open()
            .load_synced(&scope.account_id, LoadMode::AfterRestart)
            .unwrap();
        assert_eq!(loaded.synced[&scope.key].state, saved.state);
        assert_eq!(loaded.local[0].1.composer.text, "Remote");
    }
}

#[test]
fn competing_store_cas_cannot_replace_content_or_advance_event_watermark() {
    let db = Database::new();
    let mut first = db.open();
    let mut second = db.open();
    let scope = scope();
    let state = adopted();
    first
        .save([(scope.key.clone(), local("Original"))])
        .unwrap();
    let Commit::Applied(initial) = first
        .commit_synced(&scope, None, &local("Original"), &state, None)
        .unwrap()
    else {
        panic!("enroll")
    };
    let mut observation = initial.state.clone();
    observation.observe_event(&event(&state)).unwrap();
    let mut typed = initial.state.clone();
    typed.edit(
        &scope,
        Some(codec::encode(&scope.key, "New local").unwrap()),
    );
    let Commit::Applied(newer) = first
        .commit_synced(
            &scope,
            Some(initial.revision),
            &local("New local"),
            &typed,
            None,
        )
        .unwrap()
    else {
        panic!("typing")
    };
    assert!(matches!(
        second
            .commit_synced(
                &scope,
                Some(initial.revision),
                &local("Remote"),
                &observation,
                None
            )
            .unwrap(),
        Commit::Stale(_)
    ));
    let current = second
        .load_synced(&scope.account_id, LoadMode::Current)
        .unwrap();
    assert_eq!(current.synced[&scope.key].state, newer.state);
    assert_eq!(current.local[0].1.composer.text, "New local");
    assert_eq!(
        current.synced[&scope.key]
            .state
            .observed_event_ts("DrObserved"),
        None
    );
}

#[test]
fn malformed_persisted_ordering_fails_closed_without_altering_local_content() {
    let db = Database::new();
    let mut store = db.open();
    let scope = scope();
    let mut state = adopted();
    state.observe_event(&event(&state.clone())).unwrap();
    store.save([(scope.key.clone(), local("Remote"))]).unwrap();
    store
        .commit_synced(&scope, None, &local("Remote"), &state, None)
        .unwrap();
    let mut raw = serde_json::to_value(&state).unwrap();
    raw["observed_events"]["DrObserved"]["event_ts"] = "NaN".into();
    store
        .0
        .execute(
            "UPDATE composer_draft_sync SET state=?1",
            [serde_json::to_string(&raw).unwrap()],
        )
        .unwrap();
    assert!(
        store
            .load_synced(&scope.account_id, LoadMode::AfterRestart)
            .is_err()
    );
    assert_eq!(store.load().unwrap()[0].1.composer.text, "Remote");
}

#[test]
fn failed_observation_transaction_rolls_back_local_content_revision_and_event_clock() {
    let db = Database::new();
    let mut store = db.open();
    let scope = scope();
    let state = adopted();
    store
        .save([(scope.key.clone(), local("Original"))])
        .unwrap();
    let Commit::Applied(before) = store
        .commit_synced(&scope, None, &local("Original"), &state, None)
        .unwrap()
    else {
        panic!("enroll")
    };
    let mut observed = before.state.clone();
    observed.observe_event(&event(&state)).unwrap();
    store.0.execute_batch("CREATE TRIGGER fail_observation BEFORE UPDATE ON composer_draft_sync BEGIN SELECT RAISE(ABORT,'isolated failure'); END;").unwrap();
    assert!(
        store
            .commit_synced(
                &scope,
                Some(before.revision),
                &local("Remote"),
                &observed,
                None
            )
            .is_err()
    );
    let unchanged = store
        .load_synced(&scope.account_id, LoadMode::Current)
        .unwrap();
    assert_eq!(unchanged.local[0].1.composer.text, "Original");
    assert_eq!(unchanged.synced[&scope.key].revision, before.revision);
    assert_eq!(unchanged.synced[&scope.key].state, before.state);
    assert_eq!(
        unchanged.synced[&scope.key]
            .state
            .observed_event_ts("DrObserved"),
        None
    );
    store
        .0
        .execute_batch("DROP TRIGGER fail_observation")
        .unwrap();
    assert!(matches!(
        store
            .commit_synced(
                &scope,
                Some(before.revision),
                &local("Remote"),
                &observed,
                None
            )
            .unwrap(),
        Commit::Applied(_)
    ));
    drop(store);
    let restored = db
        .open()
        .load_synced(&scope.account_id, LoadMode::AfterRestart)
        .unwrap();
    assert_eq!(restored.local[0].1.composer.text, "Remote");
    assert_eq!(restored.synced[&scope.key].state, observed);
}
