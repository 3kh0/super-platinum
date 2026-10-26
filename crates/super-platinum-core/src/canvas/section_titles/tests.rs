use super::*;
use serde_json::json;
fn request(file: &str) -> CanvasSectionsRequest {
    CanvasSectionsRequest::new(file, &["s1".into()]).unwrap()
}
fn publish(store: &mut Store, file: &str, rows: Value) {
    let request = request(file);
    let read = store.capture_read(&request).unwrap();
    assert!(
        store
            .apply(&read, &request.decode(json!({"ok":true,"sections":rows})))
            .unwrap()
    );
}
fn seed(store: &mut Store, value: &Value) {
    if let Some(files) = value.as_object() {
        for (file, rows) in files {
            let rows: Vec<_> = rows
                .as_object()
                .unwrap()
                .iter()
                .map(|(id, block)| json!({"id":id,"rich_text_blocks":[block]}))
                .collect();
            publish(store, file, rows.into());
        }
    }
}
#[test]
fn original_acquisition_final_publications_and_undefined_keys() {
    let vectors: Value = serde_json::from_str(include_str!("original-acquisition.json")).unwrap();
    let mut checked = 0;
    for vector in vectors["vectors"].as_array().unwrap() {
        if vector["response"].is_null() {
            continue;
        } // Ensure scheduling is desktop-owned.
        let mut store = Store::default();
        seed(&mut store, &vector["initial"]);
        let request = request("FSTATUS0001");
        let read = store.capture_read(&request).unwrap();
        let response = if vector["response"].get("reject").is_some() {
            request.failed()
        } else {
            request.decode(vector["response"]["value"].clone())
        };
        assert!(store.apply(&read, &response).unwrap());
        let mut actual = serde_json::Map::new();
        for (file, rows) in &store.files {
            if rows.rows.is_empty() {
                continue;
            }
            let mut entries = serde_json::Map::new();
            for (id, value) in &rows.rows {
                if let Some(value) = value {
                    entries.insert(id.clone(), value.clone());
                }
            }
            actual.insert(file.clone(), Value::Object(entries));
        }
        assert_eq!(Value::Object(actual), vector["after"], "{}", vector["name"]);
        if vector["name"] == "empty-rich-block-list-own-undefined" {
            let observation = store.observe("FSTATUS0001", "s1").unwrap();
            assert!(observation.has_entry());
            assert!(observation.block().is_none());
        }
        checked += 1;
    }
    assert_eq!(checked, 6);
}
#[test]
fn exact_read_ownership_retires_late_results_but_omission_keeps_observation() {
    let mut store = Store::default();
    let missing = store.observe("F1", "s1").unwrap();
    assert!(!missing.has_entry());
    assert!(missing.block().is_none());
    let a = request("F1");
    let old = store.capture_read(&a).unwrap();
    publish(
        &mut store,
        "F1",
        json!([{"id":"s1","rich_text_blocks":[{"type":"rich_text","elements":[]}]}]),
    );
    assert!(!missing.current(&store));
    assert!(!store.apply(&old, &a.failed()).unwrap());
    let present = store.observe("F1", "s1").unwrap();
    let b = request("F1");
    let read = store.capture_read(&b).unwrap();
    assert!(store.apply(&read, &b.decode(json!({"ok":true}))).unwrap());
    assert!(present.current(&store));
    assert!(!store.apply(&read, &b.failed()).unwrap());
    let c = request("F1");
    let read = store.capture_read(&c).unwrap();
    assert!(store.apply(&read, &b.failed()).is_err());
    assert!(read.current(&store));
    assert!(!present.current(&Store::default()));
    assert!(present.current(&store.clone()));
    assert!(store.apply(&read, &c.failed()).unwrap());
    assert!(!present.current(&store));
}
#[test]
fn sibling_file_updates_do_not_retire_but_same_file_updates_do() {
    let mut store = Store::default();
    let request = request("F1");
    let read = store.capture_read(&request).unwrap();
    publish(&mut store, "F2", json!([{"id":"s1","rich_text_blocks":[]}]));
    assert!(read.current(&store));
    let foreign = Store::default();
    assert!(!read.current(&foreign));
    assert!(
        store
            .apply(
                &read,
                &request
                    .decode(json!({"ok":true,"sections":[{"id":"s2","rich_text_blocks":[null]}]}))
            )
            .unwrap()
    );
    let null = store.observe("F1", "s2").unwrap();
    assert!(null.has_entry());
    assert_eq!(null.block(), Some(&Value::Null));
}

#[test]
fn sequential_failures_do_not_fill_positive_or_absent_capacity() {
    let mut store = Store::default();
    for index in 0..512 {
        let request = request(&format!("F{index}"));
        let read = store.capture_read(&request).unwrap();
        assert!(store.apply(&read, &request.failed()).unwrap());
    }
    assert!(store.files.is_empty());
    assert!(store.absent.lock().unwrap().is_empty());
    publish(
        &mut store,
        "Ffresh",
        json!([{"id":"temp:C:fixture","rich_text_blocks":[{"type":"rich_text","elements":[]}]}]),
    );
    assert!(
        store
            .observe("Ffresh", "temp:C:fixture")
            .unwrap()
            .is_cached()
    );
}

#[test]
fn clear_is_file_local_and_absent_witness_never_revives_after_aba() {
    let mut store = Store::default();
    let first = store.observe("F1", "s1").unwrap();
    let second = store.observe("F2", "s1").unwrap();
    let old_request = request("F1");
    let old_read = store.capture_read(&old_request).unwrap();
    publish(&mut store, "F1", json!([{"id":"s1","rich_text_blocks":[]}]));
    let clear = request("F1");
    let read = store.capture_read(&clear).unwrap();
    assert!(store.apply(&read, &clear.failed()).unwrap());
    assert!(!first.current(&store));
    assert!(!old_read.current(&store));
    assert!(second.current(&store));
    let new = store.observe("F1", "s1").unwrap();
    assert!(new.current(&store));
    assert!(!first.current(&store));
    assert!(
        !store
            .apply(&old_read, &old_request.decode(json!({"ok":true})))
            .unwrap()
    );
}

#[test]
fn live_absent_witness_bound_recovers_and_clone_registry_is_independent() {
    let store = Store::default();
    let mut held = Vec::new();
    for index in 0..MAX_ABSENT {
        held.push(store.observe(&format!("F{index}"), "s1").unwrap());
    }
    assert!(store.observe("Fextra", "s1").is_err());
    held.pop();
    let extra = store.observe("Fextra", "s1").unwrap();
    let mut cloned = store.clone();
    assert!(extra.current(&cloned));
    let request = request("Fextra");
    let read = cloned.capture_read(&request).unwrap();
    assert!(cloned.apply(&read, &request.failed()).unwrap());
    assert!(extra.current(&store));
    assert!(!extra.current(&cloned));
    assert!(
        held.iter()
            .all(|observation| observation.current(&store) && observation.current(&cloned))
    );
}

#[test]
fn falsey_sections_are_noops_even_at_positive_capacity() {
    let mut store = Store::default();
    for index in 0..MAX_FILES {
        publish(
            &mut store,
            &format!("F{index}"),
            json!([{"id":"s1","rich_text_blocks":[]}]),
        )
    }
    let original = store.observe("F0", "s1").unwrap();
    for sections in [
        Value::Null,
        json!(false),
        json!(0),
        json!(0.0),
        json!(""),
        json!([]),
    ] {
        for file in ["F0", "Fnew"] {
            let request = request(file);
            let read = store.capture_read(&request).unwrap();
            assert!(
                store
                    .apply(
                        &read,
                        &request.decode(json!({"ok":true,"sections":sections}))
                    )
                    .unwrap()
            );
        }
        assert!(original.current(&store));
    }
    assert_eq!(store.files.len(), MAX_FILES);
    let request = request("Fnew");
    let read = store.capture_read(&request).unwrap();
    assert!(store.apply(&read, &request.failed()).unwrap());
    assert_eq!(store.files.len(), MAX_FILES);
    assert!(original.current(&store));
}
