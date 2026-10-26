use super::*;
use crate::slack::api::custom::texty::file_titles::CanvasReferenceText;

const ID: &str = "FSTATUS0001";
fn event() -> FileDeletionEvent {
    FileDeletionEvent::decode(&json!({"type":"file_deleted","file_id":ID})).unwrap()
}
fn known() -> Store {
    let mut store = Store::new();
    store
        .ingest(&json!({"id":ID,"mode":"quip","title":"Original"}))
        .unwrap();
    store
}
fn response(raw: &Value) -> FileReferenceResponse {
    FileReferenceResponse::decode(ID, &serde_json::to_vec(raw).unwrap()).unwrap()
}

#[test]
fn all_eight_original_event_vectors_bind_presence_direct_replacement_and_presentation() {
    let reference: Value = serde_json::from_str(include_str!("original-event.json")).unwrap();
    let vectors = reference["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 8);
    for vector in vectors {
        let mut store = Store::new();
        for raw in vector["before"].as_object().unwrap().values() {
            // Structurally valid unsupported/unknown/tombstone rows are still
            // source-known; source presence must not become candidate filtering.
            let _ = store.ingest(raw);
            assert_eq!(store.source(raw["id"].as_str().unwrap()), Some(raw));
        }
        let before = store.candidates();
        let event = FileDeletionEvent::batch(vector["messages"].as_array().unwrap()).unwrap();
        let plan = store.prepare_deletion(&event);
        let observations = store.apply_deletion(plan).unwrap();
        let expected = vector["eventWrites"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|write| write["rows"].as_array().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(observations.len(), expected.len(), "{}", vector["name"]);
        assert_eq!(before.current(&store), expected.is_empty());
        for (observed, expected) in observations.iter().zip(expected) {
            assert!(observed.current(&store));
            assert!(observed.is_event_deletion());
            assert_eq!(observed.source(), expected);
            assert_eq!(observed.effective(), expected);
            assert_eq!(observed.receipt(), &vector["messages"]);
            assert_eq!(observed.value(), ReferenceValue::DeletedCanvas);
            let evidence = observed.deletion_event().unwrap();
            assert_eq!(evidence.event().raw(), &vector["messages"]);
            assert_eq!(evidence.preimage_source(), &vector["before"][observed.id()]);
            assert_eq!(
                evidence.preimage_receipt(),
                &vector["before"][observed.id()]
            );
            assert!(store.row(observed.id()).is_err());
            assert!(store.candidate(observed.id()).is_err());
        }
        for (id, raw) in vector["after"].as_object().unwrap() {
            assert_eq!(store.source(id), Some(raw), "{} {id}", vector["name"]);
        }
        assert_eq!(store.rows.len(), vector["after"].as_object().unwrap().len());
        if !observations.is_empty() {
            assert_eq!(
                vector["slug"]["label"],
                CanvasReferenceText::english().deleted_canvas()
            );
            assert_eq!(vector["slug"]["iconName"], "trash");
            assert_eq!(vector["slug"]["isDisabled"], true);
            assert_eq!(vector["slug"]["isLocked"], false);
            assert!(vector["lookupCalls"].as_array().unwrap().is_empty());
            assert!(vector["formatCalls"].as_array().unwrap().is_empty());
        }
    }
}

#[test]
fn source_identity_replacement_and_late_absence_plans_cannot_publish() {
    let mut store = known();
    let plan = store.prepare_deletion(&event());
    let mut foreign = known();
    assert!(foreign.apply_deletion(plan).is_err());
    assert_eq!(foreign.source(ID).unwrap()["title"], "Original");

    let plan = store.prepare_deletion(&event());
    let old = store.reference(ID).unwrap();
    store
        .ingest(&json!({"id":ID,"mode":"quip","title":"Original"}))
        .unwrap();
    assert!(!old.current(&store));
    assert!(store.apply_deletion(plan).is_err());
    assert!(!store.reference(ID).unwrap().is_event_deletion());

    let mut empty = Store::new();
    let late = empty.prepare_deletion(&event());
    empty
        .ingest(&json!({"id":ID,"mode":"quip","title":"New"}))
        .unwrap();
    assert!(empty.apply_deletion(late).is_err());
    assert_eq!(empty.source(ID).unwrap()["title"], "New");
}

#[test]
fn presence_includes_unsupported_rows_and_never_adds_absent_ids() {
    let mut store = Store::new();
    let raw = json!({"id":ID,"mode":"external","opaque":{"keep":1}});
    assert!(store.ingest(&raw).is_err());
    let e = FileDeletionEvent::decode(&json!({"type":"file_deleted","file_ids":[ID,"FABSENT"]}))
        .unwrap();
    let plan = store.prepare_deletion(&e);
    let out = store.apply_deletion(plan).unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].deletion_event().unwrap().preimage_source(), &raw);
    assert_eq!(
        out[0].source(),
        &json!({"id":ID,"mode":"tombstone","is_deleted":true})
    );
    assert!(store.source("FABSENT").is_none());
}

#[test]
fn ordinary_and_api_sources_never_mint_or_retain_event_origin() {
    let mut store = known();
    let plan = store.prepare_deletion(&event());
    let old = store.apply_deletion(plan).unwrap().remove(0);
    let partial = response(&json!({"ok":true,"file":{"id":ID,"updated":102}}));
    let next = store.ingest_reference("T1", &partial).unwrap();
    assert!(!old.current(&store));
    assert!(!next.is_event_deletion());
    assert_eq!(next.value(), ReferenceValue::DeletedCanvas);
    assert_eq!(next.effective()["is_deleted"], false);
    let normal = response(&json!({"ok":true,"file":{"id":ID,"mode":"quip","title":"Recovered"}}));
    let next = store.ingest_reference("T1", &normal).unwrap();
    assert!(!next.is_event_deletion());
    assert!(matches!(next.value(), ReferenceValue::Normal(_)));
    let private = response(&json!({"ok":false,"error":"not_visible"}));
    let next = store.ingest_reference("T1", &private).unwrap();
    assert_eq!(next.value(), ReferenceValue::PrivateCanvas);
    assert!(!next.is_event_deletion());
    let api = response(&json!({"ok":false,"error":"file_deleted"}));
    let next = store.ingest_reference("T1", &api).unwrap();
    assert!(!next.is_event_deletion());
    assert_eq!(next.value(), ReferenceValue::DeletedCanvas);
    for raw in [
        json!({"id":ID,"mode":"tombstone","is_deleted":true}),
        json!({"id":ID,"isUnknown":true}),
        json!({"id":ID,"mode":"quip","title":"Normal"}),
    ] {
        let mut other = Store::new();
        let _ = other.ingest(&raw);
        assert!(other.reference(ID).map_or(true, |o| !o.is_event_deletion()));
    }
}

#[test]
fn atomic_capacity_failure_and_repeated_events_do_not_retain_row_chains() {
    let mut store = known();
    for n in 0..16 {
        store
            .ingest(&json!({"id":format!("F{n}"),"mode":"quip","title":"x"}))
            .unwrap();
    }
    let old = store.candidates();
    let ids = store.order.clone();
    let big = FileDeletionEvent::decode(
        &json!({"type":"file_deleted","file_ids":ids,"opaque":"x".repeat(600_000)}),
    )
    .unwrap();
    let plan = store.prepare_deletion(&big);
    assert!(store.apply_deletion(plan).is_err());
    assert!(old.current(&store));
    drop(old);
    for _ in 0..128 {
        let prior = Arc::downgrade(store.rows.get(ID).unwrap());
        let plan = store.prepare_deletion(&event());
        let observed = store.apply_deletion(plan).unwrap();
        assert!(observed[0].current(&store));
        assert!(
            prior.upgrade().is_none(),
            "stored evidence retained an old Row"
        );
        assert!(store.rows[ID].bytes < 4096);
    }
}
