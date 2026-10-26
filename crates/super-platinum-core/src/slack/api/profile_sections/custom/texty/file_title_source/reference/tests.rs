use super::*;
use crate::slack::api::custom::texty::file_titles::{self, CanvasReferenceText, Kind, Status};
use serde_json::json;

fn decode(id: &str, raw: Value) -> FileReferenceResponse {
    FileReferenceResponse::decode(id, &serde_json::to_vec(&raw).unwrap()).unwrap()
}
fn positive(file: Value) -> FileReferenceResponse {
    let id = file["id"].as_str().unwrap().to_owned();
    decode(&id, json!({"ok":true,"file":file}))
}
fn known() -> Value {
    json!({"id":"F1","mode":"quip","title":"Known","updated":1})
}
fn status(code: &str) -> FileReferenceResponse {
    decode("F1", json!({"ok":false,"error":code}))
}

#[test]
fn retained_twenty_two_status_vectors_bound_only_the_two_supported_categories() {
    let vectors: Value = serde_json::from_str(include_str!("original-status.json")).unwrap();
    let cases = vectors["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 22);
    let mut admitted = 0;
    for case in cases {
        let mut store = Store::new();
        let _ = store.ingest(&case["row"]);
        let result = store.reference("FSTATUS0001");
        match case["name"].as_str().unwrap() {
            "not-visible-mode"
            | "not-visible-access"
            | "template-mode"
            | "template-access"
            | "minimal-not_visible"
            | "minimal-template_not_visible" => {
                let observed = result.unwrap();
                assert_eq!(observed.value(), ReferenceValue::PrivateCanvas);
                assert_eq!(observed.effective(), &case["row"]);
                let slug = &case["slug"];
                assert_eq!(
                    slug["label"],
                    CanvasReferenceText::english().private_canvas()
                );
                assert_eq!(slug["isDisabled"], false);
                assert_eq!(slug["isLocked"], true);
                admitted += 1;
            }
            "normal" | "locked-flag" => {
                assert!(matches!(result.unwrap().value(), ReferenceValue::Normal(_)));
            }
            _ => assert!(result.is_err(), "{}", case["name"]),
        }
    }
    assert_eq!(admitted, 6);
}

#[test]
fn original_three_producer_processor_histories_recover_or_retain_access_without_reset() {
    let vectors: Value = serde_json::from_str(include_str!("original-transitions.json")).unwrap();
    assert_eq!(vectors["cases"].as_array().unwrap().len(), 3);
    for case in vectors["cases"].as_array().unwrap() {
        let mut store = Store::new();
        let initial = store
            .ingest_reference("T1", &positive(case["initial"]["mapped"].clone()))
            .unwrap();
        let mut error = case["errorData"].clone();
        error["ok"] = json!(false);
        let response = decode("FSTATUS0001", error.clone());
        assert_eq!(response.source(), &case["unavailable"]["mapped"]);
        let hidden = store.ingest_reference("T1", &response).unwrap();
        assert!(!initial.current(&store));
        assert_eq!(hidden.receipt(), &error);
        assert_eq!(hidden.effective(), &case["unavailable"]["row"]);
        assert_eq!(hidden.value(), ReferenceValue::PrivateCanvas);
        let recovered = store
            .ingest_reference("T1", &positive(case["ordinary"]["mapped"].clone()))
            .unwrap();
        assert!(!hidden.current(&store));
        for key in FIELDS {
            assert_eq!(
                recovered.effective().get(*key),
                case["ordinary"]["row"].get(*key),
                "{} {key}",
                case["name"]
            );
        }
        let label = match recovered.value() {
            ReferenceValue::PrivateCanvas => {
                CanvasReferenceText::english().private_canvas().to_owned()
            }
            ReferenceValue::Normal(row) => {
                file_titles::label(Kind::Canvas, Status::Normal, row).unwrap()
            }
            ReferenceValue::DeletedCanvas => panic!("private lifecycle unexpectedly deleted"),
        };
        assert_eq!(label, case["ordinary"]["label"].as_str().unwrap());
        // This new narrow observation does not heal legacy normal-title poison.
        assert!(store.row("FSTATUS0001").is_err());
    }
}

#[test]
fn full_error_carrier_is_retained_but_only_exact_producers_are_admitted() {
    let raw = json!({"ok":false,"error":"not_visible","filetype":"quip","title":"secret","opaque":{"keep":[1,2]}});
    let response = decode("F1", raw.clone());
    assert_eq!(response.raw(), &raw);
    assert_eq!(
        response.source(),
        &json!({"id":"F1","mode":"quip","title":"secret","file_access":"not_visible"})
    );
    for raw in [
        json!({"ok":false,"error":"access_denied"}),
        json!({"ok":false,"error":"file_not_found"}),
        json!({"ok":true,"error":"not_visible","file":known()}),
        json!({"ok":true,"file":{"id":"F2","mode":"quip"}}),
        json!({"error":"not_visible"}),
        json!({"ok":"false","error":"not_visible"}),
        Value::Null,
    ] {
        assert!(FileReferenceResponse::decode("F1", &serde_json::to_vec(&raw).unwrap()).is_err());
    }
    assert!(FileReferenceResponse::decode("G1", br#"{"ok":false,"error":"not_visible"}"#).is_err());
    assert!(FileReferenceResponse::decode("F1", b"{invalid").is_err());
}

#[test]
fn newly_committed_receipt_and_current_row_are_required_not_old_category_after_error() {
    let mut store = Store::new();
    let first = store
        .ingest_reference("T1", &status("not_visible"))
        .unwrap();
    assert!(
        store
            .ingest(&json!({"ok":false,"error":"malformed"}))
            .is_err()
    );
    assert!(first.current(&store)); // malformed did not create a new receipt
    assert!(
        store
            .ingest_reference("", &status("template_not_visible"))
            .is_err()
    );
    assert!(first.current(&store));
    let unrelated = positive(json!({"id":"F2","mode":"quip","title":"Other"}));
    store.ingest_reference("T1", &unrelated).unwrap();
    assert!(first.current(&store));
    let mut other = Store::new();
    other
        .ingest_reference("T1", &status("not_visible"))
        .unwrap();
    assert!(!first.current(&other));
    let changed = store
        .ingest_reference("T1", &status("not_visible"))
        .unwrap();
    assert!(!first.current(&store));
    assert!(changed.current(&store));
}

#[test]
fn ordinary_response_cannot_heal_poison_but_exact_private_receipt_is_independent() {
    for field in [
        "is_tombstoned",
        "is_hidden_by_limit",
        "canvas_template_mode",
        "is_external",
    ] {
        let mut store = Store::new();
        let original = store.ingest_reference("T1", &positive(known())).unwrap();
        let mut bad = known();
        bad[field] = json!(true);
        assert!(store.ingest_reference("T1", &positive(bad)).is_err());
        assert!(!original.current(&store));
        assert!(store.ingest_reference("T1", &positive(known())).is_err());
        for code in ["not_visible", "template_not_visible"] {
            let mut branch = store.clone();
            let fresh = branch.ingest_reference("T1", &status(code)).unwrap();
            assert_eq!(fresh.value(), ReferenceValue::PrivateCanvas);
            assert!(fresh.current(&branch));
            assert!(!original.current(&branch));
            assert!(branch.row("F1").is_err());
            assert!(branch.candidate("F1").is_err());
        }
    }
}

#[test]
fn original_supplement_binds_independent_status_and_no_kind_partial_merge() {
    let vectors: Value = serde_json::from_str(include_str!("original-supplement.json")).unwrap();
    let cases = vectors["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3);
    for case in cases {
        let mut store = Store::new();
        let mut previous: Option<ReferenceObservation> = None;
        for stage in case["stages"].as_array().unwrap() {
            if stage["input"]["error"]["error"] == "file_deleted" {
                // The exact producer is now admitted through its receipt;
                // the same mapped bytes through the generic raw API are not.
                let mut response = stage["input"]["error"].clone();
                response["ok"] = json!(false);
                let response = decode("FSTATUS0001", response);
                assert_eq!(response.source(), &stage["mapped"]);
                let observed = store.ingest_reference("T1", &response).unwrap();
                assert_eq!(observed.effective(), &stage["row"]);
                assert_eq!(observed.value(), ReferenceValue::DeletedCanvas);
                assert_eq!(
                    stage["slug"]["label"],
                    CanvasReferenceText::english().deleted_canvas()
                );
                previous = Some(observed);
                continue;
            }
            let raw = if let Some(envelope) = stage["input"].get("envelope") {
                envelope.clone()
            } else {
                let mut error = stage["input"]["error"].clone();
                error["ok"] = json!(false);
                error
            };
            let response = decode("FSTATUS0001", raw.clone());
            assert_eq!(response.source(), &stage["mapped"]);
            let observed = store.ingest_reference("T1", &response).unwrap();
            if let Some(previous) = previous.take() {
                assert!(!previous.current(&store));
            }
            assert_eq!(observed.receipt(), &raw);
            assert_eq!(observed.value(), ReferenceValue::PrivateCanvas);
            for key in FIELDS {
                assert_eq!(
                    observed.effective().get(*key),
                    stage["row"].get(*key),
                    "{} {key}",
                    case["name"]
                );
            }
            for key in stage["rowOwnUndefinedKeys"].as_array().unwrap() {
                assert!(observed.effective().get(key.as_str().unwrap()).is_none());
            }
            assert_eq!(
                stage["slug"]["label"],
                CanvasReferenceText::english().private_canvas()
            );
            assert_eq!(stage["slug"]["isDisabled"], false);
            assert_eq!(stage["slug"]["isLocked"], true);
            previous = Some(observed);
        }
    }
}

#[test]
fn inherited_private_status_does_not_override_new_deleted_or_unsupported_facts() {
    for patch in [
        json!({"id":"F1","updated":2,"is_deleted":true}),
        json!({"id":"F1","updated":2,"mode":"tombstone"}),
        json!({"id":"F1","updated":2,"is_tombstoned":true}),
        json!({"id":"F1","updated":2,"restriction_type":1}),
    ] {
        let mut store = Store::new();
        let private = store
            .ingest_reference("T1", &status("not_visible"))
            .unwrap();
        assert!(store.ingest(&patch).is_err());
        assert!(!private.current(&store));
        assert!(store.reference("F1").is_err());
    }
}

#[test]
fn positive_deleted_mapping_whole_list_and_status_dependency_bypass_preserve_boundaries() {
    let mut store = Store::new();
    let mut file = known();
    file["is_deleted"] = json!(true);
    let observed = store
        .ingest_reference("T1", &positive(file.clone()))
        .unwrap();
    assert_eq!(observed.source(), &file);
    assert_eq!(observed.effective()["is_deleted"], false);
    let list = positive(json!({"id":"F2","mode":"list","title":"Plan &amp; tasks"}));
    let list = store.ingest_reference("T1", &list).unwrap();
    assert_eq!(
        file_titles::label(Kind::WholeList, Status::Normal, list.effective()).unwrap(),
        "Plan & tasks"
    );
    let row = json!({"id":"F3","mode":"not_visible","title_blocks":{"malformed":"never read"},"title":42});
    let hidden = store.ingest_reference("T1", &positive(row)).unwrap();
    assert_eq!(hidden.value(), ReferenceValue::PrivateCanvas);
    // Generic Unavailable callers remain conservative without typed provenance.
    assert!(file_titles::label(Kind::Canvas, Status::Unavailable, hidden.effective()).is_err());
}

#[test]
fn legacy_raw_and_batch_updates_retire_reference_observations_too() {
    let mut store = Store::new();
    let first = store.ingest_reference("T1", &positive(known())).unwrap();
    store
        .ingest(&json!({"id":"F1","title":"Raw rename"}))
        .unwrap();
    assert!(!first.current(&store));
    assert_eq!(
        store.reference("F1").unwrap().effective()["title"],
        "Raw rename"
    );
    let raw = store.reference("F1").unwrap();
    let owner = crate::config::WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    let rows = vec![json!({"id":"F1","mode":"template_not_visible"})];
    let carrier = json!({"ok":true,"files":rows});
    store.ingest_batch(&carrier, &rows, &owner).unwrap();
    assert!(!raw.current(&store));
    assert_eq!(
        store.reference("F1").unwrap().value(),
        ReferenceValue::PrivateCanvas
    );
    let hidden = store.reference("F1").unwrap();
    let rows = vec![known()];
    store
        .ingest_batch(&json!({"ok":true,"files":rows}), &rows, &owner)
        .unwrap();
    assert!(!hidden.current(&store));
    assert!(matches!(
        store.reference("F1").unwrap().value(),
        ReferenceValue::Normal(_)
    ));
}
