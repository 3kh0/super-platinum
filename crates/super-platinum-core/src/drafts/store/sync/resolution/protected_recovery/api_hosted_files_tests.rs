//! Private SQLite regressions; no network, configured store, UI or secret backend.
use super::*;
use crate::drafts::store::sync::tests::local;
use serde_json::json;

struct Fixture {
    store: DraftStore,
    scope: Scope,
    stream: Stream,
    review: Review,
    writer: Writer,
}
fn source(id: &str) -> serde_json::Value {
    json!({"id":id,"mode":"hosted","is_external":false,"filetype":"pdf","title":"Synthetic Ω"})
}
impl Fixture {
    fn new() -> Self {
        Self::at(":memory:")
    }
    fn at(path: impl AsRef<std::path::Path>) -> Self {
        let mut store = DraftStore::open(path).unwrap();
        let scope = Scope {
            account_id: uuid::Uuid::new_v4().to_string(),
            key: DraftKey {
                team: "T00000001".into(),
                channel: "C00000001".into(),
                thread: None,
            },
        };
        storage::activate(&store, &scope.account_id).unwrap();
        adapter_schema::install(&store, &scope.account_id).unwrap();
        store
            .save([(scope.key.clone(), local("Exact caption Ω"))])
            .unwrap();
        let inventory = views::local(&store.0, &scope).unwrap();
        let review = operations::enroll(&store, &scope, &inventory).unwrap();
        let state = operations::stream_state(&store, &scope.account_id, &scope.key.team).unwrap();
        let stream =
            operations::open_stream(&store, &scope.account_id, &scope.key.team, &state).unwrap();
        let review = operations::verify(&store, &stream, &review).unwrap();
        let writer = writers::register(&store, &review).unwrap();
        let WriteResult::Applied(review) =
            writers::checkpoint(&store, &writer, 1, Some(&review), review.local(), None).unwrap()
        else {
            panic!("writer receipt required")
        };
        let tx = adapter_atomic::begin(&store.0).unwrap();
        initialize_transaction(&tx, &scope.account_id).unwrap();
        tx.commit().unwrap();
        Self {
            store,
            scope,
            stream,
            review,
            writer,
        }
    }
    fn input(&self) -> Input {
        let mut snapshot = self.review.row().state().desired().unwrap().clone();
        snapshot.file_ids = vec!["FSECOND".into(), "FFIRST".into()];
        Input::new(snapshot, vec![source("FSECOND"), source("FFIRST")]).unwrap()
    }
    fn claim(&self) -> Result<Claim, Error> {
        claim(
            &self.store,
            &self.stream,
            &self.review,
            &self.writer,
            1,
            self.input(),
        )
    }
    fn unchanged(&self) {
        assert!(
            views::review(&self.store.0, &self.scope)
                .unwrap()
                .same(&self.review)
        );
        assert!(
            retained(&self.store, &self.scope.account_id)
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn ordered_sources_and_entire_caption_are_atomically_retained_without_cleanup() {
    let f = Fixture::new();
    let original = serde_json::to_value(f.review.local()).unwrap();
    let c = f.claim().unwrap();
    let r = c.record();
    assert_eq!(r.input().snapshot().file_ids, vec!["FSECOND", "FFIRST"]);
    assert_eq!(r.input().sources(), &[source("FSECOND"), source("FFIRST")]);
    assert_eq!(r.input().request().unwrap().destination(), "C00000001");
    let after = views::review(&f.store.0, &f.scope).unwrap();
    assert_eq!(after.row().revision(), f.review.row().revision() + 1);
    assert_eq!(after.row().state(), f.review.row().state());
    assert_eq!(serde_json::to_value(after.local()).unwrap(), original);
    assert!(guard(&f.store.0, &f.scope).is_err());
    assert!(f.claim().is_err());
    assert_eq!(retained(&f.store, &f.scope.account_id).unwrap().len(), 1);
}

#[test]
fn rejects_duplicate_missing_reordered_malformed_and_unsupported_sources() {
    let f = Fixture::new();
    let valid = f.input();
    for sources in [
        vec![source("FFIRST"), source("FSECOND")],
        vec![source("FSECOND")],
        vec![source("FSECOND"), json!({"id":"FFIRST","mode":"external"})],
    ] {
        assert!(Input::new(valid.snapshot().clone(), sources).is_err());
    }
    let mut duplicate = valid.snapshot().clone();
    duplicate.file_ids[1] = "FSECOND".into();
    assert!(Input::new(duplicate, vec![source("FSECOND"), source("FSECOND")]).is_err());
    let mut malformed = source("FFIRST");
    malformed["is_deleted"] = json!("false");
    assert!(Input::new(valid.snapshot().clone(), vec![source("FSECOND"), malformed]).is_err());
    let mut thread = valid.snapshot().clone();
    thread.destinations[0]["thread_ts"] = json!("1.000001");
    assert!(Input::new(thread, valid.sources().to_vec()).is_err());
    f.unchanged();
}

#[test]
fn stale_caption_writer_and_stream_cannot_claim_or_change_storage() {
    let f = Fixture::new();
    let valid = f.input();
    let mut snapshot = valid.snapshot().clone();
    snapshot.blocks.clear();
    let changed = Input::new(snapshot, valid.sources().to_vec()).unwrap();
    assert!(claim(&f.store, &f.stream, &f.review, &f.writer, 1, changed).is_err());
    f.unchanged();
    assert!(claim(&f.store, &f.stream, &f.review, &f.writer, 2, valid).is_err());
    f.unchanged();
    f.stream.revoke();
    assert!(f.claim().is_err());
    f.unchanged();
}

#[test]
fn changed_full_local_inventory_prevents_claim() {
    let f = Fixture::new();
    // An external same-byte write increments the local clock: identity includes
    // that clock even when caption, destinations and source IDs look unchanged.
    f.store
        .0
        .execute(
            "UPDATE composer_drafts SET content=content WHERE destination=?1",
            [serde_json::to_string(&f.scope.key).unwrap()],
        )
        .unwrap();
    let before = views::local(&f.store.0, &f.scope).unwrap();
    assert!(f.claim().is_err());
    assert!(retained(&f.store, &f.scope.account_id).unwrap().is_empty());
    assert!(views::local(&f.store.0, &f.scope).unwrap().same(&before));
}

#[test]
fn lost_claim_return_remains_retained_after_private_store_reopen() {
    let path =
        std::env::temp_dir().join(format!("sp-hosted-custody-{}.sqlite", uuid::Uuid::new_v4()));
    let f = Fixture::at(&path);
    let c = f.claim().unwrap();
    let id = c.record().id().to_owned();
    let account = f.scope.account_id.clone();
    let scope = f.scope.clone();
    drop(c);
    drop(f);
    let store = DraftStore::open(&path).unwrap();
    let rows = retained(&store, &account).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id(), id);
    assert!(rows[0].returned().is_none());
    assert!(guard(&store.0, &scope).is_err());
    drop(store);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn exact_outcome_settles_after_stream_revocation_and_never_unlocks() {
    for result in [
        Returned::Response(json!({"ok":true,"opaque":"retained"})),
        Returned::Response(json!({"ok":false,"error":"restricted_action"})),
        Returned::Unknown,
        Returned::RateLimited {
            retry_after_secs: 7,
        },
    ] {
        let f = Fixture::new();
        let c = f.claim().unwrap();
        f.stream.revoke();
        let first = settle(&f.store, &c, result.clone()).unwrap();
        assert!(first.returned() == Some(&result));
        let second = settle(&f.store, &c, result).unwrap();
        assert_eq!(
            serde_json::to_value(&first).unwrap(),
            serde_json::to_value(&second).unwrap()
        );
        assert!(
            settle(
                &f.store,
                &c,
                Returned::RateLimited {
                    retry_after_secs: 19
                }
            )
            .is_err()
        );
        assert!(guard(&f.store.0, &f.scope).is_err());
        assert!(f.claim().is_err());
    }
}

#[test]
fn failed_protected_row_write_rolls_back_record_and_original_row() {
    let f = Fixture::new();
    f.store.0.execute_batch("CREATE TEMP TRIGGER hosted_row_failure BEFORE UPDATE ON composer_draft_sync BEGIN SELECT RAISE(ABORT,'synthetic'); END;").unwrap();
    assert!(f.claim().is_err());
    f.unchanged();
}

#[test]
fn corrupt_index_or_required_outcome_fails_closed() {
    let f = Fixture::new();
    let c = f.claim().unwrap();
    f.store
        .0
        .execute(
            "UPDATE composer_draft_hosted_records SET enrollment='changed'",
            [],
        )
        .unwrap();
    assert!(retained(&f.store, &f.scope.account_id).is_err());
    assert!(guard(&f.store.0, &f.scope).is_err());
    assert!(settle(&f.store, &c, Returned::Unknown).is_err());
    let mut record = serde_json::to_value(c.record()).unwrap();
    record.as_object_mut().unwrap().remove("returned");
    assert!(serde_json::from_value::<Record>(record).is_err());
}

#[test]
fn initialization_requires_owned_transaction_and_partial_schema_blocks() {
    let f = Fixture::new();
    assert!(initialize_transaction(&f.store.0, &f.scope.account_id).is_err());
    f.store
        .0
        .execute_batch("DROP TABLE composer_draft_hosted_format")
        .unwrap();
    assert!(guard(&f.store.0, &f.scope).is_err());
    assert!(f.claim().is_err());
}

#[test]
fn oversized_source_and_invalid_return_do_not_replace_original_custody() {
    let f = Fixture::new();
    let valid = f.input();
    let mut large = source("FFIRST");
    large["title"] = json!("x".repeat(256 * 1024));
    assert!(Input::new(valid.snapshot().clone(), vec![source("FSECOND"), large]).is_err());
    f.unchanged();
    let c = f.claim().unwrap();
    assert!(settle(&f.store, &c, Returned::Response(json!([true]))).is_err());
    assert!(
        settle(
            &f.store,
            &c,
            Returned::RateLimited {
                retry_after_secs: 0
            }
        )
        .is_err()
    );
    assert!(
        retained(&f.store, &f.scope.account_id).unwrap()[0]
            .returned()
            .is_none()
    );
}
