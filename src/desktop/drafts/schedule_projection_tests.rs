use super::*;
use super_platinum_core::{
    ComposerState,
    drafts::{
        store::{
            LocalWriter,
            sync::{BlockedLocal, Stored},
        },
        sync::{ConsumeKind, DraftSync, RemoteDraft, Scope},
    },
    scheduled::ScheduledMessage,
};

#[test]
fn queue_projection_requires_absence_or_strictly_newer_known_revision() {
    let incoming = ScheduledMessage {
        id: "Dr1".into(),
        last_updated_ts: "200.000001".into(),
        ..Default::default()
    };
    assert!(advances_queue(None, &incoming));
    for revision in ["", "200.000001", "201.000001"] {
        let current = ScheduledMessage {
            id: "Dr1".into(),
            last_updated_ts: revision.into(),
            ..Default::default()
        };
        assert!(!advances_queue(Some(&current), &incoming));
    }
    let current = ScheduledMessage {
        id: "Dr1".into(),
        last_updated_ts: "199.000001".into(),
        ..Default::default()
    };
    assert!(advances_queue(Some(&current), &incoming));
}
fn completed() -> (ConsumeIntent, DispatchResult, View) {
    let scope = Scope {
        account_id: LocalWriter::default().id().into(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        },
    };
    completed_in(scope)
}
fn completed_in(scope: Scope) -> (ConsumeIntent, DispatchResult, View) {
    let snapshot =
        crate::drafts::codec::encode_composer(&scope.key, &ComposerState::with_text("Frozen"))
            .unwrap();
    let base = RemoteDraft {
        scope: scope.clone(),
        id: "Dr1".into(),
        client_msg_id: LocalWriter::default().id().into(),
        last_updated_ts: "100.000001".into(),
        snapshot,
    };
    let mut state = DraftSync::adopt_remote(&scope, base.clone()).unwrap();
    let intent = state
        .request_consume(
            &scope,
            state.local_revision(),
            ConsumeKind::Schedule { post_at: 500 },
        )
        .unwrap();
    let operation = state.begin_operation(&scope).unwrap();
    let outcome = Outcome::Scheduled {
        draft: RemoteDraft {
            last_updated_ts: "200.000001".into(),
            ..base
        },
        post_at: 500,
    };
    assert!(crate::drafts::dispatch::apply(
        &mut state, &operation, &outcome
    ));
    let stored = Stored { revision: 3, state };
    let view = View {
        local: LocalDraft::default(),
        stored: Some(stored.clone()),
        blocked: None,
        uncommitted: None,
    };
    (
        intent,
        DispatchResult {
            operation,
            stored,
            applied: true,
        },
        view,
    )
}
fn snapshot(view: View) -> crate::drafts::checkpoint::Snapshot {
    crate::drafts::checkpoint::Snapshot {
        serial: 2,
        view,
        pending: None,
        error: None,
        unsaved: false,
        running: false,
        observing: false,
        projection: None,
    }
}
#[test]
fn final_projection_keeps_pending_input_newer_than_refresh_snapshot_and_blocked_metadata() {
    let (intent, receipt, view) = completed();
    let mut incoming = view.clone();
    let mut latest = snapshot(view.clone());
    let newer = LocalDraft {
        composer: ComposerState::with_text("New pending input"),
        attachments: vec![],
    };
    latest.pending = Some(newer.clone());
    let mut current = view;
    current.blocked = Some(BlockedLocal {
        token: "blocked-current".into(),
        draft: newer.clone(),
    });
    assert!(merge_projection(
        &mut incoming,
        latest,
        &current,
        &receipt,
        &intent,
        Some(receipt.stored.state.client_msg_id())
    ));
    assert!(crate::drafts::same_local(
        incoming.uncommitted.as_ref().unwrap(),
        &newer
    ));
    assert_eq!(incoming.blocked.unwrap().token, "blocked-current");
}
#[test]
fn lower_revision_foreign_enrollment_cannot_be_overwritten_by_old_projection() {
    let (intent, receipt, view) = completed();
    let (_, _, mut foreign) = completed_in(intent.scope.clone());
    foreign.stored.as_mut().unwrap().revision = 1;
    let mut incoming = view.clone();
    assert!(!merge_projection(
        &mut incoming,
        snapshot(view),
        &foreign,
        &receipt,
        &intent,
        Some(receipt.stored.state.client_msg_id())
    ));
}
#[test]
fn equal_revision_conflicting_editor_state_rejects_projection() {
    let (intent, receipt, view) = completed();
    let mut other = view.clone();
    let mut changed = intent.snapshot.clone();
    changed.blocks = vec![serde_json::json!({"type":"rich_text","elements":[]})];
    assert!(
        other
            .stored
            .as_mut()
            .unwrap()
            .state
            .edit(&intent.scope, Some(changed))
    );
    let mut incoming = view.clone();
    assert!(!merge_projection(
        &mut incoming,
        snapshot(other),
        &view,
        &receipt,
        &intent,
        Some(receipt.stored.state.client_msg_id())
    ));
}

#[test]
fn stale_refresh_metadata_never_resurrects_over_current_pending_or_blocked() {
    let (intent, receipt, view) = completed();
    let old = LocalDraft {
        composer: ComposerState::with_text("Old pending"),
        attachments: vec![],
    };
    let newer = LocalDraft {
        composer: ComposerState::with_text("Current pending"),
        attachments: vec![],
    };
    let mut incoming = view.clone();
    incoming.uncommitted = Some(old.clone());
    incoming.blocked = Some(BlockedLocal {
        token: "old".into(),
        draft: old,
    });
    let mut current = view.clone();
    current.uncommitted = Some(newer.clone());
    current.blocked = Some(BlockedLocal {
        token: "current".into(),
        draft: newer.clone(),
    });
    assert!(merge_projection(
        &mut incoming,
        snapshot(view),
        &current,
        &receipt,
        &intent,
        Some(receipt.stored.state.client_msg_id())
    ));
    assert!(crate::drafts::same_local(
        incoming.uncommitted.as_ref().unwrap(),
        &newer
    ));
    assert_eq!(incoming.blocked.unwrap().token, "current");
}

#[test]
fn competing_latest_editor_and_current_blocked_versions_refuse_projection() {
    let (intent, receipt, view) = completed();
    let mut latest = snapshot(view.clone());
    latest.view.blocked = Some(BlockedLocal {
        token: "latest-editor".into(),
        draft: LocalDraft::default(),
    });
    let mut current = view.clone();
    current.blocked = Some(BlockedLocal {
        token: "current-ui".into(),
        draft: LocalDraft::default(),
    });
    let mut incoming = view;
    assert!(!merge_projection(
        &mut incoming,
        latest,
        &current,
        &receipt,
        &intent,
        Some(receipt.stored.state.client_msg_id())
    ));
}
