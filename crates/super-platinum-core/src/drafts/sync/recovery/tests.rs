use super::*;

fn uncertain(kind: OperationKind) -> (DraftSync, Operation) {
    let scope = Scope {
        account_id: uuid::Uuid::new_v4().to_string(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        },
    };
    let mut state = DraftSync::adopt_remote(
        &scope,
        RemoteDraft {
            scope: scope.clone(),
            id: "Dr1".into(),
            client_msg_id: uuid::Uuid::new_v4().to_string(),
            last_updated_ts: "100.000001".into(),
            snapshot: crate::drafts::codec::encode(&scope.key, "Original").unwrap(),
        },
    )
    .unwrap();
    if kind == OperationKind::Send {
        state
            .request_consume(&scope, state.local_revision(), ConsumeKind::Send)
            .unwrap();
    } else {
        state.edit(
            &scope,
            Some(crate::drafts::codec::encode(&scope.key, "Edited").unwrap()),
        );
    }
    let operation = state.begin_operation(&scope).unwrap();
    assert_eq!(operation.kind, kind);
    state.mark_uncertain(&scope, &operation.id);
    (state, operation)
}

#[test]
fn update_and_send_pending_checks_never_expire_or_authorize_replay() {
    for kind in [OperationKind::Update, OperationKind::Send] {
        let (mut state, op) = uncertain(kind);
        let ticket = state.reserve_check(&op, 1_000).unwrap();
        assert_eq!(ticket.operation(), &op);
        assert!(uuid::Uuid::parse_str(ticket.id()).is_ok());
        assert_eq!(state.check_retry_at_ms(&op), None);
        assert_eq!(state.reserve_check(&op, u64::MAX), Err(CheckBlock::Pending));
        let mut restored: DraftSync =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        restored = restored.restore_after_restart();
        assert_eq!(restored.pending_check(&op), Some(&ticket));
        assert_eq!(
            restored.reserve_check(&op, u64::MAX),
            Err(CheckBlock::Pending)
        );
        assert!(restored.recovery_state_valid());
    }
}

#[test]
fn completion_retains_long_retry_after_and_a_new_check_has_a_new_ticket() {
    let (mut state, op) = uncertain(OperationKind::Update);
    let first = state.reserve_check(&op, 1_000).unwrap();
    assert!(state.defer_check(&op, 3_600_000));
    assert_eq!(state.pending_check(&op), Some(&first));
    assert!(state.finish_check(&first, 50_000));
    assert_eq!(state.check_retry_at_ms(&op), Some(3_600_000));
    assert_eq!(
        state.reserve_check(&op, 3_599_999),
        Err(CheckBlock::Cooldown(3_600_000))
    );
    let second = state.reserve_check(&op, 3_600_000).unwrap();
    assert_ne!(first.id(), second.id());
    let before = state.clone();
    assert!(!state.finish_check(&first, u64::MAX));
    assert_eq!(state, before);
    assert!(state.finish_check(&second, 0));
    assert_eq!(state.check_retry_at_ms(&op), Some(3_630_000));
}

#[test]
fn immutable_payload_scope_base_and_ticket_identity_are_all_required() {
    let (mut state, op) = uncertain(OperationKind::Update);
    let ticket = state.reserve_check(&op, 1_000).unwrap();
    for field in 0..5 {
        let mut wrong = ticket.clone();
        match field {
            0 => wrong.id = uuid::Uuid::new_v4().to_string(),
            1 => wrong
                .operation
                .snapshot
                .as_mut()
                .unwrap()
                .file_ids
                .push("Fwrong".into()),
            2 => wrong.operation.scope.key.channel = "Cother".into(),
            3 => wrong.operation.base.as_mut().unwrap().last_updated_ts = "1.000001".into(),
            _ => wrong.operation.local_revision += 1,
        }
        let before = state.clone();
        assert!(!state.finish_check(&wrong, u64::MAX));
        assert_eq!(state, before);
    }
}

#[test]
fn changed_operation_with_reused_id_cannot_adopt_or_complete_a_ticket() {
    let (mut state, op) = uncertain(OperationKind::Update);
    let ticket = state.reserve_check(&op, 0).unwrap();
    let Workflow::Uncertain(current) = &mut state.workflow else {
        panic!()
    };
    current
        .snapshot
        .as_mut()
        .unwrap()
        .file_ids
        .push("Fnew".into());
    assert!(!state.recovery_state_valid());
    assert!(!state.finish_check(&ticket, 100));
    assert_eq!(state.reserve_check(&op, 100), Err(CheckBlock::Ineligible));
    let changed = state.pending().unwrap().clone();
    assert_eq!(
        state.reserve_check(&changed, 100),
        Err(CheckBlock::Ineligible)
    );
}

#[test]
fn completion_and_positive_update_keep_newer_local_input() {
    let (mut state, op) = uncertain(OperationKind::Update);
    let ticket = state.reserve_check(&op, 0).unwrap();
    let newer = crate::drafts::codec::encode(&op.scope.key, "Newer input").unwrap();
    state.edit(&op.scope, Some(newer.clone()));
    let mut returned = op.base.clone().unwrap();
    returned.snapshot = op.snapshot.clone().unwrap();
    returned.last_updated_ts = "101.000001".into();
    assert!(state.finish_check(&ticket, 100));
    assert!(state.confirm(&op.id, returned));
    assert_eq!(state.desired(), Some(&newer));
    assert!(state.pending_check(&op).is_none());
    assert!(state.recovery_state_valid());
}

#[test]
fn conflict_does_not_release_a_pending_check_or_relabel_it_as_failed() {
    let (mut state, op) = uncertain(OperationKind::Update);
    let ticket = state.reserve_check(&op, 0).unwrap();
    let mut remote = op.base.clone().unwrap();
    remote.last_updated_ts = "102.000001".into();
    remote.snapshot = crate::drafts::codec::encode(&op.scope.key, "Other writer").unwrap();
    assert!(state.observe_conflict(remote));
    assert!(!state.finish_check(&ticket, 100));
    assert_eq!(state.pending_check(&op), Some(&ticket));
    assert_eq!(
        state.reserve_check(&op, u64::MAX),
        Err(CheckBlock::Ineligible)
    );
    assert!(state.recovery_state_valid());
}

#[test]
fn other_operation_kinds_and_inflight_requests_are_not_checkable() {
    let (mut state, op) = uncertain(OperationKind::Update);
    state.workflow = Workflow::InFlight(op.clone());
    assert_eq!(state.reserve_check(&op, 0), Err(CheckBlock::Ineligible));
    for kind in [
        OperationKind::Create,
        OperationKind::Delete,
        OperationKind::Schedule { post_at: 1000 },
    ] {
        let mut other = op.clone();
        other.kind = kind;
        state.workflow = Workflow::Uncertain(other.clone());
        assert_eq!(state.reserve_check(&other, 0), Err(CheckBlock::Ineligible));
    }
}
