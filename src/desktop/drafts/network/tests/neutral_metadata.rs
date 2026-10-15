use super::*;

fn neutral(raw: &mut Value) {
    raw["draft"]["date_stashed"] = json!(0);
    raw["draft"]["file_permissions"] = json!([]);
}

fn rejected_metadata() -> Vec<(&'static str, Value)> {
    vec![
        ("date_stashed", json!("0")),
        ("date_stashed", json!(0.0)),
        ("date_stashed", json!(1)),
        ("date_stashed", json!(-1)),
        ("date_stashed", Value::Null),
        ("date_stashed", json!(false)),
        ("date_stashed", json!([])),
        ("date_stashed", json!({})),
        ("file_permissions", json!({})),
        ("file_permissions", Value::Null),
        ("file_permissions", json!(false)),
        ("file_permissions", json!("[]")),
        ("file_permissions", json!([{"file_id":"F1","grant":false}])),
        ("file_permissions", json!([null])),
        ("mpdm_name", json!("")),
        ("mpdm_name", json!(" ")),
        ("mpdm_name", Value::Null),
    ]
}

fn readback(op: &Operation, binding: &Binding, raw: Value) -> Option<Readback> {
    match op.kind {
        OperationKind::Update => Some(update_readback::decode_update(op, &binding.workspace, raw)),
        OperationKind::Schedule { .. } => Some(schedule_readback::decode_schedule(
            op,
            &binding.workspace,
            raw,
        )),
        _ => None,
    }
}

#[tokio::test]
async fn neutral_fields_preserve_exact_ack_and_update_schedule_readback_evidence() {
    for kind in [
        OperationKind::Create,
        OperationKind::Update,
        OperationKind::Schedule {
            post_at: 1_800_001_000,
        },
    ] {
        let binding = binding();
        for thread in [false, true] {
            let mut op = operation(kind);
            if !thread {
                op.scope.key.thread = None;
                op.snapshot.as_mut().unwrap().destinations = vec![json!({"channel_id":"C1"})];
                if let Some(base) = op.base.as_mut() {
                    base.scope = op.scope.clone();
                    base.snapshot = op.snapshot.clone().unwrap();
                }
            }
            for fields in [
                json!({"date_stashed":0}),
                json!({"file_permissions":[]}),
                json!({"date_stashed":0,"file_permissions":[]}),
            ] {
                let mut raw = draft_response(&op);
                raw["draft"]
                    .as_object_mut()
                    .unwrap()
                    .extend(fields.as_object().unwrap().clone());
                raw["draft"]["is_from_composer"] =
                    json!(matches!(kind, OperationKind::Schedule { .. }));
                let result = binding
                    .execute_with(&op, 1000, |_| std::future::ready(Ok(raw.clone())))
                    .await;
                assert!(matches!(
                    result.outcome,
                    Outcome::Draft(_) | Outcome::Scheduled { .. }
                ));
                assert_eq!(result.raw, Some(raw.clone()));
                if let Some(result) = readback(&op, &binding, raw.clone()) {
                    assert!(result.outcome.is_some(), "{kind:?}");
                    assert_eq!(result.issue, None);
                    assert_eq!(result.raw, Some(raw));
                }
            }
        }
    }
}

#[tokio::test]
async fn nonneutral_metadata_remains_raw_uncertainty_in_ack_and_readback() {
    for kind in [
        OperationKind::Create,
        OperationKind::Update,
        OperationKind::Schedule {
            post_at: 1_800_001_000,
        },
    ] {
        let binding = binding();
        let op = operation(kind);
        for (field, value) in rejected_metadata() {
            let mut raw = draft_response(&op);
            neutral(&mut raw);
            raw["draft"][field] = value;
            let result = binding
                .execute_with(&op, 1000, |_| std::future::ready(Ok(raw.clone())))
                .await;
            assert!(
                matches!(result.outcome, Outcome::Uncertain),
                "{kind:?} {field}"
            );
            assert_eq!(result.raw, Some(raw.clone()));
            if let Some(result) = readback(&op, &binding, raw.clone()) {
                assert!(result.outcome.is_none(), "{kind:?} {field}");
                assert_eq!(result.raw, Some(raw));
            }
        }
    }
}

#[tokio::test]
async fn neutral_metadata_cannot_mask_identity_lifecycle_or_unknown_fields() {
    for kind in [
        OperationKind::Create,
        OperationKind::Update,
        OperationKind::Schedule {
            post_at: 1_800_001_000,
        },
    ] {
        let binding = binding();
        let op = operation(kind);
        let wrong_composer = !matches!(kind, OperationKind::Schedule { .. });
        for (field, value) in [
            (
                "client_msg_id",
                json!("00000000-0000-4000-8000-000000000099"),
            ),
            ("team_id", json!("T2")),
            ("user_id", json!("U2")),
            ("is_deleted", json!(true)),
            ("is_sent", json!(true)),
            ("is_from_composer", json!(wrong_composer)),
            ("is_from_composer", Value::Null),
            ("is_from_composer", json!("false")),
            ("is_deleted", json!(0)),
            ("date_scheduled", json!(-1)),
            ("file_ids", json!(["F1"])),
            ("destinations", json!([{"channel_id":"C2"}])),
            ("attachments", json!([{"text":"Opaque"}])),
            ("future_field", json!(false)),
        ] {
            let mut raw = draft_response(&op);
            neutral(&mut raw);
            raw["draft"][field] = value;
            let result = binding
                .execute_with(&op, 1000, |_| std::future::ready(Ok(raw.clone())))
                .await;
            assert!(
                matches!(result.outcome, Outcome::Uncertain),
                "{kind:?} {field}"
            );
            assert_eq!(result.raw, Some(raw.clone()));
            if let Some(result) = readback(&op, &binding, raw.clone()) {
                assert!(result.outcome.is_none(), "{kind:?} {field}");
                assert_eq!(result.raw, Some(raw));
            }
        }
    }
}
