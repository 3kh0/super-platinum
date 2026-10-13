use super::*;
use crate::config::WorkspaceSession;
use crate::slack::{
    api,
    client::{RequestBody, SlackClient},
};

fn policy(id: &str) -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":id}}),
    )
    .unwrap()
}
fn fixture(kind: &str, field: Option<Value>) -> (Document, RecordsPage) {
    let document = Document::parse(
        json!({"id":"F_DATE","mode":"list","access":"write","list_metadata":{"schema":[
            {"id":"COL_DATE","key":"due","name":"Due date","type":kind}
        ]}}),
    )
    .unwrap();
    let fields: Vec<_> = field
        .into_iter()
        .map(|mut field| {
            field["key"] = json!("due");
            field["column_id"] = json!("COL_DATE");
            field["row_id"] = json!("ROW_DATE");
            field
        })
        .collect();
    let page = RecordsPage::parse(json!({"ok":true,"records":[{"id":"ROW_DATE","date_created":1,"created_by":"U_DATE","fields":fields}]})).unwrap();
    (document, page)
}
fn date(value: &str) -> CellEditValue {
    CellEditValue::Date(Some(value.into()))
}
fn capture(
    doc: &Document,
    page: &RecordsPage,
    value: CellEditValue,
) -> Result<CellEdit, ListError> {
    CellEdit::capture(
        doc,
        page,
        "ROW_DATE",
        "COL_DATE",
        Some("full"),
        Some(&policy("F_OTHER")),
        value,
    )
}
fn read(doc: &Document, page: &RecordsPage) -> Result<CellEditValue, ListError> {
    doc.columns[0].edit_value(&page.records[0], &doc.id, Some(&policy("F_OTHER")))
}

#[test]
fn missing_calendar_cell_sets_exact_typed_payload_and_clear_uses_empty_array() {
    for kind in ["date", "due_date", "todo_due_date"] {
        for empty in [
            None,
            Some(json!({})),
            Some(json!({"date":[]})),
            Some(json!({"date":[],"timestamp":[],"value":null})),
        ] {
            let (doc, page) = fixture(kind, empty);
            assert_eq!(read(&doc, &page).unwrap(), CellEditValue::Date(None));
            let edit = capture(&doc, &page, date("2028-02-29")).unwrap();
            assert_eq!(
                edit.cells(),
                json!([{"row_id":"ROW_DATE","column_id":"COL_DATE","date":["2028-02-29"]}])
            );
            assert!(edit.cells()[0].get("timestamp").is_none());
            assert!(capture(&doc, &page, CellEditValue::Date(None)).is_err());
        }
        let (doc, page) = fixture(
            kind,
            Some(json!({"date":["2026-10-13"],"timestamp":[-1],"value":"2026-10-13"})),
        );
        let original = page.clone();
        assert_eq!(read(&doc, &page).unwrap(), date("2026-10-13"));
        assert!(capture(&doc, &page, date("2026-10-13")).is_err());
        let edit = capture(&doc, &page, CellEditValue::Date(None)).unwrap();
        assert_eq!(
            edit.cells(),
            json!([{"row_id":"ROW_DATE","column_id":"COL_DATE","date":[]}])
        );
        assert_eq!(
            page, original,
            "capturing/clearing must not mutate the sentinel snapshot"
        );
        let (_, cleared) = fixture(kind, Some(json!({"date":[],"timestamp":[-1],"value":null})));
        assert!(edit.readback_matches(&doc, &cleared));
    }
}

#[test]
fn exact_calendar_validation_rejects_ranges_partial_dates_rollover_and_timestamp_modes() {
    for kind in ["date", "due_date", "todo_due_date"] {
        let (doc, page) = fixture(kind, None);
        for valid in [
            "0001-01-01",
            "2000-02-29",
            "2024-02-29",
            "2026-12-31",
            "9999-12-31",
        ] {
            assert!(capture(&doc, &page, date(valid)).is_ok(), "{valid}");
        }
        for invalid in [
            "",
            "0",
            "-1",
            "2026-2-03",
            "2026-02-3",
            "2026-02-29",
            "1900-02-29",
            "2026-04-31",
            "2026-00-10",
            "2026-13-10",
            "0000-01-01",
            "10000-01-01",
            "2026-10-13T00:00:00Z",
            " 2026-10-13",
            "2026-10-13 ",
            "２０２６-10-13",
        ] {
            assert!(capture(&doc, &page, date(invalid)).is_err(), "{invalid}");
        }
        for stored in [
            json!({"date":null}),
            json!({"date":"2026-10-13"}),
            json!({"date":[null]}),
            json!({"date":[0]}),
            json!({"date":[""]}),
            json!({"date":["2026-02-29"]}),
            json!({"date":["2026-10-13","2026-10-15"]}),
            json!({"timestamp":[0]}),
            json!({"timestamp":[-1]}),
            json!({"timestamp":[1791849600]}),
            json!({"date":["2026-10-13"],"timestamp":[0]}),
            json!({"date":["2026-10-13"],"timestamp":[1791849600]}),
            json!({"date":[],"timestamp":[0]}),
            json!({"date":["2026-10-13"],"timestamp":null}),
            json!({"date":["2026-10-13"],"timestamp":[-1,-1]}),
            json!({"date":["2026-10-13"],"timestamp":["-1"]}),
            json!({"text":"Date with missing original"}),
        ] {
            let (doc, page) = fixture(kind, Some(stored.clone()));
            assert!(read(&doc, &page).is_err(), "{stored}");
            assert!(
                !doc.columns[0].editable_value(&page.records[0], &doc.id, Some(&policy("F_OTHER"))),
                "{stored}"
            );
            assert!(
                capture(&doc, &page, date("2026-10-20")).is_err(),
                "{stored}"
            );
        }
    }
}

#[test]
fn calendar_edits_need_verified_ordinary_policy_and_fresh_canonical_identity() {
    for kind in ["date", "due_date", "todo_due_date"] {
        let (doc, page) = fixture(kind, Some(json!({"date":["2026-10-13"],"timestamp":[-1]})));
        let edit = capture(&doc, &page, date("2026-10-20")).unwrap();
        assert!(
            edit.revalidate(&doc, &page, Some("full"), Some(&policy("F_OTHER")))
                .is_ok()
        );
        for personal in [None, Some(policy("F_DATE"))] {
            assert!(
                CellEdit::capture(
                    &doc,
                    &page,
                    "ROW_DATE",
                    "COL_DATE",
                    Some("full"),
                    personal.as_ref(),
                    date("2026-10-20")
                )
                .is_err()
            );
            assert!(
                edit.revalidate(&doc, &page, Some("full"), personal.as_ref())
                    .is_err()
            );
        }
        assert!(
            edit.revalidate(&doc, &page, None, Some(&policy("F_OTHER")))
                .is_err()
        );
        let mut changed = page.raw.clone();
        changed["records"][0]["fields"][0]["timestamp"] = json!([0]);
        let changed = RecordsPage::parse(changed).unwrap();
        assert!(
            edit.revalidate(&doc, &changed, Some("full"), Some(&policy("F_OTHER")))
                .is_err()
        );
        assert!(edit.readback_value(&doc, &changed).is_none());
        let mut changed = doc.raw.clone();
        changed["list_metadata"]["schema"][0]["key"] = json!("replaced");
        let changed = Document::parse(changed).unwrap();
        assert!(
            edit.revalidate(&changed, &page, Some("full"), Some(&policy("F_OTHER")))
                .is_err()
        );
        assert!(edit.readback_value(&changed, &page).is_none());
        for key in ["archived", "is_deleted"] {
            let mut changed = page.raw.clone();
            changed["records"][0][key] = json!(true);
            let changed = RecordsPage::parse(changed).unwrap();
            assert!(capture(&doc, &changed, date("2026-10-20")).is_err());
            assert!(edit.readback_value(&doc, &changed).is_none());
        }
    }
}

#[test]
fn date_receipts_and_readback_require_real_matching_values_without_conflicting_timestamp() {
    let (doc, page) = fixture(
        "todo_due_date",
        Some(json!({"date":["2026-10-13"],"timestamp":[-1]})),
    );
    let edit = capture(&doc, &page, date("2026-10-20")).unwrap();
    let receipt = json!({"ok":true,"cells":edit.cells()});
    assert!(edit.validate_response(&receipt).is_ok());
    for timestamp in [json!([]), json!([-1]), json!([-1.0])] {
        let mut receipt = receipt.clone();
        receipt["cells"][0]["timestamp"] = timestamp;
        assert!(edit.validate_response(&receipt).is_ok());
    }
    for timestamp in [
        json!([0]),
        json!([1791849600]),
        json!([-1, -1]),
        json!(null),
        json!(false),
    ] {
        let mut receipt = receipt.clone();
        receipt["cells"][0]["timestamp"] = timestamp;
        assert!(edit.validate_response(&receipt).is_err());
    }
    for invalid in [
        json!({"ok":true}),
        json!({"ok":true,"cells":[]}),
        json!({"ok":false,"cells":edit.cells()}),
    ] {
        assert!(edit.validate_response(&invalid).is_err());
    }
    for (key, value) in [
        ("row_id", json!("OTHER")),
        ("column_id", json!("OTHER")),
        ("date", json!(["2026-10-21"])),
        ("date", json!(["2026-10-20", "2026-10-21"])),
        ("date", json!([1792444800])),
        ("error", json!("invalid_date")),
    ] {
        let mut receipt = receipt.clone();
        receipt["cells"][0][key] = value;
        assert!(edit.validate_response(&receipt).is_err());
    }
    let (_, matching) = fixture(
        "todo_due_date",
        Some(json!({"date":["2026-10-20"],"timestamp":[-1]})),
    );
    assert!(edit.readback_matches(&doc, &matching));
    let (_, different) = fixture(
        "todo_due_date",
        Some(json!({"date":["2026-10-21"],"timestamp":[]})),
    );
    assert!(!edit.readback_matches(&doc, &different));
    assert_eq!(
        edit.readback_value(&doc, &different),
        Some(date("2026-10-21"))
    );
    let mut replaced = matching.raw.clone();
    replaced["records"][0]["date_created"] = json!(99);
    assert!(
        edit.readback_value(&doc, &RecordsPage::parse(replaced).unwrap())
            .is_none()
    );
    let (_, range) = fixture(
        "todo_due_date",
        Some(json!({"date":["2026-10-20","2026-10-21"]})),
    );
    assert!(edit.readback_value(&doc, &range).is_none());
}

#[test]
fn calendar_clear_does_not_claim_success_for_later_and_payload_is_one_attempt() {
    let (doc, page) = fixture(
        "date",
        Some(json!({"date":["2026-10-13"],"timestamp":[-1]})),
    );
    let edit = capture(&doc, &page, CellEditValue::Date(None)).unwrap();
    let mut receipt = json!({"ok":true,"cells":edit.cells()});
    receipt["cells"][0]["timestamp"] = json!([-1]);
    assert!(edit.validate_response(&receipt).is_ok());
    receipt["cells"][0]["timestamp"] = json!([0]);
    assert!(edit.validate_response(&receipt).is_err());
    let (_, later) = fixture("date", Some(json!({"date":[],"timestamp":[0]})));
    assert!(edit.readback_value(&doc, &later).is_none());
    let (_, cleared) = fixture("date", Some(json!({"date":[],"timestamp":[],"value":null})));
    assert!(edit.readback_matches(&doc, &cleared));
    let (_, absent) = fixture("date", None);
    assert!(edit.readback_matches(&doc, &absent));
    let workspace = WorkspaceSession {
        team_id: "T_DATE".into(),
        enterprise_id: None,
        user_id: "U_DATE".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-only".into(),
    };
    let request = api::list_cell_update(&SlackClient::default(), &workspace, &edit);
    assert!(!request.retry_safe());
    assert!(request.url.contains("/api/lists.cells.update?"));
    let RequestBody::Form(fields) = request.body else {
        panic!()
    };
    let cells: Value =
        serde_json::from_str(&fields.iter().find(|(key, _)| key == "cells").unwrap().1).unwrap();
    assert_eq!(cells, edit.cells());
    assert!(cells[0].get("timestamp").is_none());
    assert!(cells[0].get("row_id_to_create").is_none());
    assert!(cells[0].get("column_id_to_create").is_none());
    assert!(capture(&doc, &page, CellEditValue::Number(Some(1.into()))).is_err());
}
