use super::*;

fn scope() -> Scope {
    Scope {
        account_id: "00000000-0000-4000-8000-000000000001".into(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        },
    }
}
fn snapshot(text: &str) -> Snapshot {
    Snapshot {
        blocks: vec![serde_json::json!({"type":"future_block","opaque":{"text":text,"keep":true}})],
        file_ids: vec!["F1".into()],
        destinations: vec![serde_json::json!({"channel_id":"C1"})],
    }
}
fn local(text: &str) -> DraftSync {
    let mut state = DraftSync::new(scope());
    state.edit(&scope(), Some(snapshot(text)));
    state
}
fn remote(state: &DraftSync, text: &str, revision: &str) -> RemoteDraft {
    RemoteDraft {
        scope: scope(),
        id: "Dr1".into(),
        client_msg_id: state.client_msg_id().into(),
        last_updated_ts: revision.into(),
        snapshot: snapshot(text),
    }
}
fn synced() -> DraftSync {
    let mut state = local("one");
    let create = state.begin_operation(&scope()).unwrap();
    assert!(state.confirm(&create.id, remote(&state, "one", "10.000001")));
    state
}
fn consume(state: &mut DraftSync, kind: ConsumeKind) -> ConsumeIntent {
    state
        .request_consume(&scope(), state.local_revision(), kind)
        .unwrap()
}

#[test]
fn never_published_send_detaches_exact_payload_without_creating_a_draft() {
    let mut state = local("日本語 🦊");
    let identity = state.client_msg_id().to_owned();
    let revision = state.local_revision();
    let intent = consume(&mut state, ConsumeKind::Send);
    assert_eq!(intent.local_revision, revision);
    assert_eq!(intent.snapshot, snapshot("日本語 🦊"));
    assert_eq!(intent.client_msg_id, identity);
    assert!(state.desired().is_none());
    assert!(!state.tombstone());
    let op = state.begin_operation(&scope()).unwrap();
    assert_eq!(op.kind, OperationKind::Send);
    assert_eq!(op.snapshot, Some(intent.snapshot));
    assert_eq!(op.consume_id.as_deref(), Some(intent.id.as_str()));
    assert!(op.base.is_none());
    assert!(state.begin_operation(&scope()).is_none());
    assert!(state.confirm_sent(&scope(), &op.id, &identity, None, "11.000001"));
    assert!(state.consumption().is_none());
    assert_ne!(state.client_msg_id(), identity);
    assert!(state.begin_operation(&scope()).is_none());
}

#[test]
fn send_waits_for_existing_create_but_freezes_newer_content_and_preserves_next_draft() {
    let mut state = local("old autosave");
    let create = state.begin_operation(&scope()).unwrap();
    state.edit(&scope(), Some(snapshot("send this")));
    let intent = consume(&mut state, ConsumeKind::Send);
    state.edit(&scope(), Some(snapshot("next draft")));
    assert!(state.begin_operation(&scope()).is_none());
    assert!(state.confirm(&create.id, remote(&state, "old autosave", "10.000001")));
    let send = state.begin_operation(&scope()).unwrap();
    assert_eq!(send.kind, OperationKind::Send);
    assert_eq!(send.base.as_ref().unwrap().id, "Dr1");
    assert_eq!(send.snapshot.as_ref(), Some(&intent.snapshot));
    assert_ne!(
        send.snapshot.as_ref(),
        Some(&send.base.as_ref().unwrap().snapshot)
    );
    let next_revision = state.local_revision();
    assert!(state.confirm_sent(
        &scope(),
        &send.id,
        &intent.client_msg_id,
        Some("Dr1"),
        "11.000001"
    ));
    assert_eq!(state.desired(), Some(&snapshot("next draft")));
    assert_eq!(state.local_revision(), next_revision);
    let next = state.begin_operation(&scope()).unwrap();
    assert_eq!(next.kind, OperationKind::Create);
    assert_eq!(next.snapshot, Some(snapshot("next draft")));
    assert_ne!(next.client_msg_id, intent.client_msg_id);
    assert!(next.base.is_none());
    assert!(!state.confirm(&create.id, remote(&state, "old autosave", "12.000001")));
}

#[test]
fn schedule_waits_for_update_and_requires_exact_positive_schedule_evidence() {
    let mut state = synced();
    state.edit(&scope(), Some(snapshot("autosave two")));
    let update = state.begin_operation(&scope()).unwrap();
    state.edit(&scope(), Some(snapshot("schedule three")));
    let intent = consume(&mut state, ConsumeKind::Schedule { post_at: 500 });
    assert!(state.begin_operation(&scope()).is_none());
    assert!(state.confirm(&update.id, remote(&state, "autosave two", "11.000001")));
    let schedule = state.begin_operation(&scope()).unwrap();
    assert_eq!(schedule.kind, OperationKind::Schedule { post_at: 500 });
    assert_eq!(schedule.base.as_ref().unwrap().id, "Dr1");
    let receipt = remote(&state, "schedule three", "12.000001");
    assert!(!state.confirm(&schedule.id, receipt.clone()));
    assert!(state.observe(receipt.clone()));
    assert!(state.consumption().is_some()); // Matching content does not prove a schedule.
    assert!(state.begin_operation(&scope()).is_none());
    assert!(!state.confirm_scheduled(&schedule.id, receipt.clone(), 499));
    let mut wrong = receipt.clone();
    wrong.client_msg_id = uuid::Uuid::new_v4().to_string();
    assert!(!state.confirm_scheduled(&schedule.id, wrong, 500));
    state.edit(&scope(), Some(snapshot("new draft")));
    assert!(state.confirm_scheduled(&schedule.id, receipt, 500));
    assert_eq!(state.desired(), Some(&snapshot("new draft")));
    assert_ne!(state.client_msg_id(), intent.client_msg_id);
    assert!(!state.observe(remote(&state, "schedule three", "13.000001")));
}

#[test]
fn direct_schedule_can_create_its_remote_identity_without_an_ordinary_create() {
    let mut state = local("scheduled");
    consume(&mut state, ConsumeKind::Schedule { post_at: 500 });
    let op = state.begin_operation(&scope()).unwrap();
    assert_eq!(op.kind, OperationKind::Schedule { post_at: 500 });
    assert!(op.base.is_none());
    let receipt = remote(&state, "scheduled", "10.000001");
    assert!(state.confirm_scheduled(&op.id, receipt, 500));
    assert!(state.begin_operation(&scope()).is_none());
}

#[test]
fn restart_never_replays_terminal_or_prerequisite_uncertainty() {
    for published in [false, true] {
        let mut state = local("one");
        if published {
            state.begin_operation(&scope()).unwrap();
        }
        let intent = consume(&mut state, ConsumeKind::Send);
        if !published {
            state.begin_operation(&scope()).unwrap();
        }
        let before = state.pending().unwrap().clone();
        let mut restored: DraftSync =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        restored = restored.restore_after_restart();
        assert_eq!(restored.pending(), Some(&before));
        assert_eq!(restored.consumption(), Some(&intent));
        assert!(matches!(restored.workflow(), Workflow::Uncertain(_)));
        assert!(restored.begin_operation(&scope()).is_none());
        assert!(!restored.retry_consumption(&scope(), &intent.id));
        assert_eq!(
            restored.cancel_consumption(&scope(), &intent.id),
            Err(ConsumeError::Busy)
        );
    }
}

#[test]
fn remote_delete_never_confirms_send_but_exact_message_receipt_can_follow_it() {
    let mut state = synced();
    let intent = consume(&mut state, ConsumeKind::Send);
    let send = state.begin_operation(&scope()).unwrap();
    assert!(state.observe_deleted(&scope(), "Dr1"));
    assert!(state.consumption().is_some());
    assert!(matches!(state.workflow(), Workflow::Uncertain(_)));
    assert!(!state.confirm_deleted(&scope(), &send.id, "Dr1"));
    assert!(state.begin_operation(&scope()).is_none());
    assert!(!state.confirm_sent(&scope(), &send.id, "wrong", Some("Dr1"), "11.000001"));
    assert!(!state.confirm_sent(&scope(), &send.id, &intent.client_msg_id, None, "11.000001"));
    for malformed in [
        "bad",
        "1",
        "1.",
        "1.0000001",
        "1.1",
        "0.000001",
        "-1.000001",
    ] {
        assert!(!state.confirm_sent(
            &scope(),
            &send.id,
            &intent.client_msg_id,
            Some("Dr1"),
            malformed
        ));
        assert!(state.consumption().is_some());
    }
    assert!(state.confirm_sent(
        &scope(),
        &send.id,
        &intent.client_msg_id,
        Some("Dr1"),
        "11.000001"
    ));
    assert!(!state.confirm_sent(
        &scope(),
        &send.id,
        &intent.client_msg_id,
        Some("Dr1"),
        "11.000001"
    ));
}

#[test]
fn definite_rejection_parks_prerequisite_and_terminal_attempts_until_explicit_retry() {
    for prerequisite in [false, true] {
        let mut state = local("one");
        if prerequisite {
            state.begin_operation(&scope()).unwrap();
        }
        let intent = consume(&mut state, ConsumeKind::Send);
        if !prerequisite {
            state.begin_operation(&scope()).unwrap();
        }
        let rejected = state.pending().unwrap().clone();
        assert!(state.reject(&scope(), &rejected.id));
        assert!(matches!(state.workflow(), Workflow::Rejected(_)));
        assert!(state.begin_operation(&scope()).is_none());
        assert!(!state.confirm_sent(
            &scope(),
            &rejected.id,
            &intent.client_msg_id,
            None,
            "11.000001"
        ));
        assert!(!state.retry_consumption(&scope(), "stale intent"));
        assert!(state.retry_consumption(&scope(), &intent.id));
        let retry = state.begin_operation(&scope()).unwrap();
        assert_eq!(retry.kind, OperationKind::Send);
        assert_eq!(retry.snapshot, Some(intent.snapshot));
        assert_ne!(retry.id, rejected.id);
        assert_eq!(retry.client_msg_id, intent.client_msg_id);
    }
}

#[test]
fn cancel_restores_only_the_unchanged_detached_composer_and_never_discards_newer_typing() {
    let mut state = synced();
    let intent = consume(&mut state, ConsumeKind::Send);
    state.edit(&scope(), Some(snapshot("new typing")));
    assert_eq!(
        state.cancel_consumption(&scope(), &intent.id),
        Err(ConsumeError::NewerEdits)
    );
    assert_eq!(state.desired(), Some(&snapshot("new typing")));
    assert_eq!(state.consumption(), Some(&intent));
    state.edit(&scope(), None);
    assert_eq!(
        state.cancel_consumption(&scope(), &intent.id),
        Err(ConsumeError::NewerEdits)
    );

    let mut unchanged = synced();
    let intent = consume(&mut unchanged, ConsumeKind::Send);
    let op = unchanged.begin_operation(&scope()).unwrap();
    assert_eq!(
        unchanged.cancel_consumption(&scope(), &intent.id),
        Err(ConsumeError::Busy)
    );
    unchanged.reject(&scope(), &op.id);
    assert_eq!(
        unchanged.cancel_consumption(&scope(), &intent.id).unwrap(),
        snapshot("one")
    );
    assert_eq!(unchanged.desired(), Some(&snapshot("one")));
    assert!(unchanged.consumption().is_none());
    assert!(unchanged.begin_operation(&scope()).is_none());
}

#[test]
fn queued_consumption_observations_never_restore_text_into_detached_composer() {
    let mut state = synced();
    consume(&mut state, ConsumeKind::Send);
    state.edit(&scope(), Some(snapshot("next draft")));
    assert!(state.observe(remote(&state, "one", "11.000001")));
    assert_eq!(state.desired(), Some(&snapshot("next draft")));
    assert!(state.observe(remote(&state, "external change", "12.000001")));
    assert!(matches!(state.workflow(), Workflow::Conflict { .. }));
    assert!(state.begin_operation(&scope()).is_none());
    assert_eq!(state.consumption().unwrap().snapshot, snapshot("one"));
}

#[test]
fn consumption_requires_current_revision_and_scope_without_mutating_on_failure() {
    let mut state = local("one");
    let original = state.clone();
    let mut other = scope();
    other.account_id = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        state.request_consume(&other, state.local_revision(), ConsumeKind::Send),
        Err(ConsumeError::Scope)
    );
    other = scope();
    other.key.team = "T2".into();
    assert_eq!(
        state.request_consume(&other, state.local_revision(), ConsumeKind::Send),
        Err(ConsumeError::Scope)
    );
    assert_eq!(
        state.request_consume(&scope(), 0, ConsumeKind::Send),
        Err(ConsumeError::Stale)
    );
    assert_eq!(
        state.request_consume(
            &scope(),
            state.local_revision(),
            ConsumeKind::Schedule { post_at: 0 }
        ),
        Err(ConsumeError::Schedule)
    );
    assert_eq!(state, original);
    consume(&mut state, ConsumeKind::Send);
    let detached = state.clone();
    assert_eq!(
        state.request_consume(&scope(), state.local_revision(), ConsumeKind::Send),
        Err(ConsumeError::Busy)
    );
    assert_eq!(state, detached);
    assert!(state.consumption_valid());
}

#[test]
fn unsupported_remote_content_is_preserved_as_conflict_without_losing_local_or_pending() {
    let mut state = synced();
    state.edit(&scope(), Some(snapshot("local typing")));
    let pending = state.begin_operation(&scope()).unwrap();
    let mut opaque = remote(&state, "opaque remote", "11.000001");
    opaque.snapshot.blocks[0]["unknown_metadata"] = serde_json::json!({"keep":[1,2,3]});
    opaque.snapshot.file_ids.push("FREMOTE".into());
    let mut unrelated = opaque.clone();
    unrelated.scope.key.team = "T2".into();
    assert!(!state.observe_conflict(unrelated));
    let mut stale = opaque.clone();
    stale.last_updated_ts = "9.000001".into();
    assert!(!state.observe_conflict(stale));
    let mut wrong_id = opaque.clone();
    wrong_id.id = "DrOther".into();
    assert!(!state.observe_conflict(wrong_id));
    assert!(state.observe_conflict(opaque.clone()));
    assert_eq!(state.desired(), Some(&snapshot("local typing")));
    assert_eq!(state.pending(), Some(&pending));
    assert_eq!(
        state.workflow(),
        &Workflow::Conflict {
            remote: Some(opaque),
            pending: Some(pending)
        }
    );
    assert!(state.begin_operation(&scope()).is_none());
}
