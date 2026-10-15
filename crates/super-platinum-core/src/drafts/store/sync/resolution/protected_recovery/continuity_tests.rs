//! Disconnected v7 facade regressions. Rustfmt-parsed; not compiled/run by author.
use super::*;
use crate::drafts::store::sync::tests::{Database, local};
use crate::drafts::sync::{ConsumeKind, OperationKind, RejectedScheduleChoice, RemoteDraft};
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
    let local = local("original");
    let base = RemoteDraft {
        scope: scope.clone(),
        id: "D1".into(),
        client_msg_id: uuid::Uuid::new_v4().to_string(),
        last_updated_ts: "10.000001".into(),
        snapshot: codec::encode_composer(&scope.key, &local.composer).unwrap(),
    };
    let state = DraftSync::adopt_remote(&scope, base.clone()).unwrap();
    store.save([(scope.key.clone(), local.clone())]).unwrap();
    store
        .commit_synced(&scope, None, &local, &state, None)
        .unwrap();
    let raw = json!({"ok":true,"draft":{"id":base.id,"client_msg_id":base.client_msg_id,"last_updated_ts":"11.000001","team_id":"T1","user_id":"U1","is_deleted":false,"is_sent":false,"date_scheduled":0,"blocks":base.snapshot.blocks,"file_ids":base.snapshot.file_ids,"destinations":base.snapshot.destinations}});
    (scope, raw)
}
fn review(store: &DraftStore, scope: &Scope) -> ProtectedReview {
    storage::review_row(&store.0, scope).unwrap()
}
fn open(store: &mut DraftStore, scope: &Scope) -> LiveLease {
    let old = storage::team(&store.0, &scope.key.team).unwrap();
    storage::open_stream(store, &scope.account_id, &scope.key.team, &old).unwrap()
}
fn setup() -> (DraftStore, Scope, LiveLease) {
    let mut store = DraftStore::open(":memory:").unwrap();
    let (scope, raw) = seed(&mut store);
    storage::activate(&mut store, &scope.account_id).unwrap();
    let lease = open(&mut store, &scope);
    let expected = review(&store, &scope);
    let ticket = readback::reserve(&mut store, &lease, &expected, 1000).unwrap();
    readback::finish_positive(
        &mut store,
        &ticket,
        raw,
        "U1",
        &RemoteChoice::Local(LocalChoice::Current),
    )
    .unwrap();
    (store, scope, lease)
}
fn update(store: &mut DraftStore, scope: &Scope, lease: &LiveLease) -> facade::ReceiptTarget {
    let expected = review(store, scope);
    facade::preserve(store, &expected, &local("requested"), None).unwrap();
    let expected = review(store, scope);
    let (row, op) = storage::prepare(store, lease, &expected).unwrap();
    assert_eq!(op.kind, OperationKind::Update);
    facade::ReceiptTarget::for_row(&row).unwrap()
}
fn sent(store: &mut DraftStore, scope: &Scope, lease: &LiveLease) -> facade::ReceiptTarget {
    let expected = review(store, scope);
    facade::consume(store, lease, &expected, ConsumeKind::Send).unwrap();
    let expected = review(store, scope);
    let (row, op) = storage::prepare(store, lease, &expected).unwrap();
    assert_eq!(op.kind, OperationKind::Send);
    facade::ReceiptTarget::for_row(&row).unwrap()
}
fn scheduled(store: &mut DraftStore, scope: &Scope, lease: &LiveLease) -> facade::ReceiptTarget {
    let expected = review(store, scope);
    let mut rich = expected.row.stored.state.desired().unwrap().clone();
    rich.blocks = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"reviewed","style":{"bold":true}}]}]}),
    ];
    facade::reviewed_schedule(store, lease, &expected, &rich, 2_000_000_000).unwrap();
    let expected = review(store, scope);
    let (row, op) = storage::prepare(store, lease, &expected).unwrap();
    assert!(op.known_schedule_check());
    facade::ReceiptTarget::for_row(&row).unwrap()
}
fn positive(op: &Operation) -> facade::Outcome {
    match op.kind {
        OperationKind::Send => facade::Outcome::Sent {
            client_msg_id: op.client_msg_id.clone(),
            draft_id: op.base.as_ref().map(|b| b.id.clone()),
            ts: "20.000001".into(),
        },
        OperationKind::Update => facade::Outcome::Active(RemoteDraft {
            snapshot: op.snapshot.clone().unwrap(),
            last_updated_ts: "20.000001".into(),
            ..op.base.clone().unwrap()
        }),
        OperationKind::Schedule { post_at } => facade::Outcome::Scheduled {
            remote: RemoteDraft {
                snapshot: op.snapshot.clone().unwrap(),
                last_updated_ts: "20.000001".into(),
                ..op.base.clone().unwrap()
            },
            post_at,
        },
        _ => panic!("unexpected operation"),
    }
}
fn takeover(store: &mut DraftStore, scope: &Scope) -> recovery::Permit {
    let prior = recovery::owner_record(&store.0, &scope.key.team).unwrap();
    recovery::take_over(store, &scope.account_id, &scope.key.team, prior.as_ref()).unwrap()
}
fn check(
    store: &mut DraftStore,
    scope: &Scope,
    target: &facade::ReceiptTarget,
) -> (recovery::Permit, recovery::Completion) {
    facade::record(store, target, &facade::Outcome::Uncertain).unwrap();
    let permit = takeover(store, scope);
    let expected = review(store, scope);
    let ticket = recovery::reserve(store, &permit, &expected, &target.operation, 100_000).unwrap();
    let recovery::Claim::Ready {
        operation: op,
        completion,
    } = recovery::claim(store, &permit, ticket, 100_000).unwrap()
    else {
        panic!("unexpected cooldown")
    };
    assert_eq!(op, target.operation);
    (permit, completion)
}

#[test]
fn v7_requires_origin_basis_and_nested_recovery_guards() {
    let (store, scope, _) = setup();
    let row = review(&store, &scope).row;
    let encoded = wire::encode(&row).unwrap();
    for field in ["origin", "basis"] {
        let mut raw: Value = serde_json::from_str(&encoded).unwrap();
        raw.as_object_mut().unwrap().remove(field);
        assert!(wire::decode(&scope, 1, 7, &raw.to_string()).is_err());
    }
    for field in ["check_ticket", "check_deadline"] {
        let mut raw: Value = serde_json::from_str(&encoded).unwrap();
        raw["state"].as_object_mut().unwrap().remove(field);
        assert!(wire::decode(&scope, 1, 7, &raw.to_string()).is_err());
    }
    let duplicate = encoded.replace(
        "\"check_ticket\":null",
        "\"check_ticket\":null,\"check_ticket\":null",
    );
    assert_ne!(duplicate, encoded);
    assert!(wire::decode(&scope, 1, 7, &duplicate).is_err());
    assert!(wire::decode(&scope, 1, 6, &encoded).is_err());
    let mut raw: Value = serde_json::from_str(&encoded).unwrap();
    raw["basis"] = json!("Local");
    assert!(wire::decode(&scope, 1, 7, &raw.to_string()).is_err());
}
#[test]
fn local_origin_is_minted_by_enrollment_not_imported_empty_remote_state() {
    let mut store = DraftStore::open(":memory:").unwrap();
    let scope = crate::drafts::store::sync::tests::scope();
    let local = local("local only");
    store.save([(scope.key.clone(), local.clone())]).unwrap();
    let mut old = DraftSync::new(scope.clone());
    old.edit(
        &scope,
        Some(codec::encode_composer(&scope.key, &local.composer).unwrap()),
    );
    store
        .commit_synced(&scope, None, &local, &old, None)
        .unwrap();
    storage::activate(&mut store, &scope.account_id).unwrap();
    let lease = open(&mut store, &scope);
    let imported = review(&store, &scope);
    assert!(facade::verify_local(&mut store, &lease, &imported).is_err());
    archive::release(&mut store, &imported).unwrap();
    let row = facade::enroll_local(&mut store, &scope, &local).unwrap();
    assert_ne!(
        row.ownership.enrollment_id,
        imported.row.ownership.enrollment_id
    );
    assert_ne!(row.stored.state.client_msg_id(), old.client_msg_id());
    let expected = review(&store, &scope);
    facade::verify_local(&mut store, &lease, &expected).unwrap();
    let expected = review(&store, &scope);
    assert_eq!(expected.row.ownership.basis, Basis::Local);
    facade::consume(&mut store, &lease, &expected, ConsumeKind::Send).unwrap();
}
#[test]
fn update_check_commits_on_latest_owner_without_erasing_typing() {
    let (mut store, scope, lease) = setup();
    let target = update(&mut store, &scope, &lease);
    let (_, completion) = check(&mut store, &scope, &target);
    let expected = review(&store, &scope);
    facade::preserve(&mut store, &expected, &local("typed during check"), None).unwrap();
    recovery::complete(
        &mut store,
        &completion,
        recovery::ResultKind::Positive(positive(&target.operation)),
        0,
    )
    .unwrap();
    let after = review(&store, &scope);
    assert_eq!(after.review.local.composer.text, "typed during check");
    assert!(after.row.stored.state.pending().is_none());
    assert_eq!(
        after.row.stored.state.acknowledged().unwrap().snapshot,
        target.operation.snapshot.unwrap()
    );
    assert_ne!(
        after.row.ownership.verified_epoch,
        storage::team(&store.0, &scope.key.team).unwrap().epoch
    );
}
#[test]
fn send_check_preserves_newer_input_and_successor_needs_new_local_basis() {
    let (mut store, scope, lease) = setup();
    let target = sent(&mut store, &scope, &lease);
    let (_, completion) = check(&mut store, &scope, &target);
    let expected = review(&store, &scope);
    let intent = expected
        .row
        .stored
        .state
        .local_preservation_intent()
        .cloned()
        .unwrap();
    facade::preserve(&mut store, &expected, &local("newer"), Some(&intent)).unwrap();
    recovery::complete(
        &mut store,
        &completion,
        recovery::ResultKind::Positive(positive(&target.operation)),
        0,
    )
    .unwrap();
    let after = review(&store, &scope);
    assert_eq!(after.review.local.composer.text, "newer");
    assert_eq!(after.row.ownership.basis, Basis::Unverified);
    assert_eq!(after.row.ownership.verified_epoch, 0);
    let lease = open(&mut store, &scope);
    assert!(storage::prepare(&mut store, &lease, &after).is_err());
    facade::verify_local(&mut store, &lease, &after).unwrap();
    let after = review(&store, &scope);
    facade::consume(&mut store, &lease, &after, ConsumeKind::Send).unwrap();
}
#[test]
fn schedule_check_uses_exact_frozen_rich_payload_and_retains_newer_input() {
    let (mut store, scope, lease) = setup();
    let target = scheduled(&mut store, &scope, &lease);
    let (_, completion) = check(&mut store, &scope, &target);
    let expected = review(&store, &scope);
    let intent = expected
        .row
        .stored
        .state
        .local_preservation_intent()
        .cloned()
        .unwrap();
    facade::preserve(&mut store, &expected, &local("independent"), Some(&intent)).unwrap();
    recovery::complete(
        &mut store,
        &completion,
        recovery::ResultKind::Positive(positive(&target.operation)),
        0,
    )
    .unwrap();
    let after = review(&store, &scope);
    assert_eq!(after.review.local.composer.text, "independent");
    assert!(after.row.stored.state.pending().is_none());
    assert!(after.row.stored.state.consumption().is_none());
    assert_eq!(after.row.ownership.enrollment_id, target.enrollment);
}
#[test]
fn rejected_schedule_review_changes_only_frozen_intent_not_newer_composer() {
    let (mut store, scope, lease) = setup();
    let target = scheduled(&mut store, &scope, &lease);
    facade::record(&mut store, &target, &facade::Outcome::Rejected).unwrap();
    let expected = review(&store, &scope);
    let intent = expected
        .row
        .stored
        .state
        .local_preservation_intent()
        .cloned()
        .unwrap();
    facade::preserve(
        &mut store,
        &expected,
        &local("new independent draft"),
        Some(&intent),
    )
    .unwrap();
    let expected = review(&store, &scope);
    let snapshot = target.operation.snapshot.clone().unwrap();
    let choice = RejectedScheduleChoice::Replace {
        snapshot: snapshot.clone(),
        post_at: 2_000_000_100,
    };
    let row =
        facade::rejected_schedule(&mut store, &lease, &expected, &choice, 2_000_000_000).unwrap();
    assert_eq!(
        review(&store, &scope).review.local.composer.text,
        "new independent draft"
    );
    assert_eq!(row.stored.state.consumption().unwrap().snapshot, snapshot);
    assert_ne!(row.stored.state.consumption().unwrap().id, intent.id);
    assert!(row.stored.state.pending().is_none());
}
#[test]
fn same_state_under_different_enrollment_cannot_accept_reviewed_schedule() {
    let (mut store, scope, lease) = setup();
    let expected = review(&store, &scope);
    let mut replacement = expected.row.clone();
    replacement.ownership.enrollment_id = uuid::Uuid::new_v4().to_string();
    storage::write_row(&store.0, &replacement).unwrap();
    assert!(
        facade::reviewed_schedule(
            &mut store,
            &lease,
            &expected,
            expected.row.stored.state.desired().unwrap(),
            2_000_000_000
        )
        .is_err()
    );
    assert_eq!(
        review(&store, &scope).review.local.composer.text,
        "original"
    );
}
#[test]
fn gap_blocks_new_consumption_but_preserves_edits_and_exact_receipts() {
    let (mut store, scope, lease) = setup();
    let target = update(&mut store, &scope, &lease);
    storage::mark_gap(&mut store, &lease, "overflow").unwrap();
    let expected = review(&store, &scope);
    facade::preserve(&mut store, &expected, &local("saved in gap"), None).unwrap();
    let expected = review(&store, &scope);
    assert!(facade::consume(&mut store, &lease, &expected, ConsumeKind::Send).is_err());
    facade::record(&mut store, &target, &positive(&target.operation)).unwrap();
    assert_eq!(
        review(&store, &scope).review.local.composer.text,
        "saved in gap"
    );
}
#[test]
fn failed_completion_transaction_is_disk_retryable_without_new_admission() {
    let (mut store, scope, lease) = setup();
    let target = update(&mut store, &scope, &lease);
    let (_, completion) = check(&mut store, &scope, &target);
    let before = review(&store, &scope);
    let ledger = recovery::attempts(&store.0).unwrap();
    store.0.execute_batch("CREATE TRIGGER fail_status BEFORE UPDATE ON composer_draft_status_reads BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(
        recovery::complete(
            &mut store,
            &completion,
            recovery::ResultKind::Positive(positive(&target.operation)),
            0
        )
        .is_err()
    );
    assert!(before.matches(&review(&store, &scope)));
    assert_eq!(ledger, recovery::attempts(&store.0).unwrap());
    store.0.execute_batch("DROP TRIGGER fail_status;").unwrap();
    recovery::complete(
        &mut store,
        &completion,
        recovery::ResultKind::Positive(positive(&target.operation)),
        0,
    )
    .unwrap();
    assert_eq!(recovery::attempts(&store.0).unwrap().len(), 1);
}
#[path = "continuity_policy_tests.rs"]
mod policy;
#[path = "continuity_review_tests.rs"]
mod review_regressions;
