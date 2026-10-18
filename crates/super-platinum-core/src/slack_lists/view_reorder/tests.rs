use super::*;
fn document() -> Document {
    Document::parse(json!({"id":"F_ORDER","mode":"list","access":"write","created":17,"user":"U1","opaque_file":5,
        "list_metadata":{"schema":[{"id":"C_NAME","key":"NAME","name":"Item","type":"text","is_primary_column":true}],
        "default_view":"V_β","opaque":{"keep":[null,7]},"views":[
            {"id":"V_β","name":"東京 board","type":"board","position":"5000000001","columns":[{"key":"NAME","visible":true}],"grouping":{"group_by":"done"},"sorts":[{"key":"other","ascending":false}],"filters":[{"unknown":1}],"opaque":{"keep":true}},
            {"id":"V_A","name":"First","type":"table","position":"5000000000","columns":[],"grouping":{}},
            {"id":"V_C","name":"Third","type":"table","position":"5000000002","opaque":[1,2]},
            {"id":"V_ALL","name":"All items","type":"table","position":"5000000003","default_view_key":"all_items","is_all_items_view":true}]}})).unwrap()
}
fn personal() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn capture(
    doc: &Document,
    id: &str,
    direction: ViewReorderDirection,
) -> Result<ViewReorder, ListError> {
    ViewReorder::capture(doc, id, direction, Some("full"), Some(&personal()))
}
fn desired(intent: &ViewReorder) -> Document {
    let mut raw = intent.original().raw.clone();
    raw["list_metadata"] = intent.expected_metadata.clone();
    Document::parse(raw).unwrap()
}
#[test]
fn adjacent_moves_use_sorted_custom_order_and_preserve_raw_order() {
    let doc = document();
    assert_eq!(
        ViewReorder::current_order(&doc).unwrap(),
        ["V_A", "V_β", "V_C"]
    );
    let intent = capture(&doc, "V_C", ViewReorderDirection::Up).unwrap();
    assert_eq!(intent.desired_position(), "5000000000.V");
    assert_eq!(intent.desired_order(), ["V_A", "V_C", "V_β"]);
    assert!(intent.observes_desired(&desired(&intent)));
    let mut expected = doc.raw.clone();
    expected["list_metadata"]["views"][2]["position"] = json!("5000000000.V");
    assert_eq!(desired(&intent).raw, expected);
    assert_eq!(intent.view_id(), "V_C");
    assert_eq!(intent.direction(), ViewReorderDirection::Up);
}
#[test]
fn built_in_is_not_movable_but_remains_an_actual_upper_bound() {
    let doc = document();
    assert!(ViewReorder::choices(&doc, "V_ALL").is_empty());
    assert_eq!(
        ViewReorder::choices(&doc, "V_A"),
        [ViewReorderDirection::Down]
    );
    assert_eq!(
        ViewReorder::choices(&doc, "V_C"),
        [ViewReorderDirection::Up]
    );
    let intent = capture(&doc, "V_β", ViewReorderDirection::Down).unwrap();
    assert_eq!(intent.desired_position(), "5000000002.V");
    assert_eq!(intent.desired_order(), ["V_A", "V_C", "V_β"]);
    assert_eq!(desired(&intent).views[3], doc.views[3]);
    assert!(capture(&doc, "V_A", ViewReorderDirection::Up).is_err());
    assert!(capture(&doc, "V_C", ViewReorderDirection::Down).is_err());
}
#[test]
fn first_custom_move_uses_source_lower_boundary_and_unicode_identity() {
    let intent = capture(&document(), "V_β", ViewReorderDirection::Up).unwrap();
    assert_eq!(intent.desired_position(), "4999999999");
    assert_eq!(intent.desired_order(), ["V_β", "V_A", "V_C"]);
    assert_eq!(desired(&intent).views[0]["name"], "東京 board");
}
#[test]
fn repeated_fractional_move_does_not_normalize_existing_keys() {
    let first = capture(&document(), "V_C", ViewReorderDirection::Up).unwrap();
    let fresh = desired(&first);
    let second = capture(&fresh, "V_A", ViewReorderDirection::Down).unwrap();
    assert_eq!(second.desired_position(), "5000000000.l");
    assert_eq!(desired(&second).views[2]["position"], "5000000000.V");
}
#[test]
fn missing_numeric_duplicate_and_malformed_positions_refuse_without_coercion() {
    for value in [
        Value::Null,
        json!(1),
        json!("1"),
        json!("5000000001"),
        json!("5000000000.V0"),
        json!("５000000000"),
    ] {
        let mut raw = document().raw;
        raw["list_metadata"]["views"][1]["position"] = value;
        let doc = Document::parse(raw).unwrap();
        assert!(capture(&doc, "V_β", ViewReorderDirection::Down).is_err());
    }
    let mut raw = document().raw;
    raw["list_metadata"]["views"][1]
        .as_object_mut()
        .unwrap()
        .remove("position");
    assert!(ViewReorder::current_order(&Document::parse(raw).unwrap()).is_err());
}
#[test]
fn interleaved_builtin_between_adjacent_custom_targets_refuses() {
    let mut raw = document().raw;
    raw["list_metadata"]["views"][3]["position"] = json!("5000000000.V");
    let doc = Document::parse(raw).unwrap();
    assert!(capture(&doc, "V_A", ViewReorderDirection::Down).is_err());
    assert!(capture(&doc, "V_β", ViewReorderDirection::Up).is_err());
    let mut raw = document().raw;
    raw["list_metadata"]["views"][2]["type"] = json!("future_layout");
    assert!(
        capture(
            &Document::parse(raw).unwrap(),
            "V_A",
            ViewReorderDirection::Down
        )
        .is_err()
    );
}
#[test]
fn known_nonswitcher_views_need_no_position_and_are_preserved_in_full_readback() {
    let mut raw = document().raw;
    let record = json!({"id":"V_RECORD","type":"record","opaque":{"keep":[1,null]}});
    let inline = json!({"id":"V_INLINE","type":"canvas_inline","name":null,"columns":[{"key":"unknown"}],"opaque":"keep"});
    raw["list_metadata"]["views"]
        .as_array_mut()
        .unwrap()
        .insert(1, record.clone());
    raw["list_metadata"]["views"]
        .as_array_mut()
        .unwrap()
        .insert(3, inline.clone());
    let doc = Document::parse(raw).unwrap();
    assert_eq!(
        ViewReorder::current_order(&doc).unwrap(),
        ["V_A", "V_β", "V_C"]
    );
    assert!(ViewReorder::choices(&doc, "V_RECORD").is_empty());
    assert!(ViewReorder::choices(&doc, "V_INLINE").is_empty());
    let intent = capture(&doc, "V_β", ViewReorderDirection::Down).unwrap();
    assert_eq!(intent.desired_position(), "5000000002.V");
    let fresh = desired(&intent);
    assert_eq!(fresh.views[1], record);
    assert_eq!(fresh.views[3], inline);
    assert!(intent.observes_desired(&fresh));
    let mut changed = fresh.raw;
    changed["list_metadata"]["views"][1]["opaque"] = json!({"keep":false});
    assert!(!intent.observes_desired(&Document::parse(changed).unwrap()));
    let mut collision = doc.raw;
    collision["list_metadata"]["views"][1]["id"] = json!("V_β");
    // Identity integrity applies to every raw view before switcher filtering.
    // This collision is rejected by the parser, not a valid capture input.
    assert_eq!(
        Document::parse(collision).unwrap_err(),
        ListError("List views have ambiguous identities.")
    );
}
#[test]
fn fresh_policy_and_target_eligibility_are_required() {
    let doc = document();
    assert!(
        ViewReorder::capture(
            &doc,
            "V_A",
            ViewReorderDirection::Down,
            None,
            Some(&personal())
        )
        .is_err()
    );
    assert!(
        ViewReorder::capture(&doc, "V_A", ViewReorderDirection::Down, Some("full"), None).is_err()
    );
    for (key, value) in [
        ("is_locked", json!(true)),
        ("type", json!("future_layout")),
        ("default_view_key", json!("all_items")),
    ] {
        let mut raw = doc.raw.clone();
        raw["list_metadata"]["views"][1][key] = value;
        assert!(
            capture(
                &Document::parse(raw).unwrap(),
                "V_A",
                ViewReorderDirection::Down
            )
            .is_err()
        );
    }
    let mut raw = doc.raw.clone();
    raw["access"] = json!("read");
    assert!(
        capture(
            &Document::parse(raw).unwrap(),
            "V_A",
            ViewReorderDirection::Down
        )
        .is_err()
    );
}
#[test]
fn exact_preflight_allows_only_existing_last_read_exception() {
    let doc = document();
    let intent = capture(&doc, "V_β", ViewReorderDirection::Down).unwrap();
    let mut raw = doc.raw.clone();
    raw["last_read"] = json!(999);
    assert!(
        intent
            .revalidate(
                &Document::parse(raw).unwrap(),
                Some("full"),
                Some(&personal())
            )
            .is_ok()
    );
    for key in ["title", "access", "opaque_file", "created", "user"] {
        let mut raw = doc.raw.clone();
        raw[key] = json!("changed");
        if let Ok(fresh) = Document::parse(raw) {
            assert!(
                intent
                    .revalidate(&fresh, Some("full"), Some(&personal()))
                    .is_err()
            );
        }
    }
    let mut forged = doc.clone();
    forged.views[0]["position"] = json!("5000000005");
    assert!(capture(&forged, "V_β", ViewReorderDirection::Down).is_err());
}
#[test]
fn readback_rejects_every_unrelated_metadata_change_and_raw_array_reorder() {
    let intent = capture(&document(), "V_β", ViewReorderDirection::Down).unwrap();
    for case in 0..6 {
        let mut raw = desired(&intent).raw;
        match case {
            0 => raw["list_metadata"]["views"][0]["filters"] = json!([]),
            1 => raw["list_metadata"]["views"]
                .as_array_mut()
                .unwrap()
                .swap(0, 1),
            2 => raw["list_metadata"]["opaque"] = json!(null),
            3 => raw["list_metadata"]["default_view"] = json!("V_C"),
            4 => raw["created"] = json!(18),
            _ => raw["list_metadata"]["schema"][0]["name"] = json!("Changed"),
        }
        assert!(!intent.observes_desired(&Document::parse(raw).unwrap()));
    }
    let mut raw = desired(&intent).raw;
    raw["access"] = json!("read");
    raw["last_read"] = json!(19);
    assert!(intent.observes_desired(&Document::parse(raw).unwrap()));
}
#[test]
fn one_use_permit_requires_current_metadata_and_explicit_observation() {
    let doc = document();
    let intent = capture(&doc, "V_β", ViewReorderDirection::Down).unwrap();
    let desired = desired(&intent);
    let mut attempt = ViewReorderAttempt::new(intent);
    assert!(!attempt.observe(&desired));
    assert!(!attempt.finish(false));
    assert!(attempt.admit(&doc, Some("full"), None).is_err());
    let _permit = attempt
        .admit(&doc, Some("full"), Some(&personal()))
        .unwrap();
    assert!(
        attempt
            .admit(&doc, Some("full"), Some(&personal()))
            .is_err()
    );
    assert!(attempt.finish(false));
    assert_eq!(attempt.phase(), ViewReorderPhase::Unconfirmed);
    assert!(!attempt.observe(&doc));
    assert!(attempt.observe(&desired));
    assert_eq!(attempt.phase(), ViewReorderPhase::Observed);
    assert!(!attempt.observe(&desired));
}
#[test]
fn not_requested_is_terminal_and_cannot_rearm_or_claim_observation() {
    let doc = document();
    let intent = capture(&doc, "V_A", ViewReorderDirection::Down).unwrap();
    let desired = desired(&intent);
    let mut attempt = ViewReorderAttempt::new(intent);
    let _permit = attempt
        .admit(&doc, Some("full"), Some(&personal()))
        .unwrap();
    assert!(attempt.finish(true));
    assert_eq!(attempt.phase(), ViewReorderPhase::NotRequested);
    assert!(
        attempt
            .admit(&doc, Some("full"), Some(&personal()))
            .is_err()
    );
    assert!(!attempt.observe(&desired));
}
