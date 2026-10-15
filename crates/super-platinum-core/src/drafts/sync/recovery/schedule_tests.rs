use super::*;
use serde_json::json;

fn uncertain(known: bool) -> (DraftSync, Operation) {
    let scope = Scope {
        account_id: uuid::Uuid::new_v4().to_string(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        },
    };
    let snapshot = Snapshot {
        blocks: vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Frozen rich schedule","style":{"bold":true}}]}]}),
        ],
        file_ids: vec![],
        destinations: vec![json!({"channel_id":"C1"})],
    };
    let mut state = if known {
        DraftSync::adopt_remote(
            &scope,
            RemoteDraft {
                scope: scope.clone(),
                id: "DrKnown".into(),
                client_msg_id: uuid::Uuid::new_v4().to_string(),
                last_updated_ts: "100.000001".into(),
                snapshot: snapshot.clone(),
            },
        )
        .unwrap()
    } else {
        let mut state = DraftSync::new(scope.clone());
        state.edit(&scope, Some(snapshot));
        state
    };
    state
        .request_consume(
            &scope,
            state.local_revision(),
            ConsumeKind::Schedule { post_at: 500 },
        )
        .unwrap();
    let op = state.begin_operation(&scope).unwrap();
    assert!(state.mark_uncertain(&scope, &op.id));
    (state, op)
}

#[test]
fn known_schedule_ticket_survives_restart_and_never_expires_into_replay() {
    let (mut state, op) = uncertain(true);
    let ticket = state.reserve_check(&op, 900_000).unwrap(); // already after post_at
    assert!(op.known_schedule_check());
    assert_eq!(state.reserve_check(&op, u64::MAX), Err(CheckBlock::Pending));
    let mut restored: DraftSync =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    restored = restored.restore_after_restart();
    assert!(restored.recovery_state_valid());
    assert_eq!(restored.pending_check(&op), Some(&ticket));
    assert_eq!(
        restored.reserve_check(&op, u64::MAX),
        Err(CheckBlock::Pending)
    );
    assert!(restored.begin_operation(&op.scope).is_none());
}

#[test]
fn unknown_id_and_corrupted_schedule_links_are_not_checkable() {
    let (mut unknown, op) = uncertain(false);
    assert!(!op.known_schedule_check());
    assert_eq!(unknown.reserve_check(&op, 0), Err(CheckBlock::Ineligible));
    let (state, op) = uncertain(true);
    for field in 0..14 {
        let mut s = state.clone();
        let mut wrong = op.clone();
        match field {
            0 => wrong.base = None,
            1 => wrong.consume_id = None,
            2 => wrong.kind = OperationKind::Schedule { post_at: 0 },
            3 => wrong.base.as_mut().unwrap().client_msg_id = uuid::Uuid::new_v4().to_string(),
            4 => wrong.snapshot.as_mut().unwrap().file_ids.push("F1".into()),
            5 => wrong.snapshot.as_mut().unwrap().destinations[0]["channel_id"] = json!("C2"),
            6 => wrong.consume_id = Some(uuid::Uuid::new_v4().to_string()),
            7 => wrong.local_revision += 1,
            8 => wrong.base.as_mut().unwrap().id = "DrForeign".into(),
            9 => {
                wrong.scope.account_id = uuid::Uuid::new_v4().to_string();
                wrong.base.as_mut().unwrap().scope = wrong.scope.clone();
            }
            10 => s.acknowledged = None,
            11 => s.acknowledged.as_mut().unwrap().id = "DrOtherAcknowledged".into(),
            12 => s.acknowledged.as_mut().unwrap().client_msg_id = uuid::Uuid::new_v4().to_string(),
            _ => {
                s.acknowledged.as_mut().unwrap().scope.account_id = uuid::Uuid::new_v4().to_string()
            }
        }
        s.workflow = Workflow::Uncertain(wrong.clone());
        assert_eq!(
            s.reserve_check(&wrong, u64::MAX),
            Err(CheckBlock::Ineligible),
            "{field}"
        );
    }
}

#[test]
fn schedule_check_completion_requires_exact_ticket_and_retains_cooldown() {
    let (mut state, op) = uncertain(true);
    let ticket = state.reserve_check(&op, 100).unwrap();
    for field in 0..4 {
        let mut wrong = ticket.clone();
        match field {
            0 => wrong.operation.kind = OperationKind::Schedule { post_at: 501 },
            1 => wrong.operation.snapshot.as_mut().unwrap().blocks[0]["extra"] = json!(true),
            2 => wrong.operation.base.as_mut().unwrap().last_updated_ts = "101.000001".into(),
            _ => wrong.id = uuid::Uuid::new_v4().to_string(),
        }
        let before = state.clone();
        assert!(!state.finish_check(&wrong, 999_999));
        assert_eq!(state, before);
    }
    assert!(state.finish_check(&ticket, 120_100));
    assert_eq!(
        state.reserve_check(&op, 120_099),
        Err(CheckBlock::Cooldown(120_100))
    );
    let next = state.reserve_check(&op, 120_100).unwrap();
    assert_ne!(next.id(), ticket.id());
    assert!(!state.finish_check(&ticket, 0));
}

#[test]
fn positive_schedule_after_check_preserves_newer_local_and_historical_deadline() {
    let (mut state, op) = uncertain(true);
    // A positive observation may advance the known identity while the immutable
    // operation keeps its original base revision. That is still checkable.
    state.acknowledged.as_mut().unwrap().last_updated_ts = "100.000002".into();
    let ticket = state.reserve_check(&op, 100).unwrap();
    let newer = crate::drafts::codec::encode(&op.scope.key, "Newer local input").unwrap();
    state.edit(&op.scope, Some(newer.clone()));
    let mut remote = op.base.clone().unwrap();
    remote.snapshot = op.snapshot.clone().unwrap();
    remote.last_updated_ts = "101.000001".into();
    assert!(state.finish_check(&ticket, 0));
    assert!(state.confirm_scheduled(&op.id, remote, 500));
    assert_eq!(state.desired(), Some(&newer));
    assert!(state.consumption().is_none());
    assert!(state.recovery_state_valid());
    assert_eq!(
        state.reserve_check(&op, u64::MAX),
        Err(CheckBlock::Ineligible)
    );
    let restored: DraftSync =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert!(restored.recovery_state_valid());
}

#[test]
fn legacy_schedule_deadline_binds_only_a_supported_frozen_operation() {
    let (mut state, op) = uncertain(true);
    state.check_deadline = Some(CheckDeadline {
        operation_id: op.id.clone(),
        until_ms: 900,
        operation: None,
    });
    state.migrate_check_deadline();
    assert_eq!(state.check_retry_at_ms(&op), Some(900));
    assert!(state.recovery_state_valid());
    let (mut unknown, op) = uncertain(false);
    unknown.check_deadline = Some(CheckDeadline {
        operation_id: op.id,
        until_ms: 900,
        operation: None,
    });
    unknown.migrate_check_deadline();
    assert!(unknown.check_deadline.is_none());
}
