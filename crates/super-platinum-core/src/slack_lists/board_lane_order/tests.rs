use super::*;
pub(crate) fn document(kind: &str) -> Document {
    Document::parse(json!({"id":"F1","mode":"list","access":"write","created":1,"user":"U1","opaque_file":true,"list_metadata":{
        "opaque":{"keep":1},"schema":[{"id":"C1","key":"status","name":"Status","type":kind,"options":{"format":"multi_select","choices":[{"value":"a","label":"Alpha"},{"value":"β","label":"Beta"}]}}],
        "views":[{"id":"V1","name":"Board","type":"board","columns":[],"sorts":[],"grouping":{"group_by":"status","group_by_column_id":"C1","opaque":{"keep":[1,2]}},"opaque_view":5},
        {"id":"V2","name":"Other","type":"table","opaque":{"keep":true}}]}})).unwrap()
}
fn personal() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn capture(doc: &Document) -> BoardLaneOrder {
    BoardLaneOrder::capture(
        doc,
        "V1",
        "β",
        "a",
        BoardLanePlacement::Before,
        Some("full"),
        Some(&personal()),
    )
    .unwrap()
}
fn desired(intent: &BoardLaneOrder) -> Document {
    let mut raw = intent.original().raw.clone();
    raw["list_metadata"]["views"][0]["grouping"] = intent.grouping().clone();
    Document::parse(raw).unwrap()
}
#[test]
fn typed_order_preserves_complete_raw_metadata_and_hidden_empty_lane() {
    let doc = document("select");
    let intent = capture(&doc);
    assert_eq!(
        BoardLaneOrder::current_order(&doc, "V1").unwrap(),
        vec!["", "a", "β"]
    );
    assert_eq!(intent.desired_order(), &["", "β", "a"]);
    assert_eq!(
        intent.grouping(),
        &json!({"group_by":"status","group_by_column_id":"C1","opaque":{"keep":[1,2]},"order":[{"select":[]},{"select":["β"]},{"select":["a"]}]})
    );
    let after = desired(&intent);
    assert!(intent.observes_desired(&after));
    let mut restored = after.raw;
    restored["list_metadata"]["views"][0]["grouping"] = doc.views[0]["grouping"].clone();
    assert_eq!(restored, doc.raw);
}
#[test]
fn actual_order_normalizes_deleted_duplicates_without_losing_unrelated_grouping() {
    let mut raw = document("select").raw;
    raw["list_metadata"]["views"][0]["grouping"]["order"] =
        json!([{"select":["deleted"]},{"select":["β"]},{"select":["β"]}]);
    let doc = Document::parse(raw).unwrap();
    assert_eq!(
        BoardLaneOrder::current_order(&doc, "V1").unwrap(),
        vec!["β", "", "a"]
    );
    let intent = BoardLaneOrder::capture(
        &doc,
        "V1",
        "a",
        "β",
        BoardLanePlacement::Before,
        Some("full"),
        Some(&personal()),
    )
    .unwrap();
    assert_eq!(intent.desired_order(), &["a", "β", ""]);
}
#[test]
fn checkbox_order_uses_actual_boolean_cells() {
    let doc = document("checkbox");
    let intent = BoardLaneOrder::capture(
        &doc,
        "V1",
        "true",
        "",
        BoardLanePlacement::Before,
        Some("full"),
        Some(&personal()),
    )
    .unwrap();
    assert_eq!(
        intent.grouping()["order"],
        json!([{"checkbox":true},{"checkbox":false}])
    );
    assert!(intent.observes_desired(&desired(&intent)));
}
#[test]
fn utf16_sort_and_noop_and_unsupported_identity_are_explicit() {
    let mut raw = document("select").raw;
    raw["list_metadata"]["schema"][0]["options"]["choices"][0]["label"] = json!("\u{e000}");
    raw["list_metadata"]["schema"][0]["options"]["choices"][1]["label"] = json!("\u{10000}");
    raw["list_metadata"]["views"][0]["sorts"] = json!([{"key":"status","ascending":true}]);
    let doc = Document::parse(raw).unwrap();
    assert_eq!(
        BoardLaneOrder::current_order(&doc, "V1").unwrap(),
        vec!["", "β", "a"]
    );
    assert!(
        BoardLaneOrder::capture(
            &doc,
            "V1",
            "β",
            "a",
            BoardLanePlacement::Before,
            Some("full"),
            Some(&personal())
        )
        .is_err()
    );
    for value in [Value::Null, json!("wrong"), json!("")] {
        let mut raw = doc.raw.clone();
        raw["list_metadata"]["views"][0]["grouping"]["group_by_column_id"] = value;
        assert!(BoardLaneOrder::current_order(&Document::parse(raw).unwrap(), "V1").is_err());
    }
}
#[test]
fn malformed_or_enriched_order_cells_are_refused_instead_of_discarded() {
    for order in [
        json!([{"select":["a"],"opaque":7}]),
        json!([{"select":["a","β"]}]),
        json!([{"checkbox":false}]),
        json!({}),
        json!([{"select":[false]}]),
    ] {
        let mut raw = document("select").raw;
        raw["list_metadata"]["views"][0]["grouping"]["order"] = order;
        assert!(BoardLaneOrder::current_order(&Document::parse(raw).unwrap(), "V1").is_err());
    }
}
#[test]
fn fresh_access_full_guard_and_one_use_unconfirmed_observation() {
    let doc = document("select");
    let intent = capture(&doc);
    assert!(intent.revalidate(&doc, Some("full"), None).is_err());
    let mut changed = doc.raw.clone();
    changed["list_metadata"]["opaque"]["keep"] = json!(2);
    assert!(
        intent
            .revalidate(
                &Document::parse(changed).unwrap(),
                Some("full"),
                Some(&personal())
            )
            .is_err()
    );
    let mut attempt = BoardLaneOrderAttempt::new(intent.clone());
    let _permit = attempt
        .admit(&doc, Some("full"), Some(&personal()))
        .unwrap();
    assert!(
        attempt
            .admit(&doc, Some("full"), Some(&personal()))
            .is_err()
    );
    assert!(!attempt.observe(&desired(&intent)));
    assert!(attempt.finish(false));
    assert_eq!(attempt.phase(), BoardLaneOrderPhase::Unconfirmed);
    assert!(!attempt.observe(&doc));
    let mut changed = desired(&intent).raw;
    changed["list_metadata"]["views"][1]["opaque"] = json!({});
    assert!(!attempt.observe(&Document::parse(changed).unwrap()));
    assert!(attempt.observe(&desired(&intent)));
    assert!(!attempt.finish(false));
}

#[test]
fn non_switcher_views_without_names_remain_raw_and_never_block_or_hide_duplicates() {
    let mut raw = document("select").raw;
    let siblings = raw["list_metadata"]["views"].as_array_mut().unwrap();
    siblings.push(json!({"id":"V_RECORD","type":"record","opaque":{"keep":[1,2]}}));
    siblings.push(json!({"id":"V_CANVAS","type":"canvas_inline","opaque":{"nested":true}}));
    siblings.push(json!({"id":"V_FUTURE","name":"Future","type":"future_layout","unknown":7}));
    let doc = Document::parse(raw).unwrap();
    let intent = capture(&doc);
    let observed = desired(&intent);
    assert!(intent.observes_desired(&observed));
    assert_eq!(&observed.views[1..], &doc.views[1..]);
    let mut changed = observed.raw;
    changed["list_metadata"]["views"][2]["opaque"]["keep"] = json!([]);
    assert!(!intent.observes_desired(&Document::parse(changed).unwrap()));
    let mut duplicate = doc.raw;
    duplicate["list_metadata"]["views"][2]["id"] = json!("V1");
    // Non-switcher views participate in the parser's identity check too.
    assert_eq!(
        Document::parse(duplicate).unwrap_err(),
        ListError("List views have ambiguous identities.")
    );
}
