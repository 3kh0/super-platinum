use super::*;

fn foreign() -> (Scope, RemoteDraft) {
    let expected = Scope {
        account_id: uuid::Uuid::new_v4().to_string(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: Some("1.2".into()),
        },
    };
    let remote = RemoteDraft {
        scope: expected.clone(),
        id: "DrForeign".into(),
        client_msg_id: uuid::Uuid::new_v4().to_string(),
        last_updated_ts: "10.001".into(),
        snapshot: content("foreign raw"),
    };
    (expected, remote)
}

#[test]
fn explicit_foreign_adoption_preserves_opaque_content_and_updates_same_identity() {
    let (scope, mut remote) = foreign();
    remote.snapshot.destinations[0]["server_extension"] = serde_json::json!({"preserve":true});
    let mut state = DraftSync::adopt_remote(&scope, remote.clone()).unwrap();
    assert_eq!(state.scope(), &scope);
    assert_eq!(state.client_msg_id(), remote.client_msg_id);
    assert_eq!(state.desired(), Some(&remote.snapshot));
    assert_eq!(state.acknowledged(), Some(&remote));
    assert_eq!(state.local_revision(), 0);
    assert_eq!(state.acknowledged_local_revision(), 0);
    assert!(!state.tombstone());
    assert_eq!(state.workflow(), &Workflow::Idle);
    assert!(state.begin_operation(&scope).is_none());
    assert!(super::super::codec::decode(&scope.key, &remote.snapshot).is_err());
    let serialized = serde_json::to_string(&state).unwrap();
    let restored: DraftSync = serde_json::from_str(&serialized).unwrap();
    assert_eq!(restored.restore_after_restart(), state);
    state.edit(&scope, Some(content("edited")));
    let operation = state.begin_operation(&scope).unwrap();
    assert_eq!(operation.kind, OperationKind::Update);
    assert_eq!(operation.client_msg_id, remote.client_msg_id);
    assert_eq!(operation.base, Some(remote));
}

#[test]
fn foreign_adoption_rejects_scope_identity_and_revision_mismatch() {
    let (scope, remote) = foreign();
    let mut other = scope.clone();
    other.account_id = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        DraftSync::adopt_remote(&other, remote.clone()),
        Err(AdoptError::Scope)
    );
    other = scope.clone();
    other.key.team = "T2".into();
    assert_eq!(
        DraftSync::adopt_remote(&other, remote.clone()),
        Err(AdoptError::Scope)
    );
    for id in ["", "bad uuid", "00000000-0000-0000-0000-000000000000"] {
        let mut invalid = remote.clone();
        invalid.client_msg_id = id.into();
        assert_eq!(
            DraftSync::adopt_remote(&scope, invalid),
            Err(AdoptError::Identity)
        );
    }
    for id in ["", " Dr1", "Dr1\n", "Dr one"] {
        let mut invalid = remote.clone();
        invalid.id = id.into();
        assert_eq!(
            DraftSync::adopt_remote(&scope, invalid),
            Err(AdoptError::Identity)
        );
    }
    for revision in [
        "",
        "0",
        "0.1",
        "NaN",
        "-1.1",
        " 1.1",
        "1.",
        "1.2.3",
        "1.+2",
        "18446744073709551616.1",
    ] {
        let mut invalid = remote.clone();
        invalid.last_updated_ts = revision.into();
        assert_eq!(
            DraftSync::adopt_remote(&scope, invalid),
            Err(AdoptError::Revision)
        );
    }
}

#[test]
fn foreign_adoption_rejects_malformed_or_ambiguous_destinations_and_content() {
    let (scope, remote) = foreign();
    for destinations in [
        vec![],
        vec![serde_json::json!({"channel_id":"C2","thread_ts":"1.2"})],
        vec![serde_json::json!({"channel_id":"C1"})],
        vec![serde_json::json!({"channel_id":"C1","thread_ts":"9.1"})],
        vec![serde_json::json!({"channel_id":"C1","thread_ts":"1.2","broadcast":"true"})],
        vec![
            remote.snapshot.destinations[0].clone(),
            remote.snapshot.destinations[0].clone(),
        ],
    ] {
        let mut invalid = remote.clone();
        invalid.snapshot.destinations = destinations;
        assert_eq!(
            DraftSync::adopt_remote(&scope, invalid),
            Err(AdoptError::Destination)
        );
    }
    let mut invalid = remote.clone();
    invalid.snapshot.file_ids.push("".into());
    assert_eq!(
        DraftSync::adopt_remote(&scope, invalid),
        Err(AdoptError::Content)
    );
    for block in [
        Value::Null,
        serde_json::json!({}),
        serde_json::json!({"type":""}),
    ] {
        let mut invalid = remote.clone();
        invalid.snapshot.blocks = vec![block];
        assert_eq!(
            DraftSync::adopt_remote(&scope, invalid),
            Err(AdoptError::Content)
        );
    }
    let mut channel = scope.clone();
    channel.key.thread = None;
    let mut top = remote;
    top.scope = channel.clone();
    top.snapshot.destinations = vec![serde_json::json!({"channel_id":"C1"})];
    assert!(DraftSync::adopt_remote(&channel, top.clone()).is_ok());
    top.snapshot.destinations[0]["thread_ts"] = Value::Null;
    assert_eq!(
        DraftSync::adopt_remote(&channel, top),
        Err(AdoptError::Destination)
    );
}
fn scope() -> Scope {
    Scope {
        account_id: "account-a".into(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: Some("1.2".into()),
        },
    }
}
fn content(text: &str) -> Snapshot {
    Snapshot {
        blocks: vec![
            serde_json::json!({"type":"rich_text","unknown_extension":{"keep":true},"elements":[{"type":"rich_text_section","elements":[{"type":"text","text":text,"style":{"bold":true}}]}]}),
        ],
        file_ids: vec!["F1".into()],
        destinations: vec![
            serde_json::json!({"channel_id":"C1","thread_ts":"1.2","broadcast":true}),
        ],
    }
}
fn remote(state: &DraftSync, text: &str, revision: &str) -> RemoteDraft {
    RemoteDraft {
        scope: scope(),
        id: "Dr1".into(),
        client_msg_id: state.client_msg_id().into(),
        last_updated_ts: revision.into(),
        snapshot: content(text),
    }
}
fn synced() -> DraftSync {
    let mut s = DraftSync::new(scope());
    s.edit(&scope(), Some(content("one")));
    let op = s.begin_operation(&scope()).unwrap();
    assert!(s.confirm(&op.id, remote(&s, "one", "10.1")));
    s
}
#[test]
fn rapid_edits_coalesce_and_older_response_does_not_erase_new_input() {
    let mut s = DraftSync::new(scope());
    s.edit(&scope(), Some(content("one")));
    s.edit(&scope(), Some(content("two")));
    let create = s.begin_operation(&scope()).unwrap();
    assert_eq!(create.snapshot, Some(content("two")));
    s.edit(&scope(), Some(content("three")));
    assert!(s.begin_operation(&scope()).is_none());
    assert!(s.confirm(&create.id, remote(&s, "two", "10.1")));
    assert_eq!(s.desired(), Some(&content("three")));
    assert_eq!(s.acknowledged_local_revision(), 2);
    let update = s.begin_operation(&scope()).unwrap();
    assert_eq!(update.kind, OperationKind::Update);
    assert_eq!(update.snapshot, Some(content("three")));
    assert!(!s.confirm(&create.id, remote(&s, "two", "10.1")));
    assert_eq!(s.pending().unwrap().id, update.id);
}
#[test]
fn uncertain_create_keeps_exact_uuid_until_positive_matching_observation() {
    let mut s = DraftSync::new(scope());
    s.edit(&scope(), Some(content("one")));
    let op = s.begin_operation(&scope()).unwrap();
    assert!(s.mark_uncertain(&scope(), &op.id));
    let uuid = s.client_msg_id().to_owned();
    let bytes = serde_json::to_vec(&s).unwrap();
    let mut s: DraftSync = serde_json::from_slice(&bytes).unwrap();
    s = s.restore_after_restart();
    assert_eq!(s.client_msg_id(), uuid);
    assert!(s.begin_operation(&scope()).is_none());
    let mut unrelated = remote(&s, "one", "10.1");
    unrelated.client_msg_id = "other".into();
    assert!(!s.observe(unrelated));
    assert!(s.observe(remote(&s, "one", "10.1")));
    assert_eq!(s.acknowledged().unwrap().id, "Dr1");
    assert_eq!(s.workflow(), &Workflow::Idle);
    assert!(s.begin_operation(&scope()).is_none());
    let mut duplicate = remote(&s, "one", "10.2");
    duplicate.id = "Dr2".into();
    assert!(!s.observe(duplicate));
    assert_eq!(s.acknowledged().unwrap().id, "Dr1");
}
#[test]
fn matching_uuid_or_stale_create_does_not_confirm_a_later_update() {
    let mut s = synced();
    s.edit(&scope(), Some(content("two")));
    let op = s.begin_operation(&scope()).unwrap();
    s.mark_uncertain(&scope(), &op.id);
    assert!(!s.observe(remote(&s, "one", "10.1")));
    assert!(matches!(s.workflow(), Workflow::Uncertain(_)));
    assert!(!s.confirm(&op.id, remote(&s, "two", "10.0")));
    assert!(s.observe(remote(&s, "two", "10.2")));
    assert!(s.begin_operation(&scope()).is_none());
}
#[test]
fn clearing_uncertain_create_survives_restart_then_deletes_adopted_id() {
    let mut s = DraftSync::new(scope());
    s.edit(&scope(), Some(content("one")));
    let op = s.begin_operation(&scope()).unwrap();
    s.edit(&scope(), None);
    let mut s = serde_json::from_slice::<DraftSync>(&serde_json::to_vec(&s).unwrap())
        .unwrap()
        .restore_after_restart();
    assert!(s.tombstone());
    assert!(s.desired().is_none());
    assert_eq!(s.pending().unwrap().id, op.id);
    assert!(s.begin_operation(&scope()).is_none());
    assert!(s.observe(remote(&s, "one", "10.1")));
    let deletion = s.begin_operation(&scope()).unwrap();
    assert_eq!(deletion.kind, OperationKind::Delete);
    let mut s = serde_json::from_slice::<DraftSync>(&serde_json::to_vec(&s).unwrap())
        .unwrap()
        .restore_after_restart();
    assert!(s.begin_operation(&scope()).is_none());
    assert!(s.confirm_deleted(&scope(), &deletion.id, "Dr1"));
    assert!(s.tombstone());
    assert!(s.begin_operation(&scope()).is_none());
    assert!(!s.observe(remote(&s, "one", "10.2")));
}
#[test]
fn account_workspace_and_operation_identity_isolate_mutations() {
    let mut s = synced();
    for foreign in [
        Scope {
            account_id: "account-b".into(),
            ..scope()
        },
        Scope {
            key: DraftKey {
                team: "T2".into(),
                ..scope().key
            },
            ..scope()
        },
    ] {
        assert!(!s.edit(&foreign, None));
        assert!(s.begin_operation(&foreign).is_none());
        let mut r = remote(&s, "other", "11.1");
        r.scope = foreign.clone();
        assert!(!s.observe(r));
        assert!(!s.observe_deleted(&foreign, "Dr1"));
    }
    s.edit(&scope(), Some(content("two")));
    let op = s.begin_operation(&scope()).unwrap();
    assert!(!s.mark_uncertain(&scope(), "old-op"));
    assert!(!s.reject(&scope(), "old-op"));
    assert_eq!(s.pending().unwrap().id, op.id);
}
#[test]
fn simultaneous_remote_edit_preserves_both_versions_and_blocks_autosave() {
    let mut s = synced();
    s.edit(&scope(), Some(content("mine")));
    let op = s.begin_operation(&scope()).unwrap();
    assert!(s.observe(remote(&s, "theirs", "11.1")));
    assert_eq!(s.desired(), Some(&content("mine")));
    assert!(matches!(
        s.workflow(),
        Workflow::Conflict {
            remote: Some(_),
            pending: Some(_)
        }
    ));
    assert!(s.begin_operation(&scope()).is_none());
    assert!(!s.confirm(&op.id, remote(&s, "mine", "11.2")));
    let restored: DraftSync = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
    assert_eq!(restored, s);
}
#[test]
fn newer_local_content_after_delete_is_not_erased_or_given_retired_uuid() {
    let mut s = synced();
    let old_uuid = s.client_msg_id().to_owned();
    s.edit(&scope(), None);
    let delete = s.begin_operation(&scope()).unwrap();
    s.edit(&scope(), Some(content("new")));
    assert!(s.confirm_deleted(&scope(), &delete.id, "Dr1"));
    assert_eq!(s.desired(), Some(&content("new")));
    let create = s.begin_operation(&scope()).unwrap();
    assert_eq!(create.kind, OperationKind::Create);
    assert_ne!(create.client_msg_id, old_uuid);
}
#[test]
fn clean_remote_updates_follow_server_but_dirty_remote_delete_is_conflict() {
    let mut s = synced();
    assert!(s.observe(remote(&s, "remote", "10.2")));
    assert_eq!(s.desired(), Some(&content("remote")));
    assert!(s.begin_operation(&scope()).is_none());
    s.edit(&scope(), Some(content("unsent")));
    assert!(s.observe_deleted(&scope(), "Dr1"));
    assert_eq!(s.desired(), Some(&content("unsent")));
    assert!(matches!(
        s.workflow(),
        Workflow::Conflict { remote: None, .. }
    ));
    assert!(s.begin_operation(&scope()).is_none());
}
#[test]
fn pending_update_retains_newer_unchanged_remote_revision() {
    let mut s = synced();
    s.edit(&scope(), Some(content("two")));
    let op = s.begin_operation(&scope()).unwrap();
    assert!(s.observe(remote(&s, "one", "12.0")));
    assert_eq!(s.acknowledged().unwrap().last_updated_ts, "12.0");
    assert!(!s.confirm(&op.id, remote(&s, "two", "11.0")));
    assert_eq!(s.pending().unwrap().id, op.id);
    assert_eq!(s.desired(), Some(&content("two")));
    let mut restored = serde_json::from_slice::<DraftSync>(&serde_json::to_vec(&s).unwrap())
        .unwrap()
        .restore_after_restart();
    assert!(!restored.confirm(&op.id, remote(&restored, "two", "11.5")));
    assert!(restored.confirm(&op.id, remote(&restored, "two", "13.0")));
}
#[test]
fn equal_revision_different_content_is_conflict_without_overwriting_base() {
    let mut s = synced();
    assert!(s.observe(remote(&s, "different", "10.1")));
    assert_eq!(s.desired(), Some(&content("one")));
    assert_eq!(s.acknowledged().unwrap().snapshot, content("one"));
    assert!(matches!(s.workflow(), Workflow::Conflict { .. }));
    assert!(s.begin_operation(&scope()).is_none());
    let mut pending = synced();
    pending.edit(&scope(), Some(content("two")));
    let op = pending.begin_operation(&scope()).unwrap();
    assert!(pending.observe(remote(&pending, "one", "12.0")));
    assert!(!pending.confirm(&op.id, remote(&pending, "two", "12.0")));
    assert!(matches!(
        pending.workflow(),
        Workflow::Conflict {
            pending: Some(_),
            ..
        }
    ));
    assert_eq!(pending.acknowledged().unwrap().snapshot, content("one"));
}
#[test]
fn positive_delete_before_create_response_survives_restart_and_blocks_resurrection() {
    let mut s = DraftSync::new(scope());
    s.edit(&scope(), Some(content("one")));
    let op = s.begin_operation(&scope()).unwrap();
    assert!(s.observe_deleted(&scope(), "Dr1"));
    let mut s = serde_json::from_slice::<DraftSync>(&serde_json::to_vec(&s).unwrap())
        .unwrap()
        .restore_after_restart();
    assert!(!s.confirm(&op.id, remote(&s, "one", "10.1")));
    assert!(!s.observe(remote(&s, "one", "10.2")));
    assert!(s.acknowledged().is_none());
    assert_eq!(s.desired(), Some(&content("one")));
    assert!(matches!(s.workflow(), Workflow::Uncertain(_)));
    assert!(s.begin_operation(&scope()).is_none());
}

#[test]
fn account_identity_validation_is_shared_with_renderers() {
    assert!(valid_account_id("00000000-0000-4000-8000-000000000001"));
    for invalid in ["", "not-an-account", "00000000-0000-0000-0000-000000000000"] {
        assert!(!valid_account_id(invalid));
    }
}
