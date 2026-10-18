use super::*;
fn fixture() -> (Document, RecordsPage, ProjectionContext, PersonalTaskLists) {
    let doc=Document::parse(json!({"id":"F1","mode":"list","access":"write","user":"U1","created":1,"list_metadata":{"schema":[
        {"id":"CN","key":"name","name":"Item","type":"text","is_primary_column":true},
        {"id":"CS","key":"stage","name":"Stage","type":"select","options":{"format":"multi_select","choices":[{"value":"A","label":"Alpha"},{"value":"B","label":"Beta"},{"value":"C","label":"Empty"}]}}
    ],"views":[{"id":"V1","type":"board","name":"Board","columns":[{"key":"name","visible":true},{"key":"stage","visible":true}],"grouping":{"group_by":"stage","group_by_column_id":"CS"},"filters":[],"sorts":[],"opaque":7}],"opaque":{"keep":true}}})).unwrap();
    let rows=(1..=4).map(|i|json!({"id":format!("Rec{i}"),"list_id":"F1","position":format!("{i:010}"),"date_created":i,"opaque":{"keep":[i]},"fields":[{"key":"name","text":format!("Item {i}")},{"key":"stage","column_id":"CS","select":[if i<4{"A"}else{"B"}],"opaque":"cell"}]})).collect::<Vec<_>>();
    let page=RecordsPage::parse(json!({"ok":true,"records":rows,"response_metadata":{"next_cursor":"retained+/=","opaque":true}})).unwrap();
    let context = ProjectionContext {
        editable: true,
        ..Default::default()
    };
    let personal = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap();
    (doc, page, context, personal)
}
fn capture(
    doc: &Document,
    page: &RecordsPage,
    context: &ProjectionContext,
    personal: &PersonalTaskLists,
    destination: &str,
    before: Option<&str>,
) -> BoardRecordMove {
    BoardRecordMove::capture(
        doc,
        page,
        "V1",
        context,
        "Rec3",
        "A",
        destination,
        before,
        Some("full"),
        Some(personal),
    )
    .unwrap()
}
#[test]
fn same_lane_uses_fraction_between_actual_destination_neighbors_and_preserves_cursor() {
    let (doc, page, c, p) = fixture();
    let intent = capture(&doc, &page, &c, &p, "A", Some("Rec2"));
    assert_eq!(intent.desired_position(), Some("0000000001.V"));
    assert_eq!(intent.cells(), None);
    let desired = RecordsPage::parse(intent.expected_page.clone()).unwrap();
    assert!(intent.observes_desired(&doc, &desired));
    assert_eq!(desired.next_cursor, page.next_cursor);
    let mut restore = desired.raw;
    restore["records"][2]
        .as_object_mut()
        .unwrap()
        .remove("view_positions");
    assert_eq!(restore, page.raw);
}
#[test]
fn cross_lane_replaces_whole_select_value_in_same_request_and_retains_opaque_cells() {
    let (doc, page, c, p) = fixture();
    let intent = capture(&doc, &page, &c, &p, "B", Some("Rec4"));
    assert_eq!(intent.desired_position(), Some("0000000003"));
    assert_eq!(
        intent.cells(),
        Some(json!([{"row_id":"Rec3","column_id":"CS","select":["B"]}]))
    );
    assert_eq!(
        intent.expected_page["records"][2]["fields"][1]["opaque"],
        "cell"
    );
    assert!(intent.observes_desired(
        &doc,
        &RecordsPage::parse(intent.expected_page.clone()).unwrap()
    ));
}
#[test]
fn empty_destination_keeps_null_position_as_cell_only() {
    let (doc, page, c, p) = fixture();
    let intent = capture(&doc, &page, &c, &p, "C", None);
    assert!(intent.is_cell_only());
    assert_eq!(intent.desired_position(), None);
    assert!(
        intent.expected_page["records"][2]
            .get("view_positions")
            .is_none()
    );
    assert_eq!(intent.cells().unwrap()[0]["select"], json!(["C"]));
}
#[test]
fn full_raw_current_guards_prevent_neighbor_cursor_metadata_and_find_drift() {
    let (doc, page, c, p) = fixture();
    let intent = capture(&doc, &page, &c, &p, "A", Some("Rec2"));
    for changed in ["neighbor", "cursor", "opaque"] {
        let mut raw = page.raw.clone();
        match changed {
            "neighbor" => raw["records"][0]["position"] = json!("0000000000"),
            "cursor" => raw["response_metadata"]["next_cursor"] = json!("new"),
            _ => raw["records"][3]["opaque"] = json!({}),
        };
        assert!(
            intent
                .revalidate(
                    &doc,
                    &RecordsPage::parse(raw).unwrap(),
                    &c,
                    Some("full"),
                    Some(&p)
                )
                .is_err()
        );
    }
    let mut raw = doc.raw.clone();
    raw["list_metadata"]["opaque"] = json!({});
    assert!(
        intent
            .revalidate(
                &Document::parse(raw).unwrap(),
                &page,
                &c,
                Some("full"),
                Some(&p)
            )
            .is_err()
    );
    assert!(
        intent
            .revalidate(&doc, &page, &c, Some("full"), None)
            .is_err()
    );
    let mut changed_context = c.clone();
    changed_context.find = Some(super::super::find::Query {
        text: "Item 3".into(),
        ..Default::default()
    });
    assert!(
        intent
            .revalidate(&doc, &page, &changed_context, Some("full"), Some(&p))
            .is_err()
    );
}
#[test]
fn one_use_and_explicit_observation_never_treat_ack_as_success() {
    let (doc, page, c, p) = fixture();
    let intent = capture(&doc, &page, &c, &p, "A", Some("Rec2"));
    let observed = RecordsPage::parse(intent.expected_page.clone()).unwrap();
    let mut attempt = BoardRecordMoveAttempt::new(intent);
    let _permit = attempt
        .admit(&doc, &page, &c, Some("full"), Some(&p))
        .unwrap();
    assert!(
        attempt
            .admit(&doc, &page, &c, Some("full"), Some(&p))
            .is_err()
    );
    assert!(!attempt.observe(&doc, &observed));
    assert!(attempt.finish(false));
    assert_eq!(attempt.phase(), BoardRecordMovePhase::Unconfirmed);
    assert!(!attempt.observe(&doc, &page));
    assert!(attempt.observe(&doc, &observed));
}
#[test]
fn duplicated_multi_membership_and_stale_or_noop_targets_refuse() {
    let (doc, page, c, p) = fixture();
    assert!(
        BoardRecordMove::capture(
            &doc,
            &page,
            "V1",
            &c,
            "Rec3",
            "A",
            "A",
            None,
            Some("full"),
            Some(&p)
        )
        .is_err()
    );
    assert!(
        BoardRecordMove::capture(
            &doc,
            &page,
            "V1",
            &c,
            "Rec3",
            "A",
            "A",
            Some("gone"),
            Some("full"),
            Some(&p)
        )
        .is_err()
    );
    let mut raw = page.raw;
    raw["records"][2]["fields"][1]["select"] = json!(["A", "B"]);
    assert!(
        BoardRecordMove::capture(
            &doc,
            &RecordsPage::parse(raw).unwrap(),
            "V1",
            &c,
            "Rec3",
            "A",
            "B",
            Some("Rec4"),
            Some("full"),
            Some(&p)
        )
        .is_err()
    );
}

#[test]
fn exact_destination_bounds_require_same_list_and_ordinary_top_level_identity() {
    let (doc, page, c, p) = fixture();
    for (key, value) in [
        ("list_id", json!("F_OTHER")),
        ("parent_record_id", json!(true)),
        ("archived", json!("false")),
        ("is_suggested", json!(1)),
    ] {
        let mut raw = page.raw.clone();
        raw["records"][0][key] = value;
        let changed = RecordsPage::parse(raw).unwrap();
        assert!(
            BoardRecordMove::capture(
                &doc,
                &changed,
                "V1",
                &c,
                "Rec3",
                "A",
                "A",
                Some("Rec2"),
                Some("full"),
                Some(&p)
            )
            .is_err()
        );
    }
}

#[test]
fn moved_target_duplicate_overrides_and_raw_model_disagreement_refuse_before_capture() {
    let (doc, mut page, c, p) = fixture();
    page.raw["records"][2]["view_positions"] = json!({"V1":"0000000003","v1":"0000000004"});
    assert!(RecordsPage::parse(page.raw.clone()).is_err());
    assert!(
        BoardRecordMove::capture(
            &doc,
            &page,
            "V1",
            &c,
            "Rec3",
            "A",
            "A",
            Some("Rec2"),
            Some("full"),
            Some(&p)
        )
        .is_err()
    );
    let (doc, mut page, c, p) = fixture();
    page.records[0].raw["position"] = json!("0000000000");
    assert!(
        BoardRecordMove::capture(
            &doc,
            &page,
            "V1",
            &c,
            "Rec3",
            "A",
            "A",
            Some("Rec2"),
            Some("full"),
            Some(&p)
        )
        .is_err()
    );
}
