use super::*;
use serde_json::json;

fn row(id: &str) -> Value {
    json!({"id":id,"name":"Canonical name","title":"Never the label","mode":"hosted",
        "permalink":"https://example.test/original","created":10,"opaque":{"preserved":"raw"}})
}

#[test]
fn all_65_original_process_file_vectors_match_selected_facts_and_bounded_links() {
    let vectors: Value = serde_json::from_str(include_str!("source_vectors.json")).unwrap();
    assert_eq!(vectors["vectors"].as_array().unwrap().len(), 65);
    for vector in vectors["vectors"].as_array().unwrap() {
        let previous = vector.get("previous").filter(|v| !v.is_null());
        let actual = projection::project(previous, &vector["incoming"]);
        assert_eq!(actual, vector["effective"], "{}", vector["case"]);
        let link = projection::link(&actual)
            .map(|link| json!({"name":link.name,"permalink":link.permalink}));
        assert_eq!(
            link,
            vector.get("link").filter(|v| !v.is_null()).cloned(),
            "{}",
            vector["case"]
        );
    }
}

#[test]
fn duplicate_batch_occurrences_use_one_preimage_and_last_assignment() {
    let mut store = Store::default();
    let original = row("F1");
    store
        .observe_batch(std::slice::from_ref(&original))
        .unwrap();
    let old = store.get("F1").unwrap();
    let old_effective = old.effective().clone();
    let batch = vec![
        json!({"id":"F1","name":"Earlier duplicate","permalink":"https://example.test/earlier"}),
        json!({"id":"F1","permalink":"https://example.test/last"}),
    ];
    let untouched = batch.clone();
    store.observe_batch(&batch).unwrap();
    let current = store.get("F1").unwrap();
    assert_eq!(current.link().unwrap().name, "Canonical name");
    assert_eq!(
        current.link().unwrap().permalink,
        "https://example.test/last"
    );
    assert_eq!(current.source(), &batch[1]);
    assert_eq!(current.batch(), batch.as_slice());
    assert_eq!(current.predecessor(), Some(&old_effective));
    assert_eq!(old.source(), &original);
    assert_eq!(batch, untouched);
    assert!(!old.current(&store));
    assert!(current.current(&store));
    assert!(current.revision() > old.revision());
    assert!(current.effective().get("title").is_none());
    assert!(current.effective().get("opaque").is_none());
}

#[test]
fn reset_order_undefined_and_explicit_values_are_not_merged_twice() {
    let previous =
        json!({"id":"F1","name":"Old","permalink":"old","is_tombstoned":true,"restriction_type":2});
    let actual = projection::project(
        Some(&previous),
        &json!({"id":"F1","restriction_type":2,"permalink":"new"}),
    );
    assert_eq!(
        actual,
        json!({"id":"F1","restriction_type":2,"permalink":"new"})
    );
    // Equal object contents from separate receipts are unequal JS references.
    let previous = json!({"id":"F1","name":"Old","restriction_type":{"x":1}});
    let actual = projection::project(
        Some(&previous),
        &json!({"id":"F1","restriction_type":{"x":1}}),
    );
    assert_eq!(actual, json!({"id":"F1","restriction_type":{"x":1}}));
    let previous = json!({"id":"F1","name":"Old","restriction_type":0,"permalink":"old"});
    let actual = projection::project(
        Some(&previous),
        &json!({"id":"F1","restriction_type":null,"name":"","permalink":null}),
    );
    assert_eq!(
        actual,
        json!({"id":"F1","is_tombstoned":false,"restriction_type":null,"name":"","permalink":null})
    );
    let actual = projection::project(Some(&previous), &json!({"id":"F1"}));
    assert!(actual.get("permalink").is_none());
    assert_eq!(actual["name"], "Old");
}

#[test]
fn status_truthiness_numeric_dsa_and_explicit_non_refusals() {
    for key in [
        "isNonExistent",
        "is_deleted",
        "is_tombstoned",
        "is_hidden_by_limit",
    ] {
        for value in [json!(true), json!(1), json!("false"), json!([]), json!({})] {
            let mut raw = row("F1");
            raw[key] = value;
            assert!(!should_show_file(&raw), "{key}: {raw}");
            assert_eq!(
                projection::project(Some(&row("F1")), &raw),
                Value::Object(facts(&raw))
            );
        }
        for value in [Value::Null, json!(false), json!(0), json!("")] {
            let mut raw = row("F1");
            raw[key] = value;
            assert!(should_show_file(&raw), "{key}: {raw}");
        }
    }
    for restriction in [json!("1"), json!(true), json!(0), json!(2), Value::Null] {
        assert!(should_show_file(&json!({"restriction_type":restriction})));
    }
    assert!(!should_show_file(&json!({"restriction_type":1.0})));
    assert!(!should_show_file(&Value::Null));
    for fields in [
        json!({"isUnknown":true}),
        json!({"is_locked":true}),
        json!({"file_access":"file_deleted"}),
    ] {
        assert!(should_show_file(&fields));
    }
}

#[test]
fn incoming_timestamp_truthiness_clears_only_after_eligibility() {
    let old = json!({"id":"F1","isUnknown":true,"isNonExistent":true});
    for timestamp in [json!(1), json!(-1), json!("0"), json!([]), json!({})] {
        let actual = projection::project(
            Some(&old),
            &json!({"id":"F1","updated":0,"created":timestamp}),
        );
        assert_eq!(actual["isUnknown"], false);
        assert_eq!(actual["isNonExistent"], false);
    }
    for timestamp in [json!(0), json!(false), json!(""), Value::Null] {
        let actual = projection::project(Some(&old), &json!({"id":"F1","updated":timestamp}));
        assert_eq!(actual["isUnknown"], true);
        assert_eq!(actual["isNonExistent"], true);
    }
    let incoming = json!({"id":"F1","isNonExistent":true,"updated":999});
    assert_eq!(projection::project(Some(&old), &incoming), incoming);
}

#[test]
fn first_selection_and_embedded_sidecar_have_distinct_duplicate_semantics() {
    let messages = vec![json!({"files":[
        {"id":"FMISSING1","file_access":"check_file_info"}, row("F1"), "F2",
        {"id":"F1","name":"Last duplicate","permalink":"https://example.test/last"}
    ],"document_comment":{"opaque":"original"}})];
    let before = messages.clone();
    assert_eq!(first_file_id(&messages[0]), Ok(Some("FMISSING1")));
    let sidecar = embedded_files(&messages).unwrap();
    assert_eq!(sidecar.len(), 1);
    assert_eq!(sidecar[0]["name"], "Last duplicate");
    let mut store = Store::default();
    store.observe_batch(&sidecar).unwrap();
    assert!(
        store
            .get(first_file_id(&messages[0]).unwrap().unwrap())
            .is_none()
    );
    assert!(store.get("F1").unwrap().link().is_some());
    assert_eq!(messages, before);
    for raw in [json!({}), json!({"files":null}), json!({"files":[]})] {
        assert_eq!(first_file_id(&raw), Ok(None));
    }
    for first in [
        Value::Null,
        json!({}),
        json!({"id":""}),
        json!(""),
        json!(42),
        json!("F1"),
        json!("Fabcdef12"),
        json!("F123456789012345"),
    ] {
        assert!(first_file_id(&json!({"files":[first,"F1"]})).is_err());
    }
    assert_eq!(
        first_file_id(&json!({"files":["FFILE0001","F2"]})),
        Ok(Some("FFILE0001"))
    );
}

#[test]
fn replacement_invalidation_and_staged_clone_never_revive_old_observation() {
    let mut store = Store::default();
    store.observe_batch(&[row("F1"), row("F2")]).unwrap();
    let old = store.get("F1").unwrap();
    let unchanged = store.get("F2").unwrap();
    let mut staged = store.clone();
    staged.observe_batch(&[row("F1")]).unwrap();
    assert!(old.current(&store));
    assert!(!old.current(&staged));
    store = staged;
    assert!(unchanged.current(&store));
    let current = store.get("F1").unwrap();
    store.invalidate("F1").unwrap();
    assert!(!current.current(&store));
    store.observe_batch(&[row("F1")]).unwrap();
    assert!(!current.current(&store));
    let before = store.revision();
    store.invalidate("FABSENT").unwrap();
    assert!(store.revision() > before);
    assert!(store.get("FABSENT").is_none());
    assert!(!old.current(&Store::default()));
}

#[test]
fn malformed_and_oversized_batches_are_atomic_not_empty_observations() {
    let mut store = Store::default();
    store.observe_batch(&[row("F1")]).unwrap();
    let old = store.get("F1").unwrap();
    let revision = store.revision();
    for invalid in [
        Value::Null,
        json!({}),
        json!({"id":"F"}),
        json!({"id":"U1"}),
        json!({"id":"F 1"}),
    ] {
        assert!(
            store
                .observe_batch(&[json!({"id":"F1","name":"Must not land"}), invalid])
                .is_err()
        );
        assert_eq!(store.revision(), revision);
        assert!(old.current(&store));
    }
    assert_eq!(
        store.observe_batch(&vec![row("F2"); MAX_BATCH + 1]),
        Err(Error::Limit)
    );
    assert_eq!(
        store.observe_batch(&[json!({"id":"F1","opaque":"x".repeat(MAX_BATCH_BYTES)})]),
        Err(Error::Limit)
    );
    assert!(old.current(&store));
    assert_eq!(store.revision(), revision);
    store.observe_batch(&[]).unwrap();
    assert_eq!(store.revision(), revision);
}

#[test]
fn shared_batch_receipts_and_total_retention_are_bounded() {
    let mut store = Store::default();
    store.observe_batch(&[row("F1"), row("F2")]).unwrap();
    let one = store.get("F1").unwrap();
    let two = store.get("F2").unwrap();
    assert!(Arc::ptr_eq(&one.0.batch, &two.0.batch));
    let mut refused = false;
    for number in 10..22 {
        let raw = json!({"id":format!("F{number}"),"opaque":"x".repeat(1024*1024)});
        let revision = store.revision();
        if store.observe_batch(&[raw]).is_err() {
            refused = true;
            assert_eq!(store.revision(), revision);
            assert!(one.current(&store));
            assert!(store.get(&format!("F{number}")).is_none());
            break;
        }
    }
    assert!(refused);
    assert_eq!(
        embedded_files(&[json!({"files":vec!["FFILE0001";MAX_BATCH+1]})]),
        Err(Error::Limit)
    );
}
