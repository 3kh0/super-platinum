use super::*;

fn empty() -> History {
    History::initialize(StorageRead::Missing).unwrap()
}
fn entry() -> Entry {
    Entry::new(json!({"teamId":"T1","primaryView":"C12345678","ts":1234567}))
}
fn deferred(write: Write) -> DeferredWrite {
    match write {
        Write::Deferred(write) => write,
        other => panic!("{other:?}"),
    }
}

#[test]
fn actual_l24d_projection_vectors() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("cases.json")).unwrap();
    assert_eq!(cases.len(), 35);
    for case in cases {
        let mut history = History::initialize(StorageRead::Parsed(case["rows"].clone())).unwrap();
        let projected = history
            .project(
                case["sonic"].as_bool().unwrap(),
                case["team"].as_str(),
                case["multi"].as_bool().unwrap(),
            )
            .unwrap();
        let expected = case["expected"].as_object().unwrap();
        assert_eq!(projected.len(), expected.len(), "{}", case["label"]);
        for (id, want) in expected {
            let got = projected.get(id).unwrap();
            if let Some(ts) = want["ts"].as_f64() {
                assert_eq!(got.ts, ts, "{}", case["label"]);
            } else {
                assert!(!got.ts.is_finite(), "{}", case["label"]);
            }
            assert_eq!(got.position as u64, want["position"].as_u64().unwrap());
        }
        assert_eq!(history.snapshot(), case["after"], "{}", case["label"]);
    }
}

#[test]
fn exact_identity_alias_positions_and_deferred_write_custody() {
    let mut history = empty();
    let a = entry();
    let same_value = entry();
    let first = deferred(history.push(Some(a.clone()), &[]));
    history.push(Some(same_value.clone()), &[]);
    history.push(Some(a.clone()), &[]);
    assert!(matches!(history.remove(&entry()), Write::None));
    assert_eq!(
        history.project(true, Some("T1"), false).unwrap()["C12345678"].position,
        3
    );
    // Timer closes over a shallow array; it sees the subsequent read mutation.
    assert_eq!(first.snapshot_when_run()[0]["position"], 3);
    history.remove(&a);
    assert!(history.entries()[0].same_object(&same_value));
    assert!(history.entries()[1].same_object(&a));
    let keep_first = deferred(history.remove_all());
    assert_eq!(history.entries().len(), 1);
    assert!(matches!(history.delete_stack(),Write::Immediate(v) if v == json!([])));
    // Synchronous clear does not revoke older queued source timers.
    assert_eq!(keep_first.snapshot_when_run().as_array().unwrap().len(), 1);
    let restored = History::initialize(StorageRead::Parsed(json!([a.raw(), a.raw()]))).unwrap();
    assert!(!restored.entries()[0].same_object(&restored.entries()[1]));
}

#[test]
fn initializer_limits_skip_and_partial_failure_are_distinct() {
    for read in [
        StorageRead::Missing,
        StorageRead::ReadFailed,
        StorageRead::InvalidJson,
    ] {
        assert_eq!(History::initialize(read).unwrap().snapshot(), json!([]));
    }
    assert!(matches!(
        History::initialize(StorageRead::Parsed(Value::Null)),
        Err(Error::NonArrayStorage(Value::Null))
    ));
    let mut history = empty();
    assert!(matches!(history.push(None, &[]), Write::None));
    assert!(matches!(
        history.push(Some(Entry::new(Value::Null)), &[]),
        Write::None
    ));
    assert!(matches!(
        history.push(Some(entry()), &[json!("C12345678")]),
        Write::None
    ));
    for i in 0..1002 {
        history.push(Some(Entry::new(json!({"ts":i}))), &[]);
    }
    assert_eq!(history.entries().len(), 1000);
    assert_eq!(history.snapshot()[999]["ts"], 2);
    let mut primitive =
        History::initialize(StorageRead::Parsed(json!([entry().raw(), null]))).unwrap();
    assert!(
        primitive
            .project(false, Some("T1"), false)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        primitive.project(true, Some("T1"), false),
        Err(Error::PrimitiveEntry { index: 1 })
    );
    assert_eq!(primitive.snapshot()[0]["position"], 1);
    // indexOf uses strict primitive equality, while objects use identity.
    assert!(matches!(
        primitive.remove(&Entry::new(Value::Null)),
        Write::Deferred(_)
    ));
}

#[test]
fn history_handles_and_write_jobs_are_send_sync() {
    fn require<T: Send + Sync>() {}
    require::<History>();
    require::<Entry>();
    require::<Write>();
}

#[test]
fn actual_source_coercion_errors_keep_phase_and_mutated_positions() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("coercion-cases.json")).unwrap();
    assert_eq!(cases.len(), 21);
    for case in cases {
        let mut history = History::initialize(StorageRead::Parsed(case["rows"].clone())).unwrap();
        let actual = history.project(
            case["sonic"].as_bool().unwrap(),
            case["team"].as_str(),
            case["multi"].as_bool().unwrap(),
        );
        if case["error"].is_null() {
            let actual = actual.unwrap();
            let expected = case["expected"].as_object().unwrap();
            assert_eq!(actual.len(), expected.len(), "{}", case["label"]);
            for (id, want) in expected {
                let got = actual.get(id).unwrap();
                if let Some(ts) = want["ts"].as_f64() {
                    assert_eq!(got.ts, ts);
                } else {
                    assert!(!got.ts.is_finite());
                }
                assert_eq!(got.position as u64, want["position"].as_u64().unwrap());
            }
        } else {
            let field = match case["error"]["field"].as_str().unwrap() {
                "primaryView" => CoercionField::PrimaryView,
                "ts" => CoercionField::Timestamp,
                other => panic!("unexpected field {other}"),
            };
            assert_eq!(
                actual.unwrap_err(),
                Error::Coercion {
                    index: case["error"]["index"].as_u64().unwrap() as usize,
                    field
                },
                "{}",
                case["label"]
            );
        }
        assert_eq!(history.snapshot(), case["after"], "{}", case["label"]);
    }
}

#[test]
fn coercion_error_does_not_roll_back_entry_positions_or_queued_write() {
    let mut history = empty();
    let write = deferred(history.push(
        Some(Entry::new(
            json!({"teamId":"T1","primaryView":"C12345678","ts":{"toString":0}}),
        )),
        &[],
    ));
    assert_eq!(
        history.project(true, Some("T1"), false),
        Err(Error::Coercion {
            index: 0,
            field: CoercionField::Timestamp
        })
    );
    assert_eq!(write.snapshot_when_run()[0]["position"], 1);
}
