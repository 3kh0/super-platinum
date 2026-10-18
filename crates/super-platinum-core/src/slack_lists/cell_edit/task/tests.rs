use super::*;
fn policy(personal: bool) -> PersonalTaskLists {
    PersonalTaskLists::from_response(&json!({"ok":true,"prefs":{"task_lists":if personal {"[\"F_TASK\"]"}else{"[]"},"assigned_to_list":""}})).unwrap()
}
fn fixture(kind: &str, format: &str, users: Value) -> (Document, RecordsPage) {
    (Document::parse(json!({"id":"F_TASK","mode":"list","access":"write","list_metadata":{"schema":[{"id":"COL","key":"assignees","name":"Assignee","type":kind,"options":{"format":format}}],"views":[{"id":"V1","columns":["COL"]}]}})).unwrap(),
    RecordsPage::parse(json!({"ok":true,"records":[{"id":"ROW","list_id":"F_TASK","date_created":1,"created_by":"U00000001","fields":[{"key":"assignees","column_id":"COL","row_id":"ROW","user":users,"future":{"keep":true}},{"key":"sibling","number":[17]}]}],"response_metadata":{"next_cursor":"keep-cursor"}})).unwrap())
}
fn capture(d: &Document, p: &RecordsPage, users: &[&str]) -> TaskAssignment {
    TaskAssignment::capture(
        d,
        p,
        "ROW",
        "COL",
        Some("full"),
        &policy(true),
        users.iter().map(|s| s.to_string()).collect(),
    )
    .unwrap()
}
#[test]
fn new_people_need_exact_checks_and_not_now_is_an_explicit_assignment_choice() {
    let (d, p) = fixture("todo_assignee", "multi_entity", json!(["U00000001"]));
    let original = p.clone();
    let mut a = capture(&d, &p, &["W00000002", "U00000001"]);
    assert_eq!(a.users_to_check().collect::<Vec<_>>(), vec!["W00000002"]);
    assert!(
        a.clone()
            .into_edit(TaskAssignmentDecision::AssignWithoutSharing)
            .is_err()
    );
    for response in [
        json!({"ok":true}),
        json!({"ok":false,"can_see":true}),
        json!({"ok":true,"can_see":1}),
    ] {
        assert!(a.record_access("W00000002", &response).is_err());
    }
    assert!(
        a.record_access("U00000001", &json!({"ok":true,"can_see":true}))
            .is_err()
    );
    a.record_access("W00000002", &json!({"ok":true,"can_see":false}))
        .unwrap();
    assert_eq!(a.sharing_needed().unwrap(), vec!["W00000002"]);
    assert!(
        a.clone()
            .into_edit(TaskAssignmentDecision::ExistingAccess)
            .is_err()
    );
    assert!(
        a.record_access("W00000002", &json!({"ok":true,"can_see":true}))
            .is_err()
    );
    let edit = a
        .into_edit(TaskAssignmentDecision::AssignWithoutSharing)
        .unwrap();
    assert_eq!(
        edit.cells(),
        json!([{"row_id":"ROW","column_id":"COL","user":["W00000002","U00000001"]}])
    );
    assert_eq!(p, original);
    assert!(
        edit.revalidate(&d, &p, Some("full"), Some(&policy(true)))
            .is_ok()
    );
    assert!(
        edit.revalidate(&d, &p, Some("full"), Some(&policy(false)))
            .is_err()
    );
}
#[test]
fn share_ack_is_not_access_and_removal_does_not_share() {
    let (d, p) = fixture("todo_assignee", "single_entity", json!(["U00000001"]));
    let mut a = capture(&d, &p, &["W00000002"]);
    a.record_access("W00000002", &json!({"ok":true,"can_see":false}))
        .unwrap();
    assert!(
        a.observe_shared_access("W00000002", &json!({"ok":true}))
            .is_err()
    );
    a.observe_shared_access("W00000002", &json!({"ok":true,"can_see":true}))
        .unwrap();
    assert!(a.into_edit(TaskAssignmentDecision::ExistingAccess).is_ok());
    let clear = capture(&d, &p, &[]);
    assert!(clear.sharing_needed().unwrap().is_empty());
    assert_eq!(
        clear
            .into_edit(TaskAssignmentDecision::ExistingAccess)
            .unwrap()
            .cells(),
        json!([{"row_id":"ROW","column_id":"COL","user":[]}])
    )
}
#[test]
fn source_distinguishes_ordinary_assignee_and_todo_assignee_without_generic_bypass() {
    let (d, p) = fixture("assignee", "multi_entity", json!([]));
    let a = capture(&d, &p, &["U00000001", "W00000002"]);
    assert_eq!(a.users_to_check().count(), 0);
    assert!(a.into_edit(TaskAssignmentDecision::ExistingAccess).is_ok());
    assert!(
        CellEdit::capture(
            &d,
            &p,
            "ROW",
            "COL",
            Some("full"),
            Some(&policy(false)),
            CellEditValue::Users(vec!["U00000001".into()])
        )
        .is_err()
    );
}
#[test]
fn cardinality_unknown_shapes_identity_permission_and_aba_stay_guarded() {
    let (d, p) = fixture("todo_assignee", "single_entity", json!([]));
    for ids in [
        vec!["U00000001", "W00000002"],
        vec!["bad"],
        vec!["U00000001", "U00000001"],
        vec![],
    ] {
        assert!(
            TaskAssignment::capture(
                &d,
                &p,
                "ROW",
                "COL",
                Some("full"),
                &policy(true),
                ids.iter().map(|s| s.to_string()).collect()
            )
            .is_err()
        );
    }
    let a = capture(&d, &p, &["U00000001"]);
    for edit in 0..4 {
        let mut raw = d.raw.clone();
        match edit {
            0 => raw["access"] = json!("read"),
            1 => raw["list_metadata"]["schema"][0]["options"]["format"] = json!("future"),
            2 => raw["list_metadata"]["views"][0]["columns"] = json!([]),
            _ => raw["title"] = json!("changed"),
        };
        assert!(
            a.revalidate(
                &Document::parse(raw).unwrap(),
                &p,
                Some("full"),
                &policy(true)
            )
            .is_err()
        );
    }
    let mut p2 = p.clone();
    p2.records[0].raw["date_created"] = json!(2);
    assert!(a.revalidate(&d, &p2, Some("full"), &policy(true)).is_err());
}
#[test]
fn task_calendar_encoder_preserves_dates_and_refuses_timestamp_or_range_coercion() {
    let (d, mut p) = fixture("todo_due_date", "single_entity", json!([]));
    let mut raw = p.records[0].raw.clone();
    raw["fields"][0].as_object_mut().unwrap().remove("user");
    raw["fields"][0]["date"] = json!(["2026-10-17"]);
    p.records[0] = Record::parse(raw.clone()).unwrap();
    let e = CellEdit::capture_task_date(
        &d,
        &p,
        "ROW",
        "COL",
        Some("full"),
        &policy(true),
        Some("2026-10-18".into()),
    )
    .unwrap();
    assert_eq!(
        e.cells(),
        json!([{"row_id":"ROW","column_id":"COL","date":["2026-10-18"]}])
    );
    assert!(
        CellEdit::capture_task_date(
            &d,
            &p,
            "ROW",
            "COL",
            Some("full"),
            &policy(true),
            Some("2026-02-30".into())
        )
        .is_err()
    );
    for v in [json!(["2026-10-17", "2026-10-18"]), json!([123])] {
        raw["fields"][0]["date"] = v;
        p.records[0] = Record::parse(raw.clone()).unwrap();
        assert!(
            CellEdit::capture_task_date(&d, &p, "ROW", "COL", Some("full"), &policy(true), None)
                .is_err()
        );
    }
}
