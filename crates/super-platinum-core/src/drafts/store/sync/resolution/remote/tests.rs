use super::*;
use crate::{ComposerState, drafts::sync::DraftSync};
use serde_json::json;
fn local(text: &str) -> LocalDraft {
    let composer = ComposerState::with_text(text);
    LocalDraft {
        composer,
        attachments: vec![],
    }
}
fn setup() -> (DraftStore, Scope, Value) {
    setup_store(DraftStore::open(":memory:").unwrap())
}
fn setup_store(mut store: DraftStore) -> (DraftStore, Scope, Value) {
    let scope = Scope {
        account_id: uuid::Uuid::new_v4().to_string(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        },
    };
    let value = local("local");
    let base = RemoteDraft {
        scope: scope.clone(),
        id: "D1".into(),
        client_msg_id: uuid::Uuid::new_v4().to_string(),
        last_updated_ts: "10.000001".into(),
        snapshot: codec::encode_composer(&scope.key, &value.composer).unwrap(),
    };
    store.save([(scope.key.clone(), value.clone())]).unwrap();
    let state = DraftSync::adopt_remote(&scope, base.clone()).unwrap();
    assert!(matches!(
        store
            .commit_synced(&scope, None, &value, &state, None)
            .unwrap(),
        Commit::Applied(_)
    ));
    let snapshot = codec::encode_composer(&scope.key, &local("remote").composer).unwrap();
    let raw = json!({"ok":true,"draft":{"id":base.id,"client_msg_id":base.client_msg_id,"last_updated_ts":"11.000001","team_id":"T1","user_id":"U1","is_from_composer":false,"is_deleted":false,"is_sent":false,"date_scheduled":0,"blocks":snapshot.blocks,"file_ids":snapshot.file_ids,"destinations":snapshot.destinations}});
    (store, scope, raw)
}
#[test]
fn explicit_remote_choice_settles_exact_reviewed_inventory_in_one_transaction() {
    let (mut store, scope, raw) = setup();
    let writer = LocalWriter::default();
    store
        .preserve_owned_local_conflict(&scope, &local("alternate"), &writer, 1)
        .unwrap();
    let inventory = store.read_local_review(&scope).unwrap();
    let before = inventory.stored.as_ref().unwrap().revision;
    let review = RemoteReview::prepare(inventory, raw.clone(), "U1").unwrap();
    assert_eq!(review.raw_info(), &raw);
    let RemoteSettlement::Applied {
        local: chosen,
        stored,
    } = store
        .reconcile_remote_review(&review, &RemoteChoice::Remote)
        .unwrap()
    else {
        panic!("applied")
    };
    assert_eq!(chosen.composer.text, "remote");
    assert!(stored.revision > before);
    assert_eq!(stored.state.desired(), Some(&review.remote().snapshot));
    let current = store.read_local_review(&scope).unwrap();
    assert!(current.blocked.is_none() && current.alternates.is_empty());
    store
        .preserve_owned_local_conflict(&scope, &local("late old writer"), &writer, 1)
        .unwrap();
    assert!(
        store
            .read_local_review(&scope)
            .unwrap()
            .alternates
            .is_empty()
    );
    assert!(matches!(
        store
            .reconcile_remote_review(&review, &RemoteChoice::Remote)
            .unwrap(),
        RemoteSettlement::Stale
    ));
}
#[test]
fn explicit_local_choice_remains_desired_against_refreshed_same_remote() {
    let (mut store, scope, raw) = setup();
    let writer = LocalWriter::default();
    store
        .preserve_owned_local_conflict(&scope, &local("chosen alternate"), &writer, 1)
        .unwrap();
    let inventory = store.read_local_review(&scope).unwrap();
    let choice = LocalChoice::Alternate {
        writer_id: writer.id().into(),
        revision: 1,
    };
    let review = RemoteReview::prepare(inventory, raw, "U1").unwrap();
    let RemoteSettlement::Applied {
        local: chosen,
        stored,
    } = store
        .reconcile_remote_review(&review, &RemoteChoice::Local(choice))
        .unwrap()
    else {
        panic!("applied")
    };
    assert_eq!(chosen.composer.text, "chosen alternate");
    assert_eq!(stored.state.acknowledged(), Some(review.remote()));
    assert_ne!(stored.state.desired(), Some(&review.remote().snapshot));
    assert!(stored.state.pending().is_none());
}
#[test]
fn new_alternate_or_paired_change_rejects_entire_review_without_retiring_evidence() {
    for paired in [false, true] {
        let (mut store, scope, raw) = setup();
        let review =
            RemoteReview::prepare(store.read_local_review(&scope).unwrap(), raw, "U1").unwrap();
        if paired {
            let row = review.inventory().stored.as_ref().unwrap();
            let mut state = row.state.clone();
            state.edit(
                &scope,
                Some(codec::encode_composer(&scope.key, &local("newer").composer).unwrap()),
            );
            store
                .commit_synced(&scope, Some(row.revision), &local("newer"), &state, None)
                .unwrap();
        } else {
            store
                .preserve_owned_local_conflict(
                    &scope,
                    &local("new writer"),
                    &LocalWriter::default(),
                    1,
                )
                .unwrap();
        }
        assert!(matches!(
            store
                .reconcile_remote_review(&review, &RemoteChoice::Remote)
                .unwrap(),
            RemoteSettlement::Stale
        ));
        let current = store.read_local_review(&scope).unwrap();
        if paired {
            assert_eq!(current.local.composer.text, "newer")
        } else {
            assert_eq!(current.alternates[0].draft.composer.text, "new writer")
        }
    }
}
#[test]
fn foreign_lifecycle_unknown_fields_and_absence_never_form_a_review() {
    let (mut store, scope, raw) = setup();
    for (key, value) in [
        ("id", json!("D2")),
        ("client_msg_id", json!(uuid::Uuid::new_v4().to_string())),
        ("user_id", json!("U2")),
        ("is_deleted", json!(true)),
        ("is_sent", json!(true)),
        ("date_scheduled", json!(123)),
        ("is_from_composer", json!(true)),
        ("is_from_composer", json!("false")),
        ("last_updated_ts", json!("11.1")),
        ("unknown", json!({})),
    ] {
        let mut changed = raw.clone();
        changed["draft"][key] = value;
        assert!(
            RemoteReview::prepare(store.read_local_review(&scope).unwrap(), changed, "U1").is_err(),
            "{key}"
        );
    }
    assert!(
        RemoteReview::prepare(
            store.read_local_review(&scope).unwrap(),
            json!({"ok":false,"error":"draft_not_found"}),
            "U1"
        )
        .is_err()
    );
    assert_eq!(
        store.read_local_review(&scope).unwrap().local.composer.text,
        "local"
    );
}

#[test]
fn resolved_pair_and_retired_writer_floor_survive_disk_restart() {
    let db = crate::drafts::store::sync::tests::Database::new();
    let (mut store, scope, raw) = setup_store(db.open());
    let writer = LocalWriter::default();
    store
        .preserve_owned_local_conflict(&scope, &local("reviewed alternate"), &writer, 7)
        .unwrap();
    let review =
        RemoteReview::prepare(store.read_local_review(&scope).unwrap(), raw, "U1").unwrap();
    assert!(matches!(
        store
            .reconcile_remote_review(&review, &RemoteChoice::Remote)
            .unwrap(),
        RemoteSettlement::Applied { .. }
    ));
    drop(store);
    let mut store = db.open();
    let restored = store.read_local_review(&scope).unwrap();
    assert_eq!(restored.local.composer.text, "remote");
    assert_eq!(
        restored.stored.as_ref().unwrap().state.acknowledged(),
        Some(review.remote())
    );
    assert!(restored.alternates.is_empty() && restored.blocked.is_none());
    store
        .preserve_owned_local_conflict(&scope, &local("late retired input"), &writer, 7)
        .unwrap();
    assert!(
        store
            .read_local_review(&scope)
            .unwrap()
            .alternates
            .is_empty()
    );
}

#[test]
fn review_independently_rejects_non_null_error_and_preserves_warning_metadata() {
    let (mut store, scope, mut raw) = setup();
    raw["warning"] = json!("ordinary warning");
    raw["response_metadata"] = json!({"warnings":["ordinary warning"]});
    let inventory = store.read_local_review(&scope).unwrap();
    assert_eq!(
        RemoteReview::prepare(inventory.clone(), raw.clone(), "U1")
            .unwrap()
            .raw_info(),
        &raw
    );
    for error in [
        json!("failed"),
        json!(false),
        json!(0),
        json!(""),
        json!([]),
        json!({}),
    ] {
        let mut changed = raw.clone();
        changed["error"] = error;
        assert!(RemoteReview::prepare(inventory.clone(), changed, "U1").is_err());
    }
    raw["error"] = Value::Null;
    assert_eq!(
        RemoteReview::prepare(inventory, raw.clone(), "U1")
            .unwrap()
            .raw_info(),
        &raw
    );
    assert_eq!(
        store.read_local_review(&scope).unwrap().local.composer.text,
        "local"
    );
}
