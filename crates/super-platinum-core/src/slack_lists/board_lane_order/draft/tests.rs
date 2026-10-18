use super::*;
fn doc(kind: &str, shared: bool) -> Document {
    let mut raw = super::super::tests::document(kind).raw;
    if !shared {
        raw["list_metadata"]["views"][0]["grouping"]
            .as_object_mut()
            .unwrap()
            .remove("group_by_column_id");
    }
    Document::parse(raw).unwrap()
}
fn personal() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn local(d: &Document) -> BoardLaneOrderDraft {
    BoardLaneOrderDraft::moved(
        d,
        "V1",
        None,
        "β",
        "a",
        BoardLanePlacement::Before,
        Some("full"),
        Some(&personal()),
    )
    .unwrap()
}
#[test]
fn local_movement_owns_typed_cells_and_only_ephemeral_projection_document_changes() {
    let d = doc("select", false);
    let original = d.clone();
    let draft = local(&d);
    assert!(!BoardLaneOrderDraft::uses_shared_order(&d, "V1").unwrap());
    assert_eq!(
        draft.typed_order(),
        &[
            json!({"select":[]}),
            json!({"select":["β"]}),
            json!({"select":["a"]})
        ]
    );
    let mut projected = draft.projection_document(&d, "V1").unwrap().raw;
    projected["list_metadata"]["views"][0]["grouping"]
        .as_object_mut()
        .unwrap()
        .remove("order");
    assert_eq!(projected, d.raw);
    assert_eq!(d, original);
    assert!(
        BoardLaneOrder::capture(
            &d,
            "V1",
            "β",
            "a",
            BoardLanePlacement::Before,
            Some("full"),
            Some(&personal())
        )
        .is_err()
    );
}
#[test]
fn draft_converts_under_current_grouping_type_like_actual_b_utility() {
    let d = doc("select", false);
    let selected = local(&d);
    let check = doc("checkbox", false);
    assert_eq!(
        BoardLaneOrderDraft::current_order(&check, "V1", Some(&selected)).unwrap(),
        ["", "true"]
    );
    let boolean = BoardLaneOrderDraft::moved(
        &check,
        "V1",
        None,
        "true",
        "",
        BoardLanePlacement::Before,
        Some("full"),
        Some(&personal()),
    )
    .unwrap();
    assert_eq!(
        boolean.typed_order(),
        &[json!({"checkbox":true}), json!({"checkbox":false})]
    );
    assert_eq!(
        BoardLaneOrderDraft::current_order(&d, "V1", Some(&boolean)).unwrap(),
        ["", "a", "β"]
    );
}
#[test]
fn repeated_local_moves_and_deleted_choices_use_current_choices_without_changing_raw() {
    let d = doc("select", false);
    let first = local(&d);
    let second = BoardLaneOrderDraft::moved(
        &d,
        "V1",
        Some(&first),
        "a",
        "β",
        BoardLanePlacement::Before,
        Some("full"),
        Some(&personal()),
    )
    .unwrap();
    assert_eq!(
        BoardLaneOrderDraft::current_order(&d, "V1", Some(&second)).unwrap(),
        ["", "a", "β"]
    );
    let mut raw = d.raw.clone();
    raw["list_metadata"]["schema"][0]["options"]["choices"]
        .as_array_mut()
        .unwrap()
        .retain(|v| v["value"] != "β");
    let changed = Document::parse(raw).unwrap();
    assert_eq!(
        BoardLaneOrderDraft::current_order(&changed, "V1", Some(&first)).unwrap(),
        ["", "a"]
    );
}
#[test]
fn shared_bridge_uses_owned_effective_starting_order_and_full_raw_preflight_readback() {
    let local_doc = doc("select", false);
    let draft = local(&local_doc);
    let shared = doc("select", true);
    assert!(BoardLaneOrderDraft::uses_shared_order(&shared, "V1").unwrap());
    assert!(
        BoardLaneOrderDraft::moved(
            &shared,
            "V1",
            Some(&draft),
            "a",
            "β",
            BoardLanePlacement::Before,
            Some("full"),
            Some(&personal())
        )
        .is_err()
    );
    let intent = BoardLaneOrder::capture_with_draft(
        &shared,
        "V1",
        "a",
        "β",
        BoardLanePlacement::Before,
        Some(&draft),
        Some("full"),
        Some(&personal()),
    )
    .unwrap();
    assert_eq!(intent.desired_order(), ["", "a", "β"]);
    assert!(
        intent
            .revalidate(&shared, Some("full"), Some(&personal()))
            .is_ok()
    );
    let mut raw = shared.raw.clone();
    raw["list_metadata"]["views"][0]["grouping"] = intent.grouping().clone();
    assert!(intent.observes_desired(&Document::parse(raw).unwrap()));
    let mut raw = shared.raw.clone();
    raw["list_metadata"]["opaque"] = json!("changed");
    assert!(
        intent
            .revalidate(
                &Document::parse(raw).unwrap(),
                Some("full"),
                Some(&personal())
            )
            .is_err()
    );
}
#[test]
fn empty_missing_null_ids_are_local_but_malformed_or_wrong_ids_never_author() {
    for value in [Value::Null, json!("")] {
        let mut raw = doc("select", false).raw;
        raw["list_metadata"]["views"][0]["grouping"]["group_by_column_id"] = value;
        let d = Document::parse(raw).unwrap();
        assert!(!BoardLaneOrderDraft::uses_shared_order(&d, "V1").unwrap());
        assert_eq!(local(&d).typed_order().len(), 3);
    }
    for value in [json!(false), json!(5), json!("C_OTHER"), json!([])] {
        let mut raw = doc("select", false).raw;
        raw["list_metadata"]["views"][0]["grouping"]["group_by_column_id"] = value;
        assert!(
            BoardLaneOrderDraft::uses_shared_order(&Document::parse(raw).unwrap(), "V1").is_err()
        );
    }
}
#[test]
fn local_authoring_requires_current_write_policy_and_preserves_non_switcher_siblings() {
    let mut raw = doc("select", false).raw;
    raw["list_metadata"]["views"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"R","type":"record","opaque":[true,null]}));
    let d = Document::parse(raw).unwrap();
    let draft = local(&d);
    assert_eq!(
        draft.projection_document(&d, "V1").unwrap().views[2],
        d.views[2]
    );
    assert!(
        BoardLaneOrderDraft::moved(
            &d,
            "V1",
            None,
            "β",
            "a",
            BoardLanePlacement::Before,
            Some("disabled"),
            Some(&personal())
        )
        .is_err()
    );
    assert!(
        BoardLaneOrderDraft::moved(
            &d,
            "V1",
            None,
            "β",
            "a",
            BoardLanePlacement::Before,
            Some("full"),
            None
        )
        .is_err()
    );
}

#[test]
fn read_overlay_preserves_ungrouped_boards_and_does_not_author_todo_grouping() {
    let draft = local(&doc("select", false));
    for grouping in [
        None,
        Some(Value::Null),
        Some(json!({})),
        Some(json!({"group_by":""})),
    ] {
        let mut raw = doc("select", false).raw;
        let view = raw["list_metadata"]["views"][0].as_object_mut().unwrap();
        view.remove("grouping");
        if let Some(grouping) = grouping {
            view.insert("grouping".into(), grouping);
        }
        let ungrouped = Document::parse(raw).unwrap();
        assert_eq!(
            draft.projection_document(&ungrouped, "V1").unwrap(),
            ungrouped
        );
        assert!(BoardLaneOrderDraft::current_order(&ungrouped, "V1", Some(&draft)).is_err());
    }
    let mut raw = doc("todo_completed", false).raw;
    raw["list_metadata"]["schema"][0]["key"] = json!("todo_completed");
    raw["list_metadata"]["views"][0]["grouping"]["group_by"] = json!("todo_completed");
    let todo = Document::parse(raw).unwrap();
    let mut projected = draft.projection_document(&todo, "V1").unwrap().raw;
    assert_eq!(
        projected["list_metadata"]["views"][0]["grouping"]["order"],
        json!(draft.typed_order())
    );
    projected["list_metadata"]["views"][0]["grouping"]
        .as_object_mut()
        .unwrap()
        .remove("order");
    assert_eq!(projected, todo.raw);
    assert!(
        BoardLaneOrderDraft::moved(
            &todo,
            "V1",
            Some(&draft),
            "true",
            "",
            BoardLanePlacement::Before,
            Some("full"),
            Some(&personal())
        )
        .is_err()
    );
    assert!(
        BoardLaneOrder::capture_with_draft(
            &todo,
            "V1",
            "true",
            "",
            BoardLanePlacement::Before,
            Some(&draft),
            Some("full"),
            Some(&personal())
        )
        .is_err()
    );
}
#[test]
fn read_overlay_still_refuses_ambiguous_identity_and_keeps_malformed_grouping_for_reader() {
    let d = doc("select", false);
    let draft = local(&d);
    let mut raw = d.raw.clone();
    raw["list_metadata"]["views"][1]["id"] = json!("V1");
    // Ambiguous raw identity is rejected before a projection can be made.
    assert_eq!(
        Document::parse(raw).unwrap_err(),
        ListError("List views have ambiguous identities.")
    );
    assert!(draft.projection_document(&d, "V2").is_err());
    for grouping in [json!(false), json!([]), json!({"group_by":7})] {
        let mut raw = d.raw.clone();
        raw["list_metadata"]["views"][0]["grouping"] = grouping;
        let malformed = Document::parse(raw).unwrap();
        assert_eq!(
            draft.projection_document(&malformed, "V1").unwrap(),
            malformed
        );
    }
}
