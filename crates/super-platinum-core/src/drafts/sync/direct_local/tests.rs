use super::*;

fn scope() -> Scope {
    Scope {
        account_id: "00000000-0000-4000-8000-000000000101".into(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        },
    }
}
fn snapshot(text: &str) -> Snapshot {
    Snapshot {
        blocks: vec![
            serde_json::json!({"type":"section","text":{"type":"plain_text","text":text}}),
        ],
        file_ids: vec![],
        destinations: vec![serde_json::json!({"channel_id":"C1"})],
    }
}
fn initial(acknowledged: bool) -> DraftSync {
    let mut state = DraftSync::new(scope());
    state.edit(&scope(), Some(snapshot("Original")));
    if acknowledged {
        let op = state.begin_operation(&scope()).unwrap();
        let remote = RemoteDraft {
            scope: scope(),
            id: "Dr1".into(),
            client_msg_id: op.client_msg_id,
            last_updated_ts: "100.000001".into(),
            snapshot: op.snapshot.unwrap(),
        };
        assert!(state.confirm(&op.id, remote));
    }
    state
}
fn consume(state: &mut DraftSync, kind: ConsumeKind) -> ConsumeIntent {
    state
        .request_consume(&scope(), state.local_revision(), kind)
        .unwrap()
}
fn complete(state: &mut DraftSync, op: &Operation) {
    match op.kind {
        OperationKind::Send => assert!(state.confirm_sent(
            &scope(),
            &op.id,
            &op.client_msg_id,
            op.base.as_ref().map(|base| base.id.as_str()),
            "200.000001"
        )),
        OperationKind::Schedule { post_at } => assert!(
            state.confirm_scheduled(
                &op.id,
                RemoteDraft {
                    scope: scope(),
                    id: op
                        .base
                        .as_ref()
                        .map_or("DrScheduled", |base| base.id.as_str())
                        .into(),
                    client_msg_id: op.client_msg_id.clone(),
                    last_updated_ts: "200.000001".into(),
                    snapshot: op.snapshot.clone().unwrap(),
                },
                post_at
            )
        ),
        _ => panic!("terminal operation required"),
    }
}
fn kinds() -> [ConsumeKind; 2] {
    [ConsumeKind::Send, ConsumeKind::Schedule { post_at: 500 }]
}

#[test]
fn local_and_acknowledged_terminal_consumptions_preserve_exact_frozen_state() {
    for acknowledged in [false, true] {
        for kind in kinds() {
            let mut state = initial(acknowledged);
            let intent = consume(&mut state, kind);
            assert!(state.accepts_direct_local(&intent));
            let op = state.begin_operation(&scope()).unwrap();
            for phase in 0..3 {
                state.workflow = match phase {
                    0 => Workflow::InFlight(op.clone()),
                    1 => Workflow::Uncertain(op.clone()),
                    _ => Workflow::Rejected(op.clone()),
                };
                let before = state.local_revision();
                state.checkpoint_direct_local(&intent, None).unwrap();
                assert_eq!(state.local_revision(), before + 1);
                assert_eq!(state.pending(), Some(&op));
                assert_eq!(state.consumption(), Some(&intent));
                assert_eq!(state.acknowledged(), intent.base.as_ref());
                assert_eq!(
                    state.cancel_consumption(&scope(), &intent.id),
                    Err(if phase == 2 {
                        ConsumeError::NewerEdits
                    } else {
                        ConsumeError::Busy
                    })
                );
            }
        }
    }
}

#[test]
fn terminal_operation_must_match_every_frozen_field() {
    for mutation in 0..7 {
        let mut state = initial(true);
        let intent = consume(&mut state, ConsumeKind::Send);
        let mut op = state.begin_operation(&scope()).unwrap();
        match mutation {
            0 => op.scope.key.channel = "Other".into(),
            1 => op.kind = OperationKind::Schedule { post_at: 500 },
            2 => op.client_msg_id = uuid::Uuid::new_v4().to_string(),
            3 => op.consume_id = Some(uuid::Uuid::new_v4().to_string()),
            4 => op.local_revision += 1,
            5 => op.snapshot = Some(snapshot("Different")),
            _ => op.base.as_mut().unwrap().last_updated_ts = "101.000001".into(),
        }
        state.workflow = Workflow::Uncertain(op);
        let before = state.clone();
        assert_eq!(
            state.checkpoint_direct_local(&intent, None),
            Err(ConsumeError::Busy)
        );
        assert_eq!(state, before);
    }
}

#[test]
fn changed_acknowledged_base_or_conflict_revokes_local_capability() {
    for conflict in [false, true] {
        let mut state = initial(true);
        let intent = consume(&mut state, ConsumeKind::Send);
        let op = state.begin_operation(&scope()).unwrap();
        if conflict {
            state.workflow = Workflow::Conflict {
                remote: None,
                pending: Some(op),
            };
        } else {
            state.acknowledged.as_mut().unwrap().last_updated_ts = "101.000001".into();
        }
        assert!(!state.accepts_direct_local(&intent));
    }
}

#[test]
fn prerequisite_create_or_update_is_not_terminal_local_authority() {
    for acknowledged in [false, true] {
        let mut state = initial(acknowledged);
        state.edit(&scope(), Some(snapshot("Changed")));
        state.begin_operation(&scope()).unwrap();
        let intent = consume(&mut state, ConsumeKind::Send);
        assert!(!state.accepts_direct_local(&intent));
    }
}

#[test]
fn positive_completion_and_restart_retain_exact_successor_authority() {
    for acknowledged in [false, true] {
        for kind in kinds() {
            let mut state = initial(acknowledged);
            let intent = consume(&mut state, kind);
            let op = state.begin_operation(&scope()).unwrap();
            state.checkpoint_direct_local(&intent, None).unwrap();
            complete(&mut state, &op);
            assert_ne!(state.client_msg_id(), intent.client_msg_id);
            assert_eq!(state.local_preservation_intent(), Some(&intent));
            let mut restored: DraftSync =
                serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
            restored = restored.restore_after_restart();
            assert!(restored.consumption_valid());
            restored
                .checkpoint_direct_local(&intent, Some(snapshot("Newer")))
                .unwrap();
            assert!(restored.consumption().is_none());
            assert_eq!(restored.client_msg_id(), state.client_msg_id());
        }
    }
}

#[test]
fn missing_legacy_completion_marker_loads_but_never_infers_authority() {
    let mut state = initial(false);
    let intent = consume(&mut state, ConsumeKind::Send);
    let op = state.begin_operation(&scope()).unwrap();
    complete(&mut state, &op);
    let mut raw = serde_json::to_value(&state).unwrap();
    raw.as_object_mut().unwrap().remove("local_completion");
    let restored: DraftSync = serde_json::from_value(raw).unwrap();
    assert!(restored.consumption_valid());
    assert_eq!(
        restored.acknowledged_local_revision(),
        intent.local_revision + 1
    );
    assert!(!restored.accepts_direct_local(&intent));
    assert!(restored.local_preservation_intent().is_none());
}

#[test]
fn completion_checks_exact_intent_and_successor_not_only_revision() {
    let mut state = initial(true);
    let intent = consume(&mut state, ConsumeKind::Send);
    let op = state.begin_operation(&scope()).unwrap();
    complete(&mut state, &op);
    let mut foreign = intent.clone();
    foreign.id = uuid::Uuid::new_v4().to_string();
    assert!(!state.accepts_direct_local(&foreign));
    state.client_msg_id = uuid::Uuid::new_v4().to_string();
    assert!(!state.accepts_direct_local(&intent));
}

#[test]
fn later_consume_then_cancel_and_later_ordinary_operation_retire_old_authority() {
    for consume_again in [false, true] {
        let mut state = initial(false);
        let old = consume(&mut state, ConsumeKind::Send);
        let op = state.begin_operation(&scope()).unwrap();
        complete(&mut state, &op);
        state
            .checkpoint_direct_local(&old, Some(snapshot("Next")))
            .unwrap();
        if consume_again {
            let next = consume(&mut state, ConsumeKind::Schedule { post_at: 600 });
            state.cancel_consumption(&scope(), &next.id).unwrap();
        } else {
            state.begin_operation(&scope()).unwrap();
            let id = state.pending().unwrap().id.clone();
            assert!(state.reject(&scope(), &id)); // ordinary rejection returns Idle
        }
        assert!(state.local_completion.is_none());
        assert!(!state.accepts_direct_local(&old));
    }
}

#[test]
fn restart_uncertain_terminal_keeps_capability_without_minting_completion() {
    for kind in kinds() {
        let mut state = initial(true);
        let intent = consume(&mut state, kind);
        let op = state.begin_operation(&scope()).unwrap();
        let mut restored: DraftSync =
            serde_json::from_value(serde_json::to_value(state).unwrap()).unwrap();
        restored = restored.restore_after_restart();
        assert!(restored.local_completion.is_none());
        restored.checkpoint_direct_local(&intent, None).unwrap();
        assert_eq!(restored.pending(), Some(&op));
        assert!(matches!(restored.workflow(), Workflow::Uncertain(_)));
        assert!(restored.begin_operation(&scope()).is_none());
    }
}

#[test]
fn revision_exhaustion_and_invalid_completion_are_non_authorizing() {
    let mut state = initial(false);
    let intent = consume(&mut state, ConsumeKind::Send);
    state.local_revision = u64::MAX;
    assert_eq!(
        state.checkpoint_direct_local(&intent, None),
        Err(ConsumeError::RevisionExhausted)
    );
    let op = state.begin_operation(&scope()).unwrap();
    complete(&mut state, &op);
    state
        .local_completion
        .as_mut()
        .unwrap()
        .successor_client_msg_id = intent.client_msg_id.clone();
    assert!(!state.local_completion_valid());
    assert!(!state.accepts_direct_local(&intent));
}

#[test]
fn loadable_legacy_base_cannot_make_positive_completion_unpersistable() {
    let mut state = initial(true);
    state
        .acknowledged
        .as_mut()
        .unwrap()
        .snapshot
        .destinations
        .clear();
    let intent = consume(&mut state, ConsumeKind::Send);
    let op = state.begin_operation(&scope()).unwrap();
    assert!(state.consumption_valid());
    assert!(DraftSync::adopt_remote(&scope(), op.base.clone().unwrap()).is_err());
    complete(&mut state, &op);
    assert!(state.consumption_valid());
    assert!(state.consumption().is_none());
    assert!(state.acknowledged().is_none());
    assert_ne!(state.client_msg_id(), intent.client_msg_id);
    assert!(state.local_completion.is_none());
    assert!(!state.accepts_direct_local(&intent));
    let restored: DraftSync = serde_json::from_value(serde_json::to_value(state).unwrap()).unwrap();
    assert!(restored.consumption_valid());
}
