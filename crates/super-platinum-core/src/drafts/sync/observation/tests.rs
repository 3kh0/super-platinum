use super::*;

fn scope() -> Scope {
    Scope {
        account_id: "00000000-0000-4000-8000-000000000001".into(),
        key: DraftKey { team: "T1".into(), channel: "C1".into(), thread: Some("1.000001".into()) },
    }
}
fn remote(text: &str, revision: &str) -> RemoteDraft {
    RemoteDraft {
        scope: scope(), id: "Dr1".into(),
        client_msg_id: "00000000-0000-4000-8000-000000000002".into(),
        last_updated_ts: revision.into(),
        snapshot: crate::drafts::codec::encode(&scope().key, text).unwrap(),
    }
}
fn state() -> DraftSync { DraftSync::adopt_remote(&scope(), remote("Original", "1000")).unwrap() }
fn event(text: &str, revision: &str, stamp: &str) -> DraftObservation {
    let remote = remote(text, revision);
    DraftObservation {
        scope: scope(), draft_id: remote.id.clone(), client_msg_id: remote.client_msg_id.clone(),
        event_ts: stamp.into(), kind: ObservationKind::Active(remote),
    }
}

#[test]
fn ordering_is_per_owned_identity_and_separate_from_payload_revision() {
    let mut state = state();
    let latest = event("New", "2000", "10.000002");
    assert_eq!(state.observe_event(&latest), Ok(true));
    let accepted = state.clone();
    assert_eq!(state.observe_event(&latest), Ok(false));
    assert_eq!(state.observe_event(&event("Older event", "3000", "10.000001")), Ok(false));
    assert_eq!(state, accepted);
    assert_eq!(state.observe_event(&event("Older payload", "1500", "10.000003")), Ok(true));
    assert_eq!(state.desired(), accepted.desired());
    assert_eq!(state.acknowledged(), accepted.acknowledged());
    assert_eq!(state.observed_event_ts("Dr1"), Some("10.000003"));
    let mut delete = latest;
    delete.event_ts = "10.000004".into();
    delete.kind = ObservationKind::Deleted;
    assert_eq!(state.observe_event(&delete), Ok(true));
    assert!(state.desired().is_none());
    assert_eq!(state.observe_event(&event("Late", "9000", "99.000000")), Ok(false));
}

#[test]
fn equal_event_clock_contradiction_is_retained_and_blocks_all_intent() {
    let mut state = state();
    let first = event("New", "2000", "10.000001");
    state.observe_event(&first).unwrap();
    let contradiction = event("Contradiction", "3000", "10.000001");
    assert_eq!(state.observe_event(&contradiction), Ok(true));
    assert_eq!(state.blocked_observation(), Some(&contradiction));
    assert!(matches!(state.workflow(), Workflow::Conflict { .. }));
    assert_eq!(state.desired(), Some(&remote("New", "2000").snapshot));
    assert!(state.begin_operation(&scope()).is_none());
    assert_eq!(state.request_consume(&scope(), state.local_revision(), ConsumeKind::Send), Err(ConsumeError::Busy));
    // Newer ordinary events and legacy observers cannot unpark this conflict.
    state.observe_event(&event("Later", "4000", "10.000002")).unwrap();
    state.observe(remote("Later again", "5000"));
    state.observe_deleted(&scope(), "Dr1");
    assert!(state.observation_state_valid());
    assert!(matches!(state.workflow(), Workflow::Conflict { .. }));
}

#[test]
fn equal_payload_revision_conflicts_even_with_newer_event_clock() {
    let mut state = state();
    assert_eq!(state.observe_event(&event("Contradiction", "1000", "10.000001")), Ok(true));
    assert!(matches!(state.workflow(), Workflow::Conflict { remote: Some(remote), .. } if remote.snapshot == self::remote("Contradiction", "1000").snapshot));
    assert_eq!(state.desired(), Some(&remote("Original", "1000").snapshot));
}

#[test]
fn dirty_local_edits_and_opaque_content_are_preserved_as_conflicts() {
    let mut dirty = state();
    let local = remote("Local", "1000").snapshot;
    dirty.edit(&scope(), Some(local.clone()));
    dirty.observe_event(&event("Remote", "2000", "10.000001")).unwrap();
    assert_eq!(dirty.desired(), Some(&local));
    assert!(matches!(dirty.workflow(), Workflow::Conflict { remote: Some(_), .. }));
    let mut opaque = state();
    let mut update = event("Remote", "2000", "10.000001");
    if let ObservationKind::Active(remote) = &mut update.kind { remote.snapshot.file_ids.push("F1".into()); }
    opaque.observe_event(&update).unwrap();
    assert_eq!(opaque.desired(), state().desired());
    assert!(matches!(opaque.workflow(), Workflow::Conflict { remote: Some(remote), .. } if remote.snapshot.file_ids == ["F1"]));
    let mut older = event("Old opaque", "999", "10.000002");
    if let ObservationKind::Active(remote) = &mut older.kind { remote.snapshot.file_ids.push("F2".into()); }
    let conflict = opaque.workflow().clone();
    opaque.observe_event(&older).unwrap();
    assert_eq!(opaque.workflow(), &conflict);
    assert_eq!(opaque.observed_event_ts("Dr1"), Some("10.000002"));
}

#[test]
fn unsupported_lifecycle_retains_full_evidence_and_cannot_resume_after_restart() {
    let mut state = state();
    let mut observed = event("unused", "2000", "10.000001");
    let raw = serde_json::json!({"id":"Dr1", "client_msg_id":observed.client_msg_id,
        "attachments":[{"unknown":"Retain me"}], "date_scheduled":123});
    observed.kind = ObservationKind::Unsupported(raw.clone());
    state.observe_event(&observed).unwrap();
    let mut restored: DraftSync = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert_eq!(restored.blocked_observation().unwrap().kind, ObservationKind::Unsupported(raw));
    assert!(restored.observation_state_valid());
    assert!(restored.begin_operation(&scope()).is_none());
    assert!(restored.request_consume(&scope(), restored.local_revision(), ConsumeKind::Send).is_err());
}

#[test]
fn malformed_foreign_and_unowned_observations_change_nothing() {
    for kind in ["scope", "uuid", "server", "timestamp", "content"] {
        let mut state = state();
        let before = state.clone();
        let mut update = event("New", "2000", "10.000001");
        match kind {
            "scope" => update.scope.key.channel = "C2".into(),
            "uuid" => update.client_msg_id = "00000000-0000-4000-8000-000000000099".into(),
            "server" => update.draft_id = "DrOther".into(),
            "timestamp" => update.event_ts = "10.1".into(),
            "content" => if let ObservationKind::Active(remote) = &mut update.kind { remote.snapshot.destinations.clear(); },
            _ => unreachable!(),
        }
        assert!(state.observe_event(&update).is_err());
        assert_eq!(state, before);
    }
    let mut unmanaged = DraftSync::new(scope());
    assert_eq!(unmanaged.observe_event(&event("New", "2000", "10.000001")), Err(ObservationError::Identity));
}

#[test]
fn create_echo_can_bind_only_its_original_uuid_and_delete_cannot_confirm_send() {
    let mut state = DraftSync::new(scope());
    state.edit(&scope(), Some(remote("Original", "1000").snapshot));
    let create = state.begin_operation(&scope()).unwrap();
    let mut echo = event("Original", "2000", "10.000001");
    echo.client_msg_id = create.client_msg_id.clone();
    if let ObservationKind::Active(remote) = &mut echo.kind { remote.client_msg_id = create.client_msg_id.clone(); }
    let mut deleted_first = state.clone();
    let mut delete = echo.clone();
    delete.kind = ObservationKind::Deleted;
    deleted_first.observe_event(&delete).unwrap();
    assert_eq!(deleted_first.pending(), Some(&create));
    assert_eq!(deleted_first.observe_event(&echo), Ok(false));
    assert!(deleted_first.begin_operation(&scope()).is_none());
    state.observe_event(&echo).unwrap();
    assert_eq!(state.acknowledged().unwrap().id, "Dr1");
    state.request_consume(&scope(), state.local_revision(), ConsumeKind::Send).unwrap();
    let send = state.begin_operation(&scope()).unwrap();
    delete.event_ts = "10.000002".into();
    state.observe_event(&delete).unwrap();
    assert!(matches!(state.workflow(), Workflow::Uncertain(op) if op == &send));
    assert!(state.consumption().is_some());
    assert!(state.observation_state_valid());
}
