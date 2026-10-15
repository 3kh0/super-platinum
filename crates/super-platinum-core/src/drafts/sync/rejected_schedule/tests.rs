use super::*;
use serde_json::json;

fn rejected() -> (DraftSync, ConsumeIntent, Operation) {
    let scope = Scope {
        account_id: "00000000-0000-4000-8000-000000000031".into(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        },
    };
    let raw = Snapshot {
        blocks: vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_quote","elements":[{"type":"text","text":"Frozen rich quote","style":{"bold":true,"italic":true}}]}]}),
        ],
        file_ids: vec![],
        destinations: vec![json!({"channel_id":"C1"})],
    };
    let base = RemoteDraft {
        scope: scope.clone(),
        id: "Dr1".into(),
        client_msg_id: "00000000-0000-4000-8000-000000000032".into(),
        last_updated_ts: "100.000001".into(),
        snapshot: raw,
    };
    let mut state = DraftSync::adopt_remote(&scope, base).unwrap();
    let intent = state
        .request_consume(&scope, 0, ConsumeKind::Schedule { post_at: 500 })
        .unwrap();
    let operation = state.begin_operation(&scope).unwrap();
    assert!(state.reject(&scope, &operation.id));
    (state, intent, operation)
}
fn replacement(old: &ConsumeIntent) -> RejectedScheduleChoice {
    let mut snapshot = old.snapshot.clone();
    snapshot.blocks[0]["elements"][0]["elements"][0]["text"] = json!("Reviewed replacement");
    RejectedScheduleChoice::Replace {
        snapshot,
        post_at: 900,
    }
}

#[test]
fn retry_keeps_entire_intent_and_separate_newer_local_revision() {
    let (mut state, old, operation) = rejected();
    state.checkpoint_direct_local(&old, None).unwrap();
    let expected = state.clone();
    let result = state
        .review_rejected_schedule(&expected, &RejectedScheduleChoice::Retry, 200)
        .unwrap();
    assert_eq!(result, old);
    assert_eq!(state.local_revision(), expected.local_revision());
    assert_eq!(state.desired(), expected.desired());
    assert_eq!(state.acknowledged(), expected.acknowledged());
    assert!(state.accepts_direct_local(&old));
    let next = state.begin_operation(&old.scope).unwrap();
    assert_ne!(next.id, operation.id);
    assert_eq!(next.consume_id, operation.consume_id);
    assert_eq!(next.snapshot, operation.snapshot);
    assert_eq!(next.base, operation.base);
    assert!(state.begin_operation(&old.scope).is_none());
}

#[test]
fn explicit_replacement_changes_only_frozen_intent_and_preserves_current_draft() {
    let (mut state, old, _) = rejected();
    let desired =
        crate::drafts::codec::encode(&old.scope.key, "Current separate composer").unwrap();
    state
        .checkpoint_direct_local(&old, Some(desired.clone()))
        .unwrap();
    let predecessor = state.clone();
    let next = state
        .review_rejected_schedule(&predecessor, &replacement(&old), 600)
        .unwrap();
    assert_ne!(next.id, old.id);
    assert_eq!(next.client_msg_id, old.client_msg_id);
    assert_eq!(next.base, old.base);
    assert_eq!(next.local_revision, old.local_revision);
    assert_eq!(state.local_revision(), predecessor.local_revision());
    assert_eq!(state.desired(), Some(&desired));
    assert!(!state.accepts_direct_local(&old));
    assert!(state.accepts_direct_local(&next));
    let operation = state.begin_operation(&old.scope).unwrap();
    assert!(operation.known_schedule_check());
    assert_eq!(operation.kind, OperationKind::Schedule { post_at: 900 });
    assert_eq!(operation.snapshot.as_ref(), Some(&next.snapshot));
}

#[test]
fn expired_retry_is_nonmutating_and_explicit_change_remains_available() {
    let (mut state, old, _) = rejected();
    let predecessor = state.clone();
    assert_eq!(
        state.review_rejected_schedule(&predecessor, &RejectedScheduleChoice::Retry, 500),
        Err(RejectedScheduleError::Time)
    );
    assert_eq!(state, predecessor);
    assert!(
        state
            .review_rejected_schedule(&predecessor, &replacement(&old), 500)
            .is_ok()
    );
}

#[test]
fn uncertain_inflight_changed_owner_or_stale_review_cannot_authorize_replacement() {
    let (original, old, operation) = rejected();
    for workflow in [
        Workflow::Uncertain(operation.clone()),
        Workflow::InFlight(operation.clone()),
    ] {
        let mut state = original.clone();
        state.workflow = workflow;
        let expected = state.clone();
        assert_eq!(
            state.review_rejected_schedule(&expected, &replacement(&old), 200),
            Err(RejectedScheduleError::Ineligible)
        );
        assert_eq!(state, expected);
    }
    let mut state = original.clone();
    state.acknowledged.as_mut().unwrap().last_updated_ts = "101.000001".into();
    let expected = state.clone();
    assert_eq!(
        state.review_rejected_schedule(&expected, &replacement(&old), 200),
        Err(RejectedScheduleError::Ineligible)
    );
    let mut state = original.clone();
    state.checkpoint_direct_local(&old, None).unwrap();
    let expected = state.clone();
    assert_eq!(
        state.review_rejected_schedule(&original, &replacement(&old), 200),
        Err(RejectedScheduleError::Stale)
    );
    assert_eq!(state, expected);
}

#[test]
fn replacement_rejects_files_scope_metadata_and_bad_times_without_mutation() {
    let (original, old, _) = rejected();
    let mut variants = vec![];
    let mut snapshot = old.snapshot.clone();
    snapshot.file_ids.push("F1".into());
    variants.push(snapshot);
    let mut snapshot = old.snapshot.clone();
    snapshot.destinations[0]["channel_id"] = json!("C2");
    variants.push(snapshot);
    let mut snapshot = old.snapshot.clone();
    snapshot.destinations[0]["unknown"] = json!(true);
    variants.push(snapshot);
    let mut snapshot = old.snapshot.clone();
    snapshot.blocks.clear();
    variants.push(snapshot);
    for snapshot in variants {
        let mut state = original.clone();
        assert_eq!(
            state.review_rejected_schedule(
                &original,
                &RejectedScheduleChoice::Replace {
                    snapshot,
                    post_at: 900
                },
                200
            ),
            Err(RejectedScheduleError::Snapshot)
        );
        assert_eq!(state, original);
    }
    let mut state = original.clone();
    let choice = RejectedScheduleChoice::Replace {
        snapshot: old.snapshot,
        post_at: 200 + crate::scheduled::MAX_SCHEDULE_SECONDS + 1,
    };
    assert_eq!(
        state.review_rejected_schedule(&original, &choice, 200),
        Err(RejectedScheduleError::Time)
    );
    assert_eq!(state, original);
}

#[test]
fn stale_rejected_receipt_cannot_complete_new_explicit_attempt() {
    let (mut state, old, old_operation) = rejected();
    let predecessor = state.clone();
    let next = state
        .review_rejected_schedule(&predecessor, &replacement(&old), 600)
        .unwrap();
    let mut restored: DraftSync =
        serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
    assert_eq!(restored.consumption(), Some(&next));
    let operation = restored.begin_operation(&old.scope).unwrap();
    let mut receipt = operation.base.clone().unwrap();
    receipt.snapshot = next.snapshot.clone();
    receipt.last_updated_ts = "200.000001".into();
    assert!(!restored.confirm_scheduled(&old_operation.id, receipt.clone(), 900));
    assert!(restored.confirm_scheduled(&operation.id, receipt, 900));
    assert!(restored.accepts_direct_local(&next));
    assert!(!restored.accepts_direct_local(&old));
}
