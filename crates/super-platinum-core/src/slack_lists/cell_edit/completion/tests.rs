use super::*;
use serde_json::json;
fn policy() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[\"F_MY\"]","assigned_to_list":"F_ASSIGNED"}}),
    )
    .unwrap()
}
fn document(id: &str) -> Document {
    Document::parse(
        json!({"id":id,"mode":"list","access":"write","list_metadata":{"schema":[
    {"id":"C_DONE","key":"todo_completed","name":"Done","type":"todo_completed"},
    {"id":"C_REF","key":"ref","name":"Source","type":"reference"},
    {"id":"C_OTHER_REF","key":"second","name":"Other source","type":"reference"}
],"views":[{"id":"V","columns":["C_DONE"]}]}}),
    )
    .unwrap()
}
fn page(checked: Option<Value>, reference: Option<Value>) -> RecordsPage {
    let mut fields = vec![];
    if let Some(value) = checked {
        fields.push(json!({"key":"todo_completed","checkbox":value}));
    }
    if let Some(reference) = reference {
        fields.push(json!({"key":"ref","reference":reference}));
    }
    fields.push(json!({"key":"second","reference":[{"list_record":{"list_id":"F_IGNORED","row_id":"IGNORED"}}]}));
    RecordsPage::parse(json!({"ok":true,"records":[{"id":"ROW","date_created":10,"created_by":"U00000001","fields":fields}],"response_metadata":{"next_cursor":"retain"}})).unwrap()
}
fn capture(reference: Option<Value>) -> TaskCompletion {
    TaskCompletion::capture(
        &document("F_MY"),
        &page(Some(json!(false)), reference),
        "ROW",
        "C_DONE",
        Some("full"),
        &policy(),
        true,
    )
    .unwrap()
}
#[test]
fn personal_message_and_unmirrored_reference_complete_only_personal_row() {
    for reference in [
        None,
        Some(json!([])),
        Some(json!([{"message":{"channel_id":"C_MSG","ts":"1.0"}}])),
        Some(json!([{"file":{"file_id":"F_FILE"}}])),
        Some(json!([{"canvas_section":{"file_id":"F_CANVAS","section_id":"S"}}])),
    ] {
        let edit = capture(reference);
        assert!(edit.reference().is_none());
        assert_eq!(
            edit.primary().edit().cells(),
            json!([{"row_id":"ROW","column_id":"C_DONE","checkbox":true}])
        );
        assert!(edit.primary().needs_write());
    }
}
#[test]
fn first_schema_reference_and_first_array_value_match_retained_callback() {
    let edit = capture(Some(
        json!([{"list_record":{"list_id":"F_SOURCE","row_id":"ROW"}},
        {"list_record":{"list_id":"F_NOT_SELECTED","row_id":"OTHER"}}]),
    ));
    assert_eq!(
        edit.reference(),
        Some(&TaskCompletionReference {
            list_id: "F_SOURCE".into(),
            record_id: "ROW".into()
        })
    );
    let target = document("F_SOURCE");
    let rows = page(Some(json!(false)), None);
    let second = edit
        .capture_reference(&target, &rows, Some("full"), &policy())
        .unwrap();
    assert_eq!(second.edit().list_id(), "F_SOURCE");
    assert_eq!(second.edit().record_id(), "ROW");
    assert!(second.needs_write());
    assert!(
        edit.capture_reference(&document("F_OTHER"), &rows, Some("full"), &policy())
            .is_err()
    );
    assert!(
        edit.capture_reference(&target, &rows, None, &policy())
            .is_err()
    );
}
#[test]
fn generic_boolean_path_stays_closed_and_personal_identity_is_required() {
    let d = document("F_MY");
    let p = page(Some(json!(false)), None);
    assert!(
        CellEdit::capture(
            &d,
            &p,
            "ROW",
            "C_DONE",
            Some("full"),
            Some(&policy()),
            CellEditValue::Boolean(true)
        )
        .is_err()
    );
    assert!(
        TaskCompletion::capture(
            &document("F_OTHER"),
            &p,
            "ROW",
            "C_DONE",
            Some("full"),
            &policy(),
            true
        )
        .is_err()
    );
    assert!(
        TaskCompletion::capture(&d, &p, "ROW", "C_REF", Some("full"), &policy(), true).is_err()
    );
    assert!(
        TaskCompletion::capture(&d, &p, "ROW", "C_DONE", Some("full"), &policy(), false).is_err()
    );
}
#[test]
fn missing_and_null_completion_are_source_false_not_malformed_boolean_values() {
    let d = document("F_MY");
    for raw in [None, Some(json!(null)), Some(json!(false))] {
        let p = page(raw, None);
        let edit = TaskCompletion::capture(&d, &p, "ROW", "C_DONE", Some("full"), &policy(), true)
            .unwrap();
        assert_eq!(edit.primary().readback_value(&d, &p), Some(false));
        assert!(
            edit.primary()
                .revalidate(&d, &p, Some("full"), &policy())
                .is_ok()
        );
    }
    for raw in [json!(0), json!("false"), json!({}), json!([])] {
        assert!(
            TaskCompletion::capture(
                &d,
                &page(Some(raw), None),
                "ROW",
                "C_DONE",
                Some("full"),
                &policy(),
                true
            )
            .is_err()
        );
    }
}
#[test]
fn mirror_already_desired_is_observation_without_another_write_and_self_reference_collapses() {
    let edit = capture(Some(
        json!([{"list_record":{"list_id":"F_SOURCE","row_id":"ROW"}}]),
    ));
    let target = document("F_SOURCE");
    let p = page(Some(json!(true)), None);
    let mirror = edit
        .capture_reference(&target, &p, Some("full"), &policy())
        .unwrap();
    assert!(!mirror.needs_write());
    assert!(mirror.readback_matches(&target, &p));
    assert!(
        capture(Some(
            json!([{"list_record":{"list_id":"F_MY","row_id":"ROW"}}])
        ))
        .reference()
        .is_none()
    );
}
#[test]
fn malformed_ambiguous_reference_refuses_and_full_raw_custody_guards_both_steps() {
    for r in [
        json!({}),
        json!([false]),
        json!([{"list_record":{}}]),
        json!([{"list_record":{"list_id":"F_SOURCE","row_id":"ROW"},"message":{"channel_id":"C","ts":"1"}}]),
    ] {
        assert!(
            TaskCompletion::capture(
                &document("F_MY"),
                &page(Some(json!(false)), Some(r)),
                "ROW",
                "C_DONE",
                Some("full"),
                &policy(),
                true
            )
            .is_err()
        );
    }
    let reference = Some(json!([{"list_record":{"list_id":"F_SOURCE","row_id":"ROW"}}]));
    let p = page(Some(json!(false)), reference.clone());
    let d = document("F_MY");
    let edit = capture(reference);
    let mut changed = p.clone();
    changed.records[0].raw["future"] = json!({"retain":true});
    assert!(
        edit.primary()
            .revalidate(&d, &changed, Some("full"), &policy())
            .is_err()
    );
    let mut replaced = page(Some(json!(true)), None);
    replaced.records[0].raw["date_created"] = json!(11);
    assert!(!edit.primary().readback_matches(&d, &replaced));
    assert_eq!(p.raw["response_metadata"]["next_cursor"], "retain");
}
#[test]
fn actual_existing_cell_builder_receives_distinct_personal_and_referenced_forms() {
    use crate::{
        config::WorkspaceSession,
        slack::{
            api,
            client::{RequestBody, SlackClient},
        },
    };
    let edit = capture(Some(
        json!([{"list_record":{"list_id":"F_SOURCE","row_id":"ROW"}}]),
    ));
    let mirror = edit
        .capture_reference(
            &document("F_SOURCE"),
            &page(Some(json!(false)), None),
            Some("full"),
            &policy(),
        )
        .unwrap();
    let workspace = WorkspaceSession {
        team_id: "T_TEST".into(),
        enterprise_id: None,
        user_id: "U_TEST".into(),
        name: "Test".into(),
        url: "https://example.slack.com".into(),
        token: "not-a-token".into(),
    };
    for (cell, list) in [(edit.primary(), "F_MY"), (&mirror, "F_SOURCE")] {
        let request = api::list_cell_update(&SlackClient::default(), &workspace, cell.edit());
        let RequestBody::Form(form) = request.body else {
            panic!("form")
        };
        assert_eq!(
            form,
            vec![
                ("token".into(), "not-a-token".into()),
                ("list_id".into(), list.into()),
                (
                    "cells".into(),
                    json!([{"row_id":"ROW","column_id":"C_DONE","checkbox":true}]).to_string()
                ),
                ("_x_reason".into(), "bulk-update-cells".into())
            ]
        );
    }
}

#[test]
fn confirmed_primary_does_not_retarget_mirror_after_reference_or_row_replacement() {
    let d = document("F_MY");
    let reference = json!([{"list_record":{"list_id":"F_SOURCE","row_id":"ROW"}}]);
    let intent = capture(Some(reference.clone()));
    let mut observed = page(Some(json!(true)), Some(reference));
    observed.records[0].raw["date_updated"] = json!(20);
    assert!(intent.reference_still_current(&d, &observed));
    for change in 0..4 {
        let mut raw = observed.raw.clone();
        match change {
            0 => {
                raw["records"][0]["fields"][1]["reference"][0]["list_record"]["row_id"] =
                    json!("REPLACED")
            }
            1 => {
                raw["records"][0]["fields"][1]["future"] = json!("same target, changed observation")
            }
            2 => raw["records"][0]["date_created"] = json!(11),
            _ => raw["records"][0]["fields"][0]["checkbox"] = json!(false),
        }
        let changed = RecordsPage::parse(raw).unwrap();
        assert!(!intent.reference_still_current(&d, &changed));
    }
    let mut raw = d.raw.clone();
    raw["list_metadata"]["views"][0]["name"] = json!("changed metadata");
    assert!(!intent.reference_still_current(&Document::parse(raw).unwrap(), &observed));
}

#[test]
fn explicit_foreign_list_identity_never_confirms_primary_or_mirror() {
    let intent = capture(Some(
        json!([{"list_record":{"list_id":"F_SOURCE","row_id":"ROW"}}]),
    ));
    let primary_doc = document("F_MY");
    let reference = Some(json!([{"list_record":{"list_id":"F_SOURCE","row_id":"ROW"}}]));
    let mirror_doc = document("F_SOURCE");
    let mirror = intent
        .capture_reference(
            &mirror_doc,
            &page(Some(json!(false)), None),
            Some("full"),
            &policy(),
        )
        .unwrap();
    for bad in [json!("F_FOREIGN"), json!(null), json!(3)] {
        let mut primary = page(Some(json!(true)), reference.clone());
        primary.records[0].raw["list_id"] = bad.clone();
        assert_eq!(
            intent.primary().readback_value(&primary_doc, &primary),
            None
        );
        assert!(!intent.primary().readback_matches(&primary_doc, &primary));
        assert!(!intent.reference_still_current(&primary_doc, &primary));
        let mut target = page(Some(json!(true)), None);
        target.records[0].raw["list_id"] = bad;
        assert_eq!(mirror.readback_value(&mirror_doc, &target), None);
        assert!(!mirror.readback_matches(&mirror_doc, &target));
    }
    let mut primary = page(Some(json!(true)), reference);
    primary.records[0].raw["list_id"] = json!("F_MY");
    assert!(intent.reference_still_current(&primary_doc, &primary));
    let mut target = page(Some(json!(true)), None);
    target.records[0].raw["list_id"] = json!("F_SOURCE");
    assert!(mirror.readback_matches(&mirror_doc, &target));
}
