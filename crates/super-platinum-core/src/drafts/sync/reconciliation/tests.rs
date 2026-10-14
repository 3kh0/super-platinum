use super::*;
use crate::{
    ComposerState,
    drafts::{DraftKey, codec},
};
fn snapshot(key: &DraftKey, text: &str) -> Snapshot {
    let c = ComposerState::with_text(text);
    codec::encode_composer(key, &c).unwrap()
}
fn setup() -> (DraftSync, RemoteDraft) {
    let scope = Scope {
        account_id: uuid::Uuid::new_v4().to_string(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        },
    };
    let base = RemoteDraft {
        scope: scope.clone(),
        id: "D1".into(),
        client_msg_id: uuid::Uuid::new_v4().to_string(),
        last_updated_ts: "10.000001".into(),
        snapshot: snapshot(&scope.key, "base"),
    };
    let mut state = DraftSync::adopt_remote(&scope, base.clone()).unwrap();
    state.edit(&scope, Some(snapshot(&scope.key, "local")));
    let mut remote = base;
    remote.last_updated_ts = "11.000001".into();
    remote.snapshot = snapshot(&scope.key, "remote");
    state
        .observe_event(&DraftObservation {
            scope,
            draft_id: remote.id.clone(),
            client_msg_id: remote.client_msg_id.clone(),
            event_ts: "20.000001".into(),
            kind: ObservationKind::Active(remote.clone()),
        })
        .unwrap();
    (state, remote)
}
#[test]
fn explicit_local_and_remote_choices_preserve_identity_and_event_evidence() {
    let (state, remote) = setup();
    assert!(matches!(state.workflow(), Workflow::Conflict { .. }));
    for desired in [
        state.desired().cloned(),
        Some(remote.snapshot.clone()),
        None,
    ] {
        let resolved = resolve_reviewed_remote(&state, &remote, desired.clone()).unwrap();
        assert_eq!(resolved.desired(), desired.as_ref());
        assert_eq!(resolved.acknowledged(), Some(&remote));
        assert_eq!(resolved.client_msg_id(), state.client_msg_id());
        assert_eq!(resolved.observed_events, state.observed_events);
        assert_eq!(resolved.retired_ids, state.retired_ids);
        assert_eq!(resolved.workflow(), &Workflow::Idle);
        assert_eq!(resolved.local_revision(), state.local_revision() + 1);
        assert!(resolved.observation_state_valid());
        assert_eq!(resolved.pending(), None);
    }
}
#[test]
fn pending_consumption_and_blocked_evidence_cannot_be_resolved() {
    let (state, remote) = setup();
    let mut pending = DraftSync::adopt_remote(state.scope(), remote.clone()).unwrap();
    pending.edit(state.scope(), state.desired().cloned());
    pending.begin_operation(state.scope()).unwrap();
    assert_eq!(
        resolve_reviewed_remote(&pending, &remote, None),
        Err(ReconcileError::Blocked)
    );
    let mut consuming = DraftSync::adopt_remote(state.scope(), remote.clone()).unwrap();
    consuming
        .request_consume(state.scope(), consuming.local_revision(), ConsumeKind::Send)
        .unwrap();
    assert_eq!(
        resolve_reviewed_remote(&consuming, &remote, None),
        Err(ReconcileError::Blocked)
    );
    let mut blocked = state.clone();
    blocked.blocked_observation = state.observed_events.values().next().cloned();
    assert_eq!(
        resolve_reviewed_remote(&blocked, &remote, None),
        Err(ReconcileError::Blocked)
    );
}
#[test]
fn foreign_retired_older_and_equal_revision_conflicting_reads_are_rejected() {
    let (state, remote) = setup();
    let mut wrong = remote.clone();
    wrong.id = "D2".into();
    assert_eq!(
        resolve_reviewed_remote(&state, &wrong, None),
        Err(ReconcileError::Identity)
    );
    wrong = remote.clone();
    wrong.client_msg_id = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        resolve_reviewed_remote(&state, &wrong, None),
        Err(ReconcileError::Identity)
    );
    wrong = remote.clone();
    wrong.last_updated_ts = "10.000001".into();
    assert_eq!(
        resolve_reviewed_remote(&state, &wrong, None),
        Err(ReconcileError::StaleRemote)
    );
    wrong = remote.clone();
    wrong.snapshot = snapshot(&state.scope.key, "different at same revision");
    assert_eq!(
        resolve_reviewed_remote(&state, &wrong, None),
        Err(ReconcileError::StaleRemote)
    );
    let mut retired = state.clone();
    retired.retired_ids.push(remote.id.clone());
    assert_eq!(
        resolve_reviewed_remote(&retired, &remote, None),
        Err(ReconcileError::Identity)
    );
}
#[test]
fn unsupported_content_and_exhausted_revision_are_atomic() {
    let (state, remote) = setup();
    let before = state.clone();
    let mut unsupported = remote.snapshot.clone();
    unsupported.destinations[0]["unknown"] = serde_json::json!(true);
    assert_eq!(
        resolve_reviewed_remote(&state, &remote, Some(unsupported)),
        Err(ReconcileError::Unsupported)
    );
    assert_eq!(state, before);
    let mut exhausted = state;
    exhausted.local_revision = u64::MAX;
    assert_eq!(
        resolve_reviewed_remote(&exhausted, &remote, None),
        Err(ReconcileError::Revision)
    );
}

#[test]
fn ambiguous_incoming_and_retained_revision_precision_stays_unsupported() {
    let (state, remote) = setup();
    for stamp in ["11", "11.1", "11.1000000"] {
        let mut incoming = remote.clone();
        incoming.last_updated_ts = stamp.into();
        assert_eq!(
            resolve_reviewed_remote(&state, &incoming, None),
            Err(ReconcileError::Unsupported)
        );
    }
    // Each retained comparison operand is independently checked. In particular,
    // 10.1 is numerically newer than 10.000002 but ts_key would say the reverse.
    for location in 0..3 {
        let mut retained = state.clone();
        match location {
            0 => retained.acknowledged.as_mut().unwrap().last_updated_ts = "10.1".into(),
            1 => {
                let Workflow::Conflict {
                    remote: Some(old), ..
                } = &mut retained.workflow
                else {
                    panic!("conflict")
                };
                old.last_updated_ts = "10.1".into();
            }
            _ => {
                let event = retained.observed_events.values_mut().next().unwrap();
                let ObservationKind::Active(old) = &mut event.kind else {
                    panic!("active")
                };
                old.last_updated_ts = "10.1".into();
            }
        }
        let before = retained.clone();
        let mut incoming = remote.clone();
        incoming.last_updated_ts = "10.000002".into();
        assert_eq!(
            resolve_reviewed_remote(&retained, &incoming, None),
            Err(ReconcileError::Unsupported)
        );
        assert_eq!(retained, before);
    }
    assert!(resolve_reviewed_remote(&state, &remote, None).is_ok());
}
