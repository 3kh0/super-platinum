use super::super::tests::{Database, enroll, local, scope};
use super::*;
use crate::drafts::sync::{ConsumeKind, RemoteDraft};

fn top_scope() -> Scope {
    let mut s = scope();
    s.key.thread = None;
    s
}
fn alternate(row: &LocalAlternate) -> LocalChoice {
    LocalChoice::Alternate {
        writer_id: row.writer_id.clone(),
        revision: row.revision,
    }
}
fn retain(store: &mut DraftStore, scope: &Scope, writer: &LocalWriter, revision: u64, text: &str) {
    assert!(
        !store
            .save_owned_local_if_unchanged(
                scope,
                &local("obsolete"),
                &local(text),
                writer,
                revision
            )
            .unwrap()
    );
}

#[test]
fn settlement_selects_exact_version_retires_all_reviewed_versions_and_survives_restart() {
    let db = Database::new();
    let scope = top_scope();
    let a = LocalWriter::default();
    let b = LocalWriter::default();
    let mut store = db.open();
    store.save([(scope.key.clone(), local("current"))]).unwrap();
    retain(&mut store, &scope, &a, 1, "A first");
    retain(&mut store, &scope, &a, 2, "A latest");
    retain(&mut store, &scope, &b, 3, "B latest");
    let review = store.read_local_review(&scope).unwrap();
    assert!(review.contains_local(&local("A first")));
    assert!(review.contains_local(&local("B latest")));
    assert!(!review.contains_local(&local("not reviewed")));
    let selected = review
        .alternates
        .iter()
        .find(|row| row.writer_id == b.id())
        .unwrap();
    assert!(matches!(
        store
            .settle_local_versions(&review, &alternate(selected))
            .unwrap(),
        LocalSettlement::Applied { stored: None, .. }
    ));
    drop(store);
    let mut store = db.open();
    let settled = store.read_local_review(&scope).unwrap();
    assert_eq!(settled.local.composer.text, "B latest");
    assert!(settled.blocked.is_none() && settled.alternates.is_empty());
    // Delayed writes cannot recreate metadata OR replace chosen local, even
    // when their expected local value happens to equal the chosen value.
    assert!(
        !store
            .save_owned_local_if_unchanged(&scope, &local("B latest"), &local("stale A"), &a, 2)
            .unwrap()
    );
    store
        .preserve_owned_local_conflict(&scope, &local("stale B"), &b, 3)
        .unwrap();
    let stale = store.read_local_review(&scope).unwrap();
    assert!(stale.blocked.is_none() && stale.alternates.is_empty());
    assert_eq!(stale.local.composer.text, "B latest");
    assert!(
        !store
            .save_owned_local_if_unchanged(&scope, &local("B latest"), &local("newer A"), &a, 4)
            .unwrap()
    );
    let next = store.read_local_review(&scope).unwrap();
    assert_eq!(next.local.composer.text, "B latest");
    assert_eq!(next.alternates[0].draft.composer.text, "newer A");
}

#[test]
fn full_inventory_cas_rejects_new_writer_new_revision_local_and_blocked_changes() {
    for change in 0..4 {
        let db = Database::new();
        let scope = top_scope();
        let mut store = db.open();
        let a = LocalWriter::default();
        store.save([(scope.key.clone(), local("current"))]).unwrap();
        retain(&mut store, &scope, &a, 1, "A");
        let review = store.read_local_review(&scope).unwrap();
        match change {
            0 => retain(&mut store, &scope, &LocalWriter::default(), 1, "unseen B"),
            1 => retain(&mut store, &scope, &a, 2, "unseen A"),
            2 => store
                .save([(scope.key.clone(), local("new local"))])
                .unwrap(),
            _ => {
                store
                    .0
                    .execute("UPDATE composer_draft_blocked SET token='new token'", [])
                    .unwrap();
            }
        }
        assert!(matches!(
            store
                .settle_local_versions(&review, &LocalChoice::Current)
                .unwrap(),
            LocalSettlement::Stale
        ));
        assert!(
            !store
                .read_local_review(&scope)
                .unwrap()
                .alternates
                .is_empty()
        );
    }
}

#[test]
fn managed_settlement_preserves_immutable_send_and_inflight_operation() {
    let db = Database::new();
    let scope = top_scope();
    let mut store = db.open();
    let row = enroll(&mut store, &scope);
    let mut state = row.state;
    let intent = state
        .request_consume(&scope, state.local_revision(), ConsumeKind::Send)
        .unwrap();
    let Commit::Applied(row) = store
        .commit_synced(
            &scope,
            Some(row.revision),
            &LocalDraft::default(),
            &state,
            None,
        )
        .unwrap()
    else {
        panic!()
    };
    let Prepare::Prepared { operation, .. } = store.prepare_synced(&scope, row.revision).unwrap()
    else {
        panic!()
    };
    let writer = LocalWriter::default();
    store
        .save_owned_local_alternate(&scope, &local("next local"), &writer, 1)
        .unwrap();
    let review = store.read_local_review(&scope).unwrap();
    let LocalSettlement::Applied {
        stored: Some(row),
        local: chosen,
    } = store
        .settle_local_versions(&review, &alternate(&review.alternates[0]))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(chosen.composer.text, "next local");
    assert_eq!(row.state.consumption(), Some(&intent));
    assert_eq!(row.state.pending(), Some(&operation));
    assert_eq!(row.state.workflow(), &Workflow::InFlight(operation));
    assert_eq!(
        row.state.desired(),
        Some(&codec::encode(&scope.key, "next local").unwrap())
    );
    assert!(row.revision > review.stored.unwrap().revision);
}

#[test]
fn opaque_remote_conflict_and_current_local_are_preserved_exactly() {
    let db = Database::new();
    let scope = top_scope();
    let mut store = db.open();
    let row = enroll(&mut store, &scope);
    let Prepare::Prepared {
        stored: row,
        operation,
    } = store.prepare_synced(&scope, row.revision).unwrap()
    else {
        panic!()
    };
    let mut state = row.state;
    let remote = RemoteDraft {
        scope: scope.clone(),
        id: "Dr1".into(),
        client_msg_id: state.client_msg_id().into(),
        last_updated_ts: "100".into(),
        snapshot: operation.snapshot.clone().unwrap(),
    };
    assert!(state.observe_conflict(remote));
    let Commit::Applied(row) = store
        .commit_synced(
            &scope,
            Some(row.revision),
            &local("Original 🚀"),
            &state,
            None,
        )
        .unwrap()
    else {
        panic!()
    };
    store
        .save_owned_local_alternate(
            &scope,
            &local("discard reviewed local"),
            &LocalWriter::default(),
            1,
        )
        .unwrap();
    let review = store.read_local_review(&scope).unwrap();
    let LocalSettlement::Applied {
        stored: Some(after),
        ..
    } = store
        .settle_local_versions(&review, &LocalChoice::Current)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(after.state, row.state);
    assert!(matches!(after.state.workflow(), Workflow::Conflict { .. }));
}

#[test]
fn unsupported_managed_choice_keeps_all_versions_and_wrong_scope_cannot_settle() {
    let db = Database::new();
    let scope = top_scope();
    let mut store = db.open();
    enroll(&mut store, &scope);
    store
        .save_owned_local_alternate(&scope, &local("<@U123>"), &LocalWriter::default(), 1)
        .unwrap();
    let review = store.read_local_review(&scope).unwrap();
    assert!(matches!(
        store
            .settle_local_versions(&review, &alternate(&review.alternates[0]))
            .unwrap(),
        LocalSettlement::Blocked
    ));
    assert!(review.matches(&store.read_local_review(&scope).unwrap()));
    let mut foreign = review.clone();
    foreign.scope.account_id = uuid::Uuid::new_v4().to_string();
    assert!(
        store
            .settle_local_versions(&foreign, &LocalChoice::Current)
            .is_err()
    );
    let mut foreign = review.clone();
    foreign.scope.key.channel = "C2".into();
    assert!(matches!(
        store
            .settle_local_versions(&foreign, &LocalChoice::Current)
            .unwrap(),
        LocalSettlement::Stale
    ));
    let bad = LocalChoice::Alternate {
        writer_id: review.alternates[0].writer_id.clone(),
        revision: 999,
    };
    assert!(matches!(
        store.settle_local_versions(&review, &bad).unwrap(),
        LocalSettlement::Stale
    ));
}

#[test]
fn blocked_choice_is_exact_and_sync_receipt_revision_invalidates_review() {
    let db = Database::new();
    let scope = top_scope();
    let mut store = db.open();
    let row = enroll(&mut store, &scope);
    store
        .save_owned_local_alternate(
            &scope,
            &local("selected first blocked"),
            &LocalWriter::default(),
            1,
        )
        .unwrap();
    let review = store.read_local_review(&scope).unwrap();
    store
        .commit_synced(&scope, Some(row.revision), &review.local, &row.state, None)
        .unwrap();
    assert!(matches!(
        store
            .settle_local_versions(&review, &LocalChoice::Current)
            .unwrap(),
        LocalSettlement::Stale
    ));
    let review = store.read_local_review(&scope).unwrap();
    let choice = LocalChoice::Blocked {
        token: review.blocked.as_ref().unwrap().token.clone(),
    };
    let LocalSettlement::Applied { local, .. } =
        store.settle_local_versions(&review, &choice).unwrap()
    else {
        panic!()
    };
    assert_eq!(local.composer.text, "selected first blocked");
}

#[test]
fn choosing_thread_alternate_preserves_broadcast_but_flag_only_is_not_a_message() {
    for text in ["Restored reply", ""] {
        let db = Database::new();
        let scope = scope();
        let mut store = db.open();
        enroll(&mut store, &scope);
        let mut chosen = local(text);
        chosen.composer.reply_broadcast = true;
        store
            .save_owned_local_alternate(&scope, &chosen, &LocalWriter::default(), 1)
            .unwrap();
        let review = store.read_local_review(&scope).unwrap();
        let LocalSettlement::Applied {
            stored: Some(row),
            local,
        } = store
            .settle_local_versions(&review, &alternate(&review.alternates[0]))
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(
            serde_json::to_value(local).unwrap(),
            serde_json::to_value(chosen).unwrap()
        );
        if text.is_empty() {
            assert!(row.state.desired().is_none());
        } else {
            assert_eq!(
                row.state.desired().unwrap().destinations[0]["broadcast"],
                true
            );
            assert_eq!(
                row.state.desired().unwrap().destinations[0]["thread_ts"],
                "1.2"
            );
        }
        drop(store);
        assert!(
            db.open()
                .read_local_review(&scope)
                .unwrap()
                .local
                .composer
                .reply_broadcast
        );
    }
}
