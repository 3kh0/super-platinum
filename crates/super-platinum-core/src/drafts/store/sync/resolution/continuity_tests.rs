//! Candidate regression tests. These have NOT been compiled or executed.
use super::*;
use crate::drafts::store::sync::tests::{Database, local};
use crate::drafts::sync::{ConsumeKind, DraftObservation, ObservationKind, RemoteDraft};
use serde_json::{Value, json};

fn seed(store: &mut DraftStore) -> (Scope, Value) {
    let scope = Scope {
        account_id: uuid::Uuid::new_v4().to_string(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        },
    };
    let initial = local("original");
    let base = RemoteDraft {
        scope: scope.clone(),
        id: "D1".into(),
        client_msg_id: uuid::Uuid::new_v4().to_string(),
        last_updated_ts: "10.000001".into(),
        snapshot: codec::encode_composer(&scope.key, &initial.composer).unwrap(),
    };
    let state = DraftSync::adopt_remote(&scope, base.clone()).unwrap();
    store.save([(scope.key.clone(), initial.clone())]).unwrap();
    store
        .commit_synced(&scope, None, &initial, &state, None)
        .unwrap();
    let snapshot = codec::encode_composer(&scope.key, &local("remote").composer).unwrap();
    let raw = json!({"ok":true,"draft":{"id":base.id,"client_msg_id":base.client_msg_id,"last_updated_ts":"11.000001","team_id":"T1","user_id":"U1","is_deleted":false,"is_sent":false,"date_scheduled":0,"blocks":snapshot.blocks,"file_ids":snapshot.file_ids,"destinations":snapshot.destinations}});
    (scope, raw)
}
fn setup() -> (DraftStore, Scope, Value, LiveLease) {
    let mut store = DraftStore::open(":memory:").unwrap();
    let (scope, raw) = seed(&mut store);
    storage::activate(&mut store, &scope.account_id).unwrap();
    let previous = storage::team(&store.0, &scope.key.team).unwrap();
    let lease =
        storage::open_stream(&mut store, &scope.account_id, &scope.key.team, &previous).unwrap();
    (store, scope, raw, lease)
}
fn review(store: &DraftStore, scope: &Scope) -> ProtectedReview {
    storage::review_row(&store.0, scope).unwrap()
}
fn event_for(expected: &ProtectedReview) -> DraftObservation {
    let mut remote = expected.row.stored.state.acknowledged().unwrap().clone();
    remote.last_updated_ts = "12.000001".into();
    remote.snapshot =
        codec::encode_composer(&remote.scope.key, &local("observed content").composer).unwrap();
    DraftObservation {
        scope: remote.scope.clone(),
        draft_id: remote.id.clone(),
        client_msg_id: remote.client_msg_id.clone(),
        event_ts: "12.000002".into(),
        kind: ObservationKind::Active(remote),
    }
}
fn verify(store: &mut DraftStore, scope: &Scope, raw: Value, lease: &LiveLease) {
    let expected = review(store, scope);
    let ticket = readback::reserve(store, lease, &expected, 1000).unwrap();
    readback::finish_positive(
        store,
        &ticket,
        raw,
        "U1",
        &RemoteChoice::Local(LocalChoice::Current),
    )
    .unwrap();
}

#[test]
fn activation_preserves_semantics_and_rejects_old_reads_and_writes() {
    let mut store = DraftStore::open(":memory:").unwrap();
    let (scope, _) = seed(&mut store);
    let old = store.read_local_review(&scope).unwrap();
    storage::activate(&mut store, &scope.account_id).unwrap();
    let current = review(&store, &scope);
    assert_eq!(current.row.stored.state, old.stored.unwrap().state);
    assert!(same_local(&current.review.local, &old.local));
    assert_eq!(current.row.ownership.verified_epoch, 0);
    assert!(valid_uuid(&current.row.ownership.enrollment_id));
    assert!(store.read_local_review(&scope).is_err());
    assert!(
        store
            .0
            .execute("UPDATE composer_draft_sync SET format_version=5", [])
            .is_err()
    );
    assert!(
        store
            .0
            .execute(
                "INSERT INTO composer_draft_sync VALUES('other',1,5,'{}')",
                []
            )
            .is_err()
    );
}

#[test]
fn strict_v6_keeps_v5_required_fields_and_rejects_nested_duplicates() {
    let (store, scope, _, _) = setup();
    let row = review(&store, &scope).row;
    let encoded = wire::encode(&row).unwrap();
    assert!(
        wire::decode(&scope, row.stored.revision as i64, 6, &encoded)
            .unwrap()
            .matches(&row)
    );
    for key in ["check_ticket", "check_deadline"] {
        let mut raw: Value = serde_json::from_str(&encoded).unwrap();
        raw["state"].as_object_mut().unwrap().remove(key);
        assert!(wire::decode(&scope, 1, 6, &raw.to_string()).is_err());
        let duplicate = encoded.replace(
            &format!("\"{key}\":null"),
            &format!("\"{key}\":null,\"{key}\":null"),
        );
        assert_ne!(duplicate, encoded);
        assert!(wire::decode(&scope, 1, 6, &duplicate).is_err());
    }
    let mut raw: Value = serde_json::from_str(&encoded).unwrap();
    raw["ignored_metadata"] = json!(true);
    assert!(wire::decode(&scope, 1, 6, &raw.to_string()).is_err());
}

#[test]
fn incarnation_and_verification_are_independent_full_row_cas_inputs() {
    let (mut store, scope, _, _) = setup();
    let expected = review(&store, &scope);
    for incarnation in [true, false] {
        let mut changed = expected.row.clone();
        if incarnation {
            changed.ownership.enrollment_id = uuid::Uuid::new_v4().to_string();
        } else {
            changed.ownership.verified_epoch = 42;
        }
        storage::write_row(&store.0, &changed).unwrap();
        assert!(storage::checkpoint(&mut store, &expected, &local("not accepted"), None).is_err());
        storage::write_row(&store.0, &expected.row).unwrap();
    }
    assert_eq!(
        review(&store, &scope).review.local.composer.text,
        "original"
    );
}

#[test]
fn positive_exact_review_allows_mutation_but_local_typing_invalidates_read() {
    let (mut store, scope, raw, lease) = setup();
    let expected = review(&store, &scope);
    assert!(storage::prepare(&mut store, &lease, &expected).is_err());
    let ticket = readback::reserve(&mut store, &lease, &expected, 1000).unwrap();
    storage::checkpoint(&mut store, &expected, &local("new typing"), None).unwrap();
    assert!(
        readback::finish_positive(
            &mut store,
            &ticket,
            raw.clone(),
            "U1",
            &RemoteChoice::Remote
        )
        .is_err()
    );
    assert_eq!(review(&store, &scope).row.ownership.verified_epoch, 0);
    readback::finish_nonpositive(&mut store, &ticket, KnownReadFailure::Unsupported, 0).unwrap();
    let current = review(&store, &scope);
    let ticket = readback::reserve(&mut store, &lease, &current, 31000).unwrap();
    readback::finish_positive(
        &mut store,
        &ticket,
        raw,
        "U1",
        &RemoteChoice::Local(LocalChoice::Current),
    )
    .unwrap();
    let current = review(&store, &scope);
    assert_eq!(current.review.local.composer.text, "new typing");
    assert_eq!(current.row.ownership.verified_epoch, lease.epoch);
    assert!(readback::reserve(&mut store, &lease, &current, 999999).is_err());
    assert!(storage::prepare(&mut store, &lease, &current).is_ok());
}

#[test]
fn wrong_positive_identity_and_version_leave_admission_unresolved() {
    for field in ["id", "client_msg_id", "last_updated_ts"] {
        let (mut store, scope, mut raw, lease) = setup();
        let expected = review(&store, &scope);
        let ticket = readback::reserve(&mut store, &lease, &expected, 1000).unwrap();
        raw["draft"][field] = json!(if field == "last_updated_ts" {
            "9.000001"
        } else {
            "wrong"
        });
        assert!(
            readback::finish_positive(&mut store, &ticket, raw, "U1", &RemoteChoice::Remote)
                .is_err()
        );
        assert!(storage::team(&store.0, "T1").unwrap().pending.is_some());
        assert!(review(&store, &scope).matches(&expected));
    }
}

#[test]
fn failed_gap_write_revokes_runtime_and_before_after_race_is_fenced() {
    let (mut store, scope, raw, lease) = setup();
    verify(&mut store, &scope, raw, &lease);
    let expected = review(&store, &scope);
    // Mutation that committed before a gap remains an immutable receipt owner.
    let (_, operation) = storage::prepare(&mut store, &lease, &expected).unwrap();
    store.0.execute_batch("CREATE TRIGGER fail_gap BEFORE UPDATE ON composer_draft_continuity BEGIN SELECT RAISE(ABORT,'injected storage failure'); END;").unwrap();
    assert!(storage::mark_gap(&mut store, &lease, "overflow").is_err());
    assert!(lease.live_guard().is_err());
    assert!(storage::team(&store.0, "T1").unwrap().mode == Mode::Open);
    let current = review(&store, &scope);
    assert_eq!(current.row.stored.state.pending(), Some(&operation));
    assert!(storage::prepare(&mut store, &lease, &current).is_err());
    assert!(readback::reserve(&mut store, &lease, &current, 100000).is_err());
    // A second otherwise identical owner has no pending operation. The control
    // above proved preparation succeeds before revocation; only the failed-gap
    // runtime fence now prevents this still-dispatchable state from preparing.
    let (mut store, scope, raw, lease) = setup();
    verify(&mut store, &scope, raw, &lease);
    let expected = review(&store, &scope);
    let mut probe = expected.row.stored.state.clone();
    assert!(probe.begin_operation(&scope).is_some());
    store.0.execute_batch("CREATE TRIGGER fail_gap BEFORE UPDATE ON composer_draft_continuity BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(storage::mark_gap(&mut store, &lease, "overflow").is_err());
    assert!(storage::prepare(&mut store, &lease, &expected).is_err());
    assert!(review(&store, &scope).matches(&expected));
}

#[test]
fn pending_read_restart_and_takeover_never_replay_or_borrow_loaded_lease() {
    let db = Database::new();
    let mut store = db.open();
    let (scope, _) = seed(&mut store);
    storage::activate(&mut store, &scope.account_id).unwrap();
    let prior = storage::team(&store.0, "T1").unwrap();
    let lease = storage::open_stream(&mut store, &scope.account_id, "T1", &prior).unwrap();
    let expected = review(&store, &scope);
    let ticket = readback::reserve(&mut store, &lease, &expected, 1000).unwrap();
    drop(ticket);
    drop(store); // No response retained. There is no deserialize-to-ticket API.
    let mut store = db.open();
    let loaded = storage::team(&store.0, "T1").unwrap();
    assert!(loaded.pending.is_some());
    let replacement = storage::open_stream(&mut store, &scope.account_id, "T1", &loaded).unwrap();
    assert_ne!(replacement.id, lease.id);
    assert!(replacement.epoch > lease.epoch);
    assert!(readback::reserve(&mut store, &replacement, &expected, 999999).is_err());
    assert!(
        storage::reserve_observation(&mut store, &lease, &expected, &event_for(&expected)).is_err()
    );
    assert_eq!(
        storage::team(&store.0, "T1").unwrap().read_retry_at_ms,
        31000
    );
}

#[test]
fn late_429_survives_replacement_and_reopen_without_clearing_new_work() {
    let db = Database::new();
    let mut store = db.open();
    let (scope, _) = seed(&mut store);
    storage::activate(&mut store, &scope.account_id).unwrap();
    let prior = storage::team(&store.0, "T1").unwrap();
    let lease = storage::open_stream(&mut store, &scope.account_id, "T1", &prior).unwrap();
    let expected = review(&store, &scope);
    let ticket = readback::reserve(&mut store, &lease, &expected, 1000).unwrap();
    let prior = storage::team(&store.0, "T1").unwrap();
    let replacement = storage::open_stream(&mut store, &scope.account_id, "T1", &prior).unwrap();
    let newer =
        storage::reserve_observation(&mut store, &replacement, &expected, &event_for(&expected))
            .unwrap();
    readback::finish_nonpositive(&mut store, &ticket, KnownReadFailure::RateLimited, 121000)
        .unwrap();
    drop(store);
    let mut store = db.open();
    let retained = storage::team(&store.0, "T1").unwrap();
    assert_eq!(retained.read_retry_at_ms, 121000);
    assert_eq!(retained.pending, Some(newer.pending));
    assert!(readback::reserve(&mut store, &replacement, &expected, 120999).is_err());
    assert!(
        readback::finish_nonpositive(&mut store, &ticket, KnownReadFailure::ApiError, 0).is_err()
    );
    assert_eq!(
        storage::team(&store.0, "T1").unwrap().read_retry_at_ms,
        121000
    );
}

#[test]
fn observation_and_admission_completion_roll_back_together() {
    let (mut store, scope, _, lease) = setup();
    let expected = review(&store, &scope);
    let event = event_for(&expected);
    let admission = storage::reserve_observation(&mut store, &lease, &expected, &event).unwrap();
    store.0.execute_batch("CREATE TRIGGER fail_completion BEFORE UPDATE ON composer_draft_continuity BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(storage::finish_observation(&mut store, &admission, &event).is_err());
    assert!(review(&store, &scope).matches(&expected));
    assert_eq!(
        review(&store, &scope).review.local.composer.text,
        "original"
    );
    assert_eq!(
        storage::team(&store.0, "T1").unwrap().pending,
        Some(admission.pending.clone())
    );
    store
        .0
        .execute_batch("DROP TRIGGER fail_completion;")
        .unwrap();
    storage::finish_observation(&mut store, &admission, &event).unwrap();
    assert!(storage::team(&store.0, "T1").unwrap().pending.is_none());
    assert_eq!(review(&store, &scope).row.ownership.verified_epoch, 0);
    assert_eq!(
        review(&store, &scope).review.local.composer.text,
        "observed content"
    );
}

#[test]
fn exact_terminal_receipt_during_gap_retains_new_input_and_unverifies_successor() {
    let mut store = DraftStore::open(":memory:").unwrap();
    let (scope, _) = seed(&mut store);
    let old = store.read_local_review(&scope).unwrap();
    let mut state = old.stored.as_ref().unwrap().state.clone();
    state
        .request_consume(&scope, state.local_revision(), ConsumeKind::Send)
        .unwrap();
    store
        .commit_synced(
            &scope,
            Some(old.stored.unwrap().revision),
            &LocalDraft::default(),
            &state,
            None,
        )
        .unwrap();
    let previous = store.read_local_review(&scope).unwrap().stored.unwrap();
    let crate::drafts::store::sync::Prepare::Prepared { operation, .. } =
        store.prepare_synced(&scope, previous.revision).unwrap()
    else {
        panic!("send admission");
    };
    storage::activate(&mut store, &scope.account_id).unwrap();
    // Fixture for an operation admitted while this owner's prior epoch was
    // verified. Its exact terminal receipt must clear that verification.
    let mut admitted = review(&store, &scope).row;
    admitted.ownership.verified_epoch = 7;
    storage::write_row(&store.0, &admitted).unwrap();
    let before = review(&store, &scope);
    let intent = before.row.stored.state.consumption().unwrap().clone();
    storage::checkpoint(
        &mut store,
        &before,
        &local("new input during receipt"),
        Some(&intent),
    )
    .unwrap();
    let current = review(&store, &scope);
    let after = storage::receipt(
        &mut store,
        &current,
        &operation,
        storage::Receipt::Sent {
            client_msg_id: operation.client_msg_id.clone(),
            remote_id: operation.base.as_ref().map(|r| r.id.clone()),
            message_ts: "30.000001".into(),
        },
    )
    .unwrap();
    assert_eq!(
        after.ownership.enrollment_id,
        before.row.ownership.enrollment_id
    );
    assert_eq!(after.ownership.verified_epoch, 0);
    assert_ne!(after.stored.state.client_msg_id(), operation.client_msg_id);
    assert_eq!(
        review(&store, &scope).review.local.composer.text,
        "new input during receipt"
    );
    assert!(after.stored.state.has_remote_identity_history());
    let current = review(&store, &scope);
    assert!(storage::release(&mut store, &current).is_err());
    storage::checkpoint(&mut store, &current, &local("after receipt"), Some(&intent)).unwrap();
}

#[test]
fn old_history_cannot_be_released_even_when_workflow_is_idle() {
    let (mut store, scope, _, _) = setup();
    let current = review(&store, &scope);
    assert_eq!(current.row.stored.state.workflow(), &Workflow::Idle);
    assert!(storage::release(&mut store, &current).is_err());
}

#[test]
fn identity_free_release_reenrollment_changes_nonce_even_with_identical_state_and_revision() {
    let (mut store, mut scope, _, _) = setup();
    scope.key.channel = "C2".into();
    let empty = LocalDraft::default();
    let state = DraftSync::new(scope.clone());
    storage::enroll_identity_free(&mut store, &scope, &empty, &state).unwrap();
    let first = review(&store, &scope);
    storage::release(&mut store, &first).unwrap();
    storage::enroll_identity_free(&mut store, &scope, &empty, &state).unwrap();
    let second = review(&store, &scope);
    assert_eq!(first.row.stored.revision, second.row.stored.revision);
    assert_eq!(first.row.stored.state, second.row.stored.state);
    assert_ne!(
        first.row.ownership.enrollment_id,
        second.row.ownership.enrollment_id
    );
    assert!(!first.matches(&second));
    assert!(storage::checkpoint(&mut store, &first, &local("stale owner"), None).is_err());
}

#[test]
fn omitted_or_contradictory_nullable_team_guards_never_erase_pending_admission() {
    let (mut store, scope, raw, lease) = setup();
    verify(&mut store, &scope, raw, &lease);
    let expected = review(&store, &scope);
    storage::reserve_observation(&mut store, &lease, &expected, &event_for(&expected)).unwrap();
    let original: String = store
        .0
        .query_row(
            "SELECT content FROM composer_draft_continuity WHERE team='T1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    for field in [
        "lease_id",
        "pending",
        "last_interrupted_id",
        "gap_reason",
        "contradictory_mode",
        "bad_last_id",
    ] {
        let mut corrupt: Value = serde_json::from_str(&original).unwrap();
        match field {
            "contradictory_mode" => {
                corrupt["mode"] = json!("Gap");
                corrupt["lease_id"] = Value::Null;
            }
            "bad_last_id" => corrupt["last_interrupted_id"] = json!("invalid"),
            field => {
                corrupt.as_object_mut().unwrap().remove(field);
            }
        }
        store
            .0
            .execute(
                "UPDATE composer_draft_continuity SET content=?1 WHERE team='T1'",
                [corrupt.to_string()],
            )
            .unwrap();
        assert!(storage::team(&store.0, "T1").is_err());
        assert!(storage::prepare(&mut store, &lease, &expected).is_err());
        assert!(review(&store, &scope).matches(&expected));
    }
}

#[test]
fn checkpoint_preserves_caret_only_raw_broadcast_and_acknowledged_empty_snapshot() {
    for text in ["*rich* text", ""] {
        let mut store = DraftStore::open(":memory:").unwrap();
        let scope = Scope {
            account_id: uuid::Uuid::new_v4().to_string(),
            key: DraftKey {
                team: "T1".into(),
                channel: "C1".into(),
                thread: Some("1.000001".into()),
            },
        };
        let mut raw = codec::encode_composer(&scope.key, &local(text).composer).unwrap();
        raw.destinations[0]["broadcast"] = json!(false);
        let projection = codec::decode(&scope.key, &raw).unwrap();
        let initial = LocalDraft {
            composer: projection.composer().clone(),
            attachments: vec![],
        };
        let remote = RemoteDraft {
            scope: scope.clone(),
            id: "D1".into(),
            client_msg_id: uuid::Uuid::new_v4().to_string(),
            last_updated_ts: "10.000001".into(),
            snapshot: raw.clone(),
        };
        let state = DraftSync::adopt_remote(&scope, remote).unwrap();
        store.save([(scope.key.clone(), initial.clone())]).unwrap();
        store
            .commit_synced(&scope, None, &initial, &state, None)
            .unwrap();
        storage::activate(&mut store, &scope.account_id).unwrap();
        let before = review(&store, &scope);
        let mut caret = initial.clone();
        caret.composer.set_selection(0, text.len());
        let after = storage::checkpoint(&mut store, &before, &caret, None).unwrap();
        assert_eq!(after.stored.state.desired(), Some(&raw));
        assert_eq!(
            after.stored.state.local_revision(),
            before.row.stored.state.local_revision()
        );
        assert!(!after.stored.state.tombstone());
        assert!(same_local(&review(&store, &scope).review.local, &caret));
        if !text.is_empty() {
            let current = review(&store, &scope);
            let changed = local("changed");
            let changed = storage::checkpoint(&mut store, &current, &changed, None).unwrap();
            assert_eq!(
                changed.stored.state.desired().unwrap().destinations[0]["broadcast"],
                json!(false)
            );
        }
    }
}

#[test]
fn identity_collision_added_after_admission_blocks_reduction_and_new_admission() {
    let (mut store, scope, _, lease) = setup();
    let expected = review(&store, &scope);
    let event = event_for(&expected);
    let admission = storage::reserve_observation(&mut store, &lease, &expected, &event).unwrap();
    let mut second_scope = scope.clone();
    second_scope.key.channel = "C2".into();
    let mut remote = expected.row.stored.state.acknowledged().unwrap().clone();
    remote.scope = second_scope.clone();
    remote.snapshot =
        codec::encode_composer(&second_scope.key, &local("collision").composer).unwrap();
    let second = ProtectedRow {
        stored: Stored {
            revision: 1,
            state: DraftSync::adopt_remote(&second_scope, remote).unwrap(),
        },
        ownership: Ownership {
            enrollment_id: uuid::Uuid::new_v4().to_string(),
            verified_epoch: 0,
        },
    };
    store.0.execute("INSERT INTO composer_draft_sync(destination,storage_revision,format_version,state) VALUES(?1,1,6,?2)",params![serde_json::to_string(&second_scope.key).unwrap(),wire::encode(&second).unwrap()]).unwrap();
    assert!(storage::finish_observation(&mut store, &admission, &event).is_err());
    assert!(review(&store, &scope).matches(&expected));
    let current = storage::team(&store.0, "T1").unwrap();
    let replacement = storage::open_stream(&mut store, &scope.account_id, "T1", &current).unwrap();
    assert!(storage::reserve_observation(&mut store, &replacement, &expected, &event).is_err());
    let second = review(&store, &second_scope);
    assert!(
        storage::reserve_observation(&mut store, &replacement, &second, &event_for(&second))
            .is_err()
    );
    assert!(readback::reserve(&mut store, &replacement, &expected, 1000).is_err());
}

#[test]
fn read_inventory_rejects_hidden_team_malformed_counters_and_missing_completion() {
    let (mut store, scope, _, lease) = setup();
    let expected = review(&store, &scope);
    let ticket = readback::reserve(&mut store, &lease, &expected, 1000).unwrap();
    let original: String = store
        .0
        .query_row(
            "SELECT content FROM composer_draft_continuity_reads WHERE id=?1",
            [&ticket.admission.pending.id],
            |r| r.get(0),
        )
        .unwrap();
    let prior = storage::team(&store.0, "T1").unwrap();
    let replacement = storage::open_stream(&mut store, &scope.account_id, "T1", &prior).unwrap();
    for kind in [
        "physical_team",
        "floor",
        "epoch",
        "sequence",
        "completion",
        "finished",
    ] {
        let mut raw: Value = serde_json::from_str(&original).unwrap();
        match kind {
            "floor" => raw["floor_ms"] = json!(0),
            "epoch" => raw["epoch"] = json!(u64::MAX),
            "sequence" => raw["sequence"] = json!(0),
            "completion" => {
                raw.as_object_mut().unwrap().remove("completion");
            }
            "finished" => raw["state"] = json!("Finished"),
            _ => {}
        }
        let physical = if kind == "physical_team" {
            "hidden-team"
        } else {
            "T1"
        };
        store
            .0
            .execute(
                "UPDATE composer_draft_continuity_reads SET team=?1,content=?2",
                params![physical, raw.to_string()],
            )
            .unwrap();
        assert!(inventory::reads(&store.0).is_err());
        assert!(readback::reserve(&mut store, &replacement, &expected, 999999).is_err());
        assert!(review(&store, &scope).matches(&expected));
    }
}

#[test]
fn pending_operation_cannot_reserve_an_unusable_reconciliation_read() {
    let (mut store, scope, raw, lease) = setup();
    verify(&mut store, &scope, raw, &lease);
    let current = review(&store, &scope);
    storage::prepare(&mut store, &lease, &current).unwrap();
    let prior = storage::team(&store.0, "T1").unwrap();
    let replacement = storage::open_stream(&mut store, &scope.account_id, "T1", &prior).unwrap();
    let before = storage::team(&store.0, "T1").unwrap();
    let reads = inventory::reads(&store.0).unwrap();
    let current = review(&store, &scope);
    assert!(readback::reserve(&mut store, &replacement, &current, 999999).is_err());
    assert_eq!(storage::team(&store.0, "T1").unwrap(), before);
    assert_eq!(inventory::reads(&store.0).unwrap(), reads);
}

#[test]
fn historical_terminal_uuid_and_completed_check_block_only_colliding_destination() {
    for deadline_only in [false, true] {
        for collides in [false, true] {
            let (mut store, scope, _, lease) = setup();
            let expected = review(&store, &scope);
            let mut other_scope = scope.clone();
            other_scope.key.channel = "C2".into();
            let client = if collides {
                expected.row.stored.state.client_msg_id().to_owned()
            } else {
                uuid::Uuid::new_v4().to_string()
            };
            let snapshot =
                codec::encode_composer(&other_scope.key, &local("old consumed content").composer)
                    .unwrap();
            let remote = RemoteDraft {
                scope: other_scope.clone(),
                id: "D-old-other".into(),
                client_msg_id: client.clone(),
                last_updated_ts: "8.000001".into(),
                snapshot: snapshot.clone(),
            };
            let mut history = DraftSync::adopt_remote(&other_scope, remote).unwrap();
            history
                .request_consume(&other_scope, history.local_revision(), ConsumeKind::Send)
                .unwrap();
            let operation = history.begin_operation(&other_scope).unwrap();
            if deadline_only {
                assert!(history.mark_uncertain(&other_scope, &operation.id));
                let ticket = history.reserve_check(&operation, 1).unwrap();
                assert!(history.finish_check(&ticket, 30001));
            }
            assert!(history.confirm_sent(
                &other_scope,
                &operation.id,
                &client,
                Some("D-old-other"),
                "20.000001"
            ));
            if deadline_only {
                // Starting a new operation clears local_completion, leaving the
                // old full CheckDeadline operation as the sole old UUID claim.
                history.edit(&other_scope, Some(snapshot));
                history.begin_operation(&other_scope).unwrap();
            }
            let row = ProtectedRow {
                stored: Stored {
                    revision: 1,
                    state: history,
                },
                ownership: Ownership {
                    enrollment_id: uuid::Uuid::new_v4().to_string(),
                    verified_epoch: 0,
                },
            };
            write_local(
                &store.0,
                &other_scope.key,
                &storage::project(&row.stored.state).unwrap(),
            )
            .unwrap();
            store.0.execute("INSERT INTO composer_draft_sync(destination,storage_revision,format_version,state) VALUES(?1,1,6,?2)",params![serde_json::to_string(&other_scope.key).unwrap(),wire::encode(&row).unwrap()]).unwrap();
            let admitted =
                storage::reserve_observation(&mut store, &lease, &expected, &event_for(&expected));
            assert_eq!(admitted.is_err(), collides);
            assert!(review(&store, &scope).matches(&expected));
        }
    }
}
