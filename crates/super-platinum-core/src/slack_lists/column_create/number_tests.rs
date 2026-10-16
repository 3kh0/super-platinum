use super::*;
fn document() -> Document {
    Document::parse(json!({"id":"F1","mode":"list","user":"U1","created":1,"access":"write",
        "list_limits":{"over_column_maximum":false},"list_metadata":{"schema":[
            {"id":"TITLE","key":"NAME","name":"Title","type":"text","options":{"format":"text"},"is_primary_column":true},
            {"id":"OLD_NUMBER","key":"old_count","name":"Existing","type":"number","options":{"precision":2}},
            {"id":"CHECK","key":"done","name":"Done","type":"checkbox"}],
            "views":[{"id":"V1","name":"Custom","type":"table","columns":[{"id":"TITLE","key":"NAME","visible":true},{"id":"OLD_NUMBER","key":"old_count","visible":true}],
            "sorts":[{"key":"old_count","ascending":true}],"filters":[{"key":"done","operator":"is_not_empty","values":[]}],"info_column_filters":[]}],"opaque":{"keep":1}}})).unwrap()
}
fn records() -> RecordsPage {
    RecordsPage::parse(json!({"ok":true,"records":[
        {"id":"R1","fields":[{"key":"NAME","text":"Keep"},{"key":"old_count","number":[12.5]},{"key":"done","checkbox":true}]},
        {"id":"R2","parent_record_id":"R1","fields":[]}],"response_metadata":{"next_cursor":"NEXT"}})).unwrap()
}
fn ordinary() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn capture(d: &Document, p: &RecordsPage, kind: ColumnCreateKind) -> ColumnCreate {
    ColumnCreate::capture_kind(
        d,
        p,
        "V1",
        "Amount",
        kind,
        "U1",
        Some("full"),
        Some(&ordinary()),
    )
    .unwrap()
}
fn added(d: &Document, kind: ColumnCreateKind) -> Document {
    let mut raw = d.raw.clone();
    raw["list_metadata"]["schema"].as_array_mut().unwrap().insert(1,json!({"id":"SERVER_NEW","key":"new_amount","name":"Amount","type":kind.as_str(),"options":kind.options(),"opaque":{"server":2}}));
    raw["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .insert(
            1,
            json!({"id":"SERVER_NEW","key":"new_amount","visible":true,"opaque":{"width":99}}),
        );
    Document::parse(raw).unwrap()
}
#[test]
fn text_wrapper_is_identical_to_explicit_text_and_cannot_observe_number() {
    let d = document();
    let p = records();
    let old = ColumnCreate::capture(
        &d,
        &p,
        "V1",
        "Amount",
        "U1",
        Some("full"),
        Some(&ordinary()),
    )
    .unwrap();
    let explicit = capture(&d, &p, ColumnCreateKind::Text);
    assert_eq!(old, explicit);
    assert_eq!(old.kind(), ColumnCreateKind::Text);
    assert!(
        old.matching_addition(&added(&d, ColumnCreateKind::Text), &p)
            .is_some()
    );
    assert!(
        old.matching_addition(&added(&d, ColumnCreateKind::NumberDefault0), &p)
            .is_none()
    );
}
#[test]
fn number_observation_projects_empty_cells_preserving_filter_sort_cursor_and_hierarchy() {
    use super::super::{
        CellValue, ColumnPrecision, ColumnRename,
        saved_views::{ProjectionContext, catalog, project},
    };
    let d = document();
    let p = records();
    let intent = capture(&d, &p, ColumnCreateKind::NumberDefault0);
    let fresh = added(&d, ColumnCreateKind::NumberDefault0);
    let column = intent.matching_addition(&fresh, &p).unwrap();
    assert_eq!(
        (&column.id, &column.key),
        (&"SERVER_NEW".to_owned(), &"new_amount".to_owned())
    );
    assert_eq!(column.raw["options"], json!({"precision":0}));
    assert_eq!(column.raw["opaque"], json!({"server":2}));
    assert!(p.records.iter().all(|r| column.cell(r) == CellValue::Empty));
    assert_eq!(p.next_cursor.as_deref(), Some("NEXT"));
    assert_eq!(p.records[1].parent_id(), Some("R1"));
    let before = project(
        &d,
        &p,
        &catalog(&d).unwrap()[0],
        &ProjectionContext::default(),
    )
    .unwrap();
    let after = project(
        &fresh,
        &p,
        &catalog(&fresh).unwrap()[0],
        &ProjectionContext::default(),
    )
    .unwrap();
    assert_eq!(before.groups, after.groups);
    assert_eq!(before.incomplete, after.incomplete);
    assert_eq!(after.column_ids, vec!["TITLE", "SERVER_NEW", "OLD_NUMBER"]);
    assert_eq!(intent.original_records(), &p);
    // Existing actions can review this field; creation itself did not run them.
    assert!(ColumnRename::supports(column));
    assert!(
        ColumnPrecision::capture(&fresh, &column.id, 2, Some("full"), Some(&ordinary())).is_ok()
    );
    let mut with_zero = p.raw.clone();
    with_zero["records"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":"new_amount","number":[0]}));
    assert!(
        intent
            .matching_addition(&fresh, &RecordsPage::parse(with_zero).unwrap())
            .is_none()
    );
}
#[test]
fn number_requires_exact_default_options_and_original_identity_metadata_population() {
    let d = document();
    let p = records();
    let intent = capture(&d, &p, ColumnCreateKind::NumberDefault0);
    let fresh = added(&d, ColumnCreateKind::NumberDefault0);
    for options in [
        Value::Null,
        json!({}),
        json!({"precision":1}),
        json!({"precision":"0"}),
        json!({"precision":0,"format":"number"}),
        json!({"format":"text"}),
    ] {
        let mut raw = fresh.raw.clone();
        raw["list_metadata"]["schema"][1]["options"] = options;
        assert!(
            intent
                .matching_addition(&Document::parse(raw).unwrap(), &p)
                .is_none()
        );
    }
    assert!(
        intent
            .matching_addition(&added(&d, ColumnCreateKind::Text), &p)
            .is_none()
    );
    for edit in 0..4 {
        let mut raw = fresh.raw.clone();
        match edit {
            0 => raw["list_metadata"]["opaque"]["keep"] = json!(2),
            1 => raw["list_metadata"]["schema"][1]["is_primary_column"] = json!(true),
            2 => raw["list_metadata"]["views"][0]["sorts"] = json!([]),
            _ => raw["list_metadata"]["views"][0]["filters"] = json!([]),
        }
        assert!(
            intent
                .matching_addition(&Document::parse(raw).unwrap(), &p)
                .is_none()
        );
    }
    let mut cursor = p.raw.clone();
    cursor["response_metadata"]["next_cursor"] = json!("OTHER");
    assert!(
        intent
            .matching_addition(&fresh, &RecordsPage::parse(cursor).unwrap())
            .is_none()
    );
    // A key in opaque old metadata is still an orphan even for Number creation.
    let mut raw = d.raw.clone();
    raw["list_metadata"]["opaque"]["new_amount"] = json!(true);
    let orphan = Document::parse(raw).unwrap();
    assert!(
        capture(&orphan, &p, ColumnCreateKind::NumberDefault0)
            .matching_addition(&added(&orphan, ColumnCreateKind::NumberDefault0), &p)
            .is_none()
    );
}
#[test]
fn captured_kind_and_permit_do_not_follow_a_later_type_choice() {
    use crate::{
        config::WorkspaceSession,
        slack::{SlackClient, api, client::RequestBody},
    };
    let d = document();
    let p = records();
    let mut choice = ColumnCreateKind::NumberDefault0;
    let intent = capture(&d, &p, choice);
    let mut attempt = ColumnCreateAttempt::new(intent.clone());
    let permit = attempt
        .admit(&d, &p, "U1", Some("full"), Some(&ordinary()))
        .unwrap();
    choice = ColumnCreateKind::Text;
    let later = capture(&d, &p, choice);
    assert_ne!(intent.kind(), later.kind());
    assert_eq!(attempt.intent().kind(), ColumnCreateKind::NumberDefault0);
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.slack.com".into(),
        token: "fixture-token".into(),
    };
    let request = api::list_column_create(&SlackClient::default(), &session, permit);
    let RequestBody::Form(fields) = request.body else {
        panic!()
    };
    assert_eq!(fields[4], ("type".into(), "number".into()));
    assert_eq!(fields[5], ("options".into(), r#"{"precision":0}"#.into()));
    assert!(intent.matching_addition(&added(&d, choice), &p).is_none());
    assert!(
        intent
            .matching_addition(&added(&d, ColumnCreateKind::NumberDefault0), &p)
            .is_some()
    );
    assert!(
        attempt
            .admit(&d, &p, "U1", Some("full"), Some(&ordinary()))
            .is_err()
    );
}
#[test]
fn number_admission_keeps_owner_capacity_guards_and_never_rearms() {
    let d = document();
    let p = records();
    for not_requested in [false, true] {
        let mut attempt =
            ColumnCreateAttempt::new(capture(&d, &p, ColumnCreateKind::NumberDefault0));
        let mut raw = d.raw.clone();
        raw["list_limits"]["over_column_maximum"] = json!(true);
        assert!(
            attempt
                .admit(
                    &Document::parse(raw).unwrap(),
                    &p,
                    "U1",
                    Some("full"),
                    Some(&ordinary())
                )
                .is_err()
        );
        assert!(
            attempt
                .admit(&d, &p, "U2", Some("full"), Some(&ordinary()))
                .is_err()
        );
        assert_eq!(attempt.phase(), ColumnCreatePhase::Reviewing);
        let _permit = attempt
            .admit(&d, &p, "U1", Some("full"), Some(&ordinary()))
            .unwrap();
        assert!(attempt.finish(not_requested));
        assert_eq!(
            attempt.phase(),
            if not_requested {
                ColumnCreatePhase::NotRequested
            } else {
                ColumnCreatePhase::Unconfirmed
            }
        );
        assert!(!attempt.finish(not_requested));
        assert!(
            attempt
                .admit(&d, &p, "U1", Some("full"), Some(&ordinary()))
                .is_err()
        );
    }
}
