use super::*;
use crate::slack::api::custom::texty::file_titles::{self, CanvasReferenceText, Kind, Status};
use serde_json::json;

const ID: &str = "FSTATUS0001";
fn decode(raw: &Value) -> Result<FileReferenceResponse, Error> {
    FileReferenceResponse::decode(ID, &serde_json::to_vec(raw).unwrap())
}
fn positive(row: Value) -> FileReferenceResponse {
    decode(&json!({"ok":true,"file":row})).unwrap()
}
fn deleted() -> FileReferenceResponse {
    decode(&json!({"ok":false,"error":"file_deleted"})).unwrap()
}

#[test]
fn original_eight_vectors_bind_deleted_mapping_recovery_and_excluded_controls() {
    let vectors: Value = serde_json::from_str(include_str!("original-deleted.json")).unwrap();
    let cases = vectors["vectors"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    let mut deleted_stages = 0;
    let mut ordinary_stages = 0;
    let mut refused = 0;
    for case in cases {
        let mut store = Store::new();
        let mut previous = if case["initial"].is_object() {
            Some(
                store
                    .ingest_reference("T1", &positive(case["initial"].clone()))
                    .unwrap(),
            )
        } else {
            None
        };
        let Some(stages) = case["stages"].as_array() else {
            // Original no-fetch display is not a source receipt or acquisition.
            assert!(store.reference(ID).is_err());
            assert!(decode(&Value::Null).is_err());
            refused += 1;
            continue;
        };
        for stage in stages {
            let raw = if stage["input"]["envelope"].is_object() {
                stage["input"]["envelope"].clone()
            } else {
                let mut raw = stage["input"]["error"].clone();
                raw["ok"] = json!(false);
                raw
            };
            let response = decode(&raw);
            if raw["ok"] == false && raw["error"] != "file_deleted" {
                assert!(response.is_err(), "{}", case["name"]);
                if let Some(previous) = &previous {
                    assert!(previous.current(&store));
                } else {
                    assert!(store.reference(ID).is_err());
                }
                refused += 1;
                continue;
            }
            let response = response.unwrap();
            assert_eq!(response.raw(), &raw);
            assert_eq!(response.source(), &stage["mapped"]);
            let observation = store.ingest_reference("T1", &response).unwrap();
            if let Some(previous) = previous.take() {
                assert!(!previous.current(&store));
            }
            assert!(observation.current(&store));
            assert_eq!(observation.receipt(), &raw);
            for key in FIELDS {
                assert_eq!(
                    observation.effective().get(*key),
                    stage["row"].get(*key),
                    "{} {key}",
                    case["name"]
                );
            }
            for key in stage["rowOwnUndefinedKeys"].as_array().unwrap() {
                assert!(observation.effective().get(key.as_str().unwrap()).is_none());
            }
            match observation.value() {
                ReferenceValue::DeletedCanvas => {
                    assert_eq!(
                        stage["slug"]["label"],
                        CanvasReferenceText::english().deleted_canvas()
                    );
                    assert_eq!(stage["slug"]["iconName"], "trash");
                    assert_eq!(stage["slug"]["isDisabled"], true);
                    assert_eq!(stage["slug"]["isLocked"], false);
                    deleted_stages += 1;
                }
                ReferenceValue::Normal(row) => {
                    assert_eq!(
                        file_titles::label(Kind::Canvas, Status::Normal, row).unwrap(),
                        stage["slug"]["label"].as_str().unwrap()
                    );
                    assert_eq!(row["is_deleted"], false);
                    ordinary_stages += 1;
                }
                ReferenceValue::PrivateCanvas => {
                    panic!("deleted history retained old private access")
                }
            }
            assert!(store.row(ID).is_err()); // legacy title poison is unchanged
            previous = Some(observation);
        }
    }
    assert_eq!((deleted_stages, ordinary_stages, refused), (4, 3, 5));
}

#[test]
fn only_exact_error_receipt_originates_deleted_authority_and_retires_old_observation() {
    for raw in [
        json!({"id":ID,"mode":"tombstone","is_deleted":true}),
        json!({"id":ID,"mode":"quip","is_deleted":true}),
        json!({"id":ID,"isNonExistent":true}),
    ] {
        let mut store = Store::new();
        assert!(store.ingest(&raw).is_err());
        assert!(store.reference(ID).is_err());
        // Supported incoming producer bypasses unrelated predecessor poison.
        let response = deleted();
        let observed = store.ingest_reference("T1", &response).unwrap();
        assert_eq!(observed.value(), ReferenceValue::DeletedCanvas);
        assert_eq!(
            observed.effective(),
            &json!({"id":ID,"mode":"tombstone","is_deleted":true})
        );
        assert!(store.row(ID).is_err());
        let next = store.ingest_reference("T1", &deleted()).unwrap();
        assert!(!observed.current(&store));
        assert!(next.current(&store));
        assert!(!next.current(&Store::new()));
    }
    let mut store = Store::new();
    let raw = json!({"id":ID,"mode":"quip","title":"Normal","is_deleted":true});
    let observed = store
        .ingest_reference("T1", &positive(raw.clone()))
        .unwrap();
    assert_eq!(observed.source(), &raw);
    assert!(matches!(observed.value(), ReferenceValue::Normal(_)));
    assert_eq!(observed.effective()["is_deleted"], false);
    let malformed = json!({"ok":true,"error":"file_deleted","file":raw});
    assert!(decode(&malformed).is_err());
    assert!(observed.current(&store));
    assert!(
        FileReferenceResponse::decode("invalid", br#"{"ok":false,"error":"file_deleted"}"#)
            .is_err()
    );
}

#[test]
fn deleted_receipt_ignores_error_metadata_and_partial_history_cannot_smuggle_other_statuses() {
    let raw = json!({"ok":false,"error":"file_deleted","filetype":"quip","title":"Private secret","file":{"id":"FOTHER"},"opaque":{"keep":1}});
    let response = decode(&raw).unwrap();
    assert_eq!(response.raw(), &raw);
    assert_eq!(
        response.source(),
        &json!({"id":ID,"mode":"tombstone","is_deleted":true})
    );
    for patch in [
        json!({"id":ID,"updated":2,"is_tombstoned":true}),
        json!({"id":ID,"isNonExistent":true}),
        json!({"id":ID,"updated":2,"restriction_type":1}),
        json!({"id":ID,"updated":2,"is_external":true}),
        json!({"id":ID,"updated":2,"canvas_template_mode":true}),
        json!({"id":ID,"updated":2,"title":42}),
    ] {
        let mut store = Store::new();
        let observed = store.ingest_reference("T1", &deleted()).unwrap();
        assert!(store.ingest_reference("T1", &positive(patch)).is_err());
        assert!(!observed.current(&store));
        assert!(store.reference(ID).is_err());
    }
}
