use super::*;
use serde_json::json;

fn initial(thread: bool) -> DraftSync {
    let scope = Scope {
        account_id: "00000000-0000-4000-8000-000000000001".into(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: thread.then(|| "100.000001".into()),
        },
    };
    let mut destination = json!({"channel_id":"C1"});
    if thread {
        destination["thread_ts"] = json!("100.000001");
        destination["broadcast"] = json!(false);
    }
    let remote = RemoteDraft {
        scope: scope.clone(),
        id: "Dr1".into(),
        client_msg_id: "00000000-0000-4000-8000-000000000002".into(),
        last_updated_ts: "100.000001".into(),
        snapshot: Snapshot {
            blocks: vec![
                json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Original"}]}]}),
            ],
            file_ids: vec![],
            destinations: vec![destination],
        },
    };
    DraftSync::adopt_remote(&scope, remote).unwrap()
}
fn rich(state: &DraftSync) -> Snapshot {
    Snapshot {
        blocks: vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_quote","elements":[{"type":"text","text":"Reviewed quote","style":{"bold":true,"italic":true}}]}]}),
        ],
        file_ids: vec![],
        destinations: state.desired().unwrap().destinations.clone(),
    }
}

#[test]
fn exact_channel_and_thread_rich_blocks_become_terminal_operation() {
    for thread in [false, true] {
        let mut state = initial(thread);
        let predecessor = state.clone();
        let reviewed = rich(&state);
        assert!(crate::drafts::codec::decode(&state.scope().key, &reviewed).is_err());
        let intent = state
            .request_reviewed_schedule(&predecessor, reviewed.clone(), 500)
            .unwrap();
        assert_eq!(intent.snapshot, reviewed);
        assert_eq!(intent.local_revision, predecessor.local_revision());
        assert_eq!(intent.base.as_ref(), predecessor.acknowledged());
        assert_eq!(intent.client_msg_id, predecessor.client_msg_id());
        assert_eq!(state.local_revision(), predecessor.local_revision() + 1);
        assert!(state.desired().is_none());
        let op = state.begin_operation(predecessor.scope()).unwrap();
        assert_eq!(op.kind, OperationKind::Schedule { post_at: 500 });
        assert_eq!(op.snapshot.as_ref(), Some(&reviewed));
        assert_eq!(op.base.as_ref(), predecessor.acknowledged());
        assert!(state.begin_operation(predecessor.scope()).is_none());
    }
}

#[test]
fn stale_local_and_same_revision_different_identity_are_denied() {
    let predecessor = initial(false);
    let reviewed = rich(&predecessor);
    let mut changed = predecessor.clone();
    changed.edit(predecessor.scope(), Some(reviewed.clone()));
    let before = changed.clone();
    assert_eq!(
        changed.request_reviewed_schedule(&predecessor, reviewed.clone(), 500),
        Err(ReviewedScheduleError::Stale)
    );
    assert_eq!(changed, before);
    let mut changed = predecessor.clone();
    changed.client_msg_id = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        changed.request_reviewed_schedule(&predecessor, reviewed, 500),
        Err(ReviewedScheduleError::Stale)
    );
}

#[test]
fn invalid_payload_and_time_are_transactional() {
    let predecessor = initial(true);
    let valid = rich(&predecessor);
    let mut variants = vec![];
    let mut wrong = valid.clone();
    wrong.destinations[0]["channel_id"] = json!("C2");
    variants.push(wrong);
    let mut wrong = valid.clone();
    wrong.destinations[0]
        .as_object_mut()
        .unwrap()
        .remove("broadcast");
    variants.push(wrong);
    let mut wrong = valid.clone();
    wrong.file_ids.push("F1".into());
    variants.push(wrong);
    let mut wrong = valid.clone();
    wrong.blocks.clear();
    variants.push(wrong);
    let mut wrong = valid.clone();
    wrong.blocks = vec![json!("not a block")];
    variants.push(wrong);
    for reviewed in variants {
        let mut state = predecessor.clone();
        assert_eq!(
            state.request_reviewed_schedule(&predecessor, reviewed, 500),
            Err(ReviewedScheduleError::Snapshot)
        );
        assert_eq!(state, predecessor);
    }
    let mut state = predecessor.clone();
    assert_eq!(
        state.request_reviewed_schedule(&predecessor, valid, 0),
        Err(ReviewedScheduleError::Consume(ConsumeError::Schedule))
    );
    assert_eq!(state, predecessor);
}

#[test]
fn pending_update_and_never_acknowledged_drafts_are_not_admitted() {
    let mut pending = initial(false);
    let payload = rich(&pending);
    let scope = pending.scope().clone();
    pending.edit(&scope, Some(payload.clone()));
    pending.begin_operation(&scope).unwrap();
    let predecessor = pending.clone();
    assert_eq!(
        pending.request_reviewed_schedule(&predecessor, payload.clone(), 500),
        Err(ReviewedScheduleError::Ineligible)
    );
    assert_eq!(pending, predecessor);
    let mut fresh = DraftSync::new(scope.clone());
    fresh.edit(&scope, Some(payload.clone()));
    let predecessor = fresh.clone();
    assert_eq!(
        fresh.request_reviewed_schedule(&predecessor, payload, 500),
        Err(ReviewedScheduleError::Ineligible)
    );
    assert_eq!(fresh, predecessor);
}

#[test]
fn unsupported_newer_local_survives_positive_receipt_and_blocks_old_dialog() {
    let mut state = initial(false);
    let predecessor = state.clone();
    let reviewed = rich(&state);
    let intent = state
        .request_reviewed_schedule(&predecessor, reviewed.clone(), 500)
        .unwrap();
    let op = state.begin_operation(predecessor.scope()).unwrap();
    state.checkpoint_direct_local(&intent, None).unwrap();
    // None is unsupported paired LocalDraft here, not a request to delete it.
    let revision = state.local_revision();
    let mut receipt = op.base.clone().unwrap();
    receipt.snapshot = reviewed.clone();
    receipt.last_updated_ts = "101.000001".into();
    assert!(state.confirm_scheduled(&op.id, receipt, 500));
    assert_eq!(state.local_revision(), revision);
    assert!(state.accepts_direct_local(&intent));
    assert_eq!(
        state.request_reviewed_schedule(&predecessor, reviewed, 500),
        Err(ReviewedScheduleError::Stale)
    );
}

#[test]
fn restart_retains_exact_rich_operation_without_replay() {
    let mut state = initial(false);
    let predecessor = state.clone();
    let reviewed = rich(&state);
    state
        .request_reviewed_schedule(&predecessor, reviewed, 500)
        .unwrap();
    let operation = state.begin_operation(predecessor.scope()).unwrap();
    let mut restored: DraftSync =
        serde_json::from_value(serde_json::to_value(state).unwrap()).unwrap();
    restored = restored.restore_after_restart();
    assert_eq!(restored.pending(), Some(&operation));
    assert!(matches!(restored.workflow(), Workflow::Uncertain(_)));
    assert!(restored.begin_operation(predecessor.scope()).is_none());
}

#[test]
fn revision_exhaustion_preserves_every_predecessor_field() {
    let mut state = initial(false);
    state.local_revision = u64::MAX;
    let predecessor = state.clone();
    let reviewed = rich(&state);
    assert_eq!(
        state.request_reviewed_schedule(&predecessor, reviewed, 500),
        Err(ReviewedScheduleError::Consume(
            ConsumeError::RevisionExhausted
        ))
    );
    assert_eq!(state, predecessor);
}

#[test]
fn observation_and_check_ticket_changes_invalidate_exact_predecessor() {
    let mut state = initial(false);
    let predecessor = state.clone();
    let reviewed = rich(&state);
    let base = state.acknowledged().unwrap().clone();
    let event = DraftObservation {
        scope: state.scope().clone(),
        draft_id: base.id.clone(),
        client_msg_id: base.client_msg_id.clone(),
        event_ts: "101.000001".into(),
        kind: ObservationKind::Active(base),
    };
    state.observe_event(&event).unwrap();
    assert_eq!(state.local_revision(), predecessor.local_revision());
    assert_eq!(state.desired(), predecessor.desired());
    let before = state.clone();
    assert_eq!(
        state.request_reviewed_schedule(&predecessor, reviewed.clone(), 500),
        Err(ReviewedScheduleError::Stale)
    );
    assert_eq!(state, before);

    let mut state = initial(false);
    let scope = state.scope().clone();
    state.edit(&scope, Some(reviewed.clone()));
    let operation = state.begin_operation(&scope).unwrap();
    state.mark_uncertain(&scope, &operation.id);
    let predecessor = state.clone();
    state.reserve_check(&operation, 1_000).unwrap();
    assert_eq!(state.local_revision(), predecessor.local_revision());
    let before = state.clone();
    assert_eq!(
        state.request_reviewed_schedule(&predecessor, reviewed, 500),
        Err(ReviewedScheduleError::Stale)
    );
    assert_eq!(state, before);
}

#[test]
fn exact_predecessor_still_rejects_blocked_conflicted_consuming_and_rejected() {
    let mut blocked = initial(false);
    let base = blocked.acknowledged().unwrap().clone();
    blocked
        .observe_event(&DraftObservation {
            scope: blocked.scope().clone(),
            draft_id: base.id.clone(),
            client_msg_id: base.client_msg_id.clone(),
            event_ts: "101.000001".into(),
            kind: ObservationKind::Unsupported(
                json!({"id":base.id,"client_msg_id":base.client_msg_id,"unknown_lifecycle":true}),
            ),
        })
        .unwrap();
    let mut conflicted = initial(false);
    conflicted.workflow = Workflow::Conflict {
        remote: conflicted.acknowledged.clone(),
        pending: None,
    };
    let mut consuming = initial(false);
    let scope = consuming.scope().clone();
    consuming
        .request_consume(
            &scope,
            consuming.local_revision(),
            ConsumeKind::Schedule { post_at: 500 },
        )
        .unwrap();
    let mut rejected = consuming.clone();
    let op = rejected.begin_operation(&scope).unwrap();
    assert!(rejected.reject(&scope, &op.id));
    let payload = rich(&initial(false));
    for mut state in [blocked, conflicted, consuming, rejected] {
        let predecessor = state.clone();
        assert_eq!(
            state.request_reviewed_schedule(&predecessor, payload.clone(), 500),
            Err(ReviewedScheduleError::Ineligible)
        );
        assert_eq!(state, predecessor);
    }
}

#[test]
fn raw_shape_policy_preserves_opaque_nested_data_and_all_broadcast_forms() {
    // This helper is NOT a Block Kit semantic validator. The activation gate
    // must bind raw blocks to the authentic current rich-editor receipt.
    for broadcast in [None, Some(false), Some(true)] {
        let mut state = initial(true);
        let desired = state.desired.as_mut().unwrap();
        let acknowledged = &mut state.acknowledged.as_mut().unwrap().snapshot;
        for snapshot in [desired, acknowledged] {
            let destination = snapshot.destinations[0].as_object_mut().unwrap();
            match broadcast {
                None => {
                    destination.remove("broadcast");
                }
                Some(value) => {
                    destination.insert("broadcast".into(), json!(value));
                }
            }
        }
        let predecessor = state.clone();
        let mut reviewed = rich(&state);
        reviewed.blocks =
            vec![json!({"type":"future_block","opaque":{"elements":"uninterpreted"}})];
        let intent = state
            .request_reviewed_schedule(&predecessor, reviewed.clone(), 500)
            .unwrap();
        assert_eq!(intent.snapshot, reviewed);
        assert_eq!(
            intent.snapshot.destinations,
            predecessor.desired().unwrap().destinations
        );
    }
}
