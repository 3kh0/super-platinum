use super::*;
fn document() -> Document {
    Document::parse(json!({"id":"F1","mode":"list","access":"write","opaque_file":{"keep":true},"list_metadata":{
        "opaque_metadata":[null,1,true],"schema":[
            {"id":"C_NAME","key":"name","name":"Name","type":"text"},
            {"id":"C_STATUS","key":"status","name":"Status","type":"select","options":{"format":"single_select","choices":[{"value":"A","label":"Alpha"},{"value":"B","label":"Beta"}]}}
        ],"views":[
            {"id":"V1","name":"Delivery","type":"board","columns":[{"id":"C_NAME","key":"name","visible":true,"width":240,"opaque":true},{"id":"C_STATUS","key":"status","visible":false}],"grouping":{"group_by":"status","group_by_column_id":"C_STATUS","opaque":{"keep":true}},"filters":[],"sorts":[],"opaque_view":5},
            {"id":"V2","name":"Planning","type":"table","opaque_sibling":[1,2,3]}
        ]}})).unwrap()
}
#[test]
fn new_draft_does_not_invent_changes_or_rewrite_raw_metadata() {
    let doc = document();
    let draft = ViewRefinementDraft::capture(&doc, "V1").unwrap();
    assert!(!draft.changes(&doc).unwrap().any());
    assert_eq!(draft.projection_document(&doc).unwrap(), doc);
    // Provider order has no input to this API: an order-only move cannot expose Save.
    assert!(!ViewRefinementChanges::default().any());
}
#[test]
fn same_group_key_is_not_dirty_but_changed_key_or_clear_is() {
    let doc = document();
    let original = ViewRefinementDraft::capture(&doc, "V1").unwrap();
    let same = original.with_grouping(&doc, Some("C_STATUS")).unwrap();
    assert!(!same.changes(&doc).unwrap().grouping);
    assert_eq!(
        same.projection_document(&doc).unwrap().views[0]["grouping"],
        json!({"group_by":"status"})
    );
    assert!(
        original
            .with_grouping(&doc, None)
            .unwrap()
            .changes(&doc)
            .unwrap()
            .grouping
    );
    assert_eq!(doc.views[0]["grouping"]["opaque"], json!({"keep":true}));
}
#[test]
fn source_sort_comparison_ignores_id_and_defaults_null_direction_but_keeps_order() {
    let mut raw = document().raw;
    raw["list_metadata"]["views"][0]["sorts"] =
        json!([{"key":"name","column_id":"OLD","ascending":false}]);
    let doc = Document::parse(raw).unwrap();
    let draft = ViewRefinementDraft::capture(&doc, "V1").unwrap();
    assert!(
        !draft
            .with_sorts(
                &doc,
                vec![json!({"key":"name","column_id":"NEW","ascending":null})]
            )
            .unwrap()
            .changes(&doc)
            .unwrap()
            .sorts
    );
    assert!(
        draft
            .with_sorts(
                &doc,
                vec![
                    json!({"key":"status","ascending":true}),
                    json!({"key":"name","ascending":false})
                ]
            )
            .unwrap()
            .changes(&doc)
            .unwrap()
            .sorts
    );
    assert_ne!(
        normalized(&[json!({"key":"a"}), json!({"key":"b"})], false).unwrap(),
        normalized(&[json!({"key":"b"}), json!({"key":"a"})], false).unwrap()
    );
}
#[test]
fn source_filter_comparison_defaults_arrays_ignores_id_and_preserves_values_order() {
    let mut raw = document().raw;
    raw["list_metadata"]["views"][0]["filters"] =
        json!([{"key":"status","operator":"is","column_id":"OLD"}]);
    let doc = Document::parse(raw).unwrap();
    let draft = ViewRefinementDraft::capture(&doc, "V1").unwrap();
    assert!(!draft.with_filters(&doc, vec![json!({"key":"status","operator":"is","column_id":"NEW","values":[],"typed_values":[]})]).unwrap().changes(&doc).unwrap().filters);
    assert_ne!(
        normalized(&[json!({"key":"status","values":["A","B"]})], true).unwrap(),
        normalized(&[json!({"key":"status","values":["B","A"]})], true).unwrap()
    );
    assert_ne!(
        normalized(
            &[json!({"key":"status","values":["A"],"opaque":true})],
            true
        )
        .unwrap(),
        normalized(&[json!({"key":"status","values":["A"]})], true).unwrap()
    );
}
#[test]
fn combined_local_projection_changes_only_selected_fields_and_keeps_canonical_source() {
    let doc = document();
    let untouched = doc.clone();
    let draft = ViewRefinementDraft::capture(&doc, "V1")
        .unwrap()
        .with_layout(&doc, ViewLayout::Table)
        .unwrap()
        .with_visibility(&doc, "C_STATUS", true)
        .unwrap()
        .with_completed(&doc, true)
        .unwrap()
        .with_filters(
            &doc,
            vec![json!({"key":"status","operator":"is","values":["A"]})],
        )
        .unwrap();
    assert_eq!(
        draft.changes(&doc).unwrap(),
        ViewRefinementChanges {
            layout: true,
            visibility: true,
            completed: true,
            filters: true,
            ..Default::default()
        }
    );
    let result = draft.projection_document(&doc).unwrap();
    let mut expected = doc.raw.clone();
    expected["list_metadata"]["views"][0]["type"] = json!("table");
    expected["list_metadata"]["views"][0]["columns"][1]["visible"] = json!(true);
    expected["list_metadata"]["views"][0]["show_completed_items"] = json!(true);
    expected["list_metadata"]["views"][0]["filters"] =
        json!([{"key":"status","operator":"is","values":["A"]}]);
    expected["list_metadata"]["views"][0]["info_column_filters"] = json!([]);
    assert_eq!(result.raw, expected);
    assert_eq!(doc, untouched);
}
#[test]
fn current_server_matching_local_values_hides_save_without_erasing_local_state() {
    let doc = document();
    let draft = ViewRefinementDraft::capture(&doc, "V1")
        .unwrap()
        .with_layout(&doc, ViewLayout::Table)
        .unwrap();
    assert!(draft.changes(&doc).unwrap().layout);
    let current = draft.projection_document(&doc).unwrap();
    assert!(!draft.changes(&current).unwrap().any());
    assert_eq!(draft.layout, Some(ViewLayout::Table));
}
#[test]
fn foreign_list_and_forged_representation_never_receive_local_projection() {
    let doc = document();
    let draft = ViewRefinementDraft::capture(&doc, "V1").unwrap();
    let mut raw = doc.raw.clone();
    raw["id"] = json!("F_OTHER");
    assert!(
        draft
            .projection_document(&Document::parse(raw).unwrap())
            .is_err()
    );
    let mut forged = doc.clone();
    forged.id = "F_FORGED".into();
    assert!(ViewRefinementDraft::capture(&forged, "V1").is_err());
}
#[test]
fn malformed_rules_or_ambiguous_visibility_refuse_without_mutating_original() {
    let doc = document();
    let draft = ViewRefinementDraft::capture(&doc, "V1").unwrap();
    for rule in [
        json!(null),
        json!({"key":"unknown","ascending":true}),
        json!({"key":"name","ascending":"true"}),
    ] {
        assert!(draft.with_sorts(&doc, vec![rule]).is_err());
    }
    assert!(
        draft
            .with_filters(&doc, vec![json!({"key":"name","operator":true})])
            .is_err()
    );
    let mut raw = doc.raw.clone();
    let duplicate = raw["list_metadata"]["views"][0]["columns"][0].clone();
    raw["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    let ambiguous = Document::parse(raw).unwrap();
    assert!(draft.with_visibility(&ambiguous, "C_NAME", false).is_err());
    assert!(!draft.changes(&doc).unwrap().any());
}

#[test]
fn non_string_filter_arrays_refuse_instead_of_losing_js_serialization_identity() {
    let doc = document();
    let draft = ViewRefinementDraft::capture(&doc, "V1").unwrap();
    for field in ["values", "typed_values"] {
        for value in [
            json!([1]),
            json!([1.0]),
            json!([true]),
            json!([null]),
            json!([{"a":1,"b":2}]),
        ] {
            let mut row = json!({"key":"status","operator":"is","values":[]});
            row[field] = value;
            assert!(draft.with_filters(&doc, vec![row]).is_err());
        }
    }
    let mut raw = doc.raw.clone();
    raw["list_metadata"]["views"][0]["filters"] =
        json!([{"key":"status","operator":"is","values":[{"a":1,"b":2}]}]);
    let original = Document::parse(raw).unwrap();
    let changed = draft.with_filters(&original, vec![]).unwrap();
    assert!(changed.changes(&original).is_err());
    assert_eq!(draft.projection_document(&original).unwrap(), original);
}
#[test]
fn dirty_comparison_uses_js_numeric_equality_without_rewriting_raw_numbers() {
    let mut raw = document().raw;
    raw["list_metadata"]["views"][0]["sorts"] =
        json!([{"key":"name","ascending":true,"opaque":[1,9007199254740993u64]}]);
    let doc = Document::parse(raw).unwrap();
    let draft = ViewRefinementDraft::capture(&doc, "V1")
        .unwrap()
        .with_sorts(
            &doc,
            vec![json!({"key":"name","ascending":true,"opaque":[1.0,9007199254740992u64]})],
        )
        .unwrap();
    assert!(!draft.changes(&doc).unwrap().sorts);
    assert_eq!(
        doc.views[0]["sorts"][0]["opaque"][1],
        json!(9007199254740993u64)
    );
    assert!(!js_equal(&json!(true), &json!(1)));
    assert!(js_equal(&json!({"x":[1]}), &json!({"x":[1.0]})));
}

#[test]
fn typed_local_choices_reuse_validation_without_granting_shared_access() {
    use crate::slack_lists::{ViewFilterChoice, ViewSortChoice};
    let mut raw = document().raw;
    raw["access"] = json!("read");
    let doc = Document::parse(raw).unwrap();
    let draft = ViewRefinementDraft::capture(&doc, "V1").unwrap();
    let desired = ViewSortChoice::Field {
        column_id: "C_NAME".into(),
        ascending: true,
    };
    let local = draft
        .with_filter_choice(&doc, &ViewFilterChoice::None)
        .unwrap()
        .with_sort_choice(&doc, &desired)
        .unwrap();
    let read = local.projection_document(&doc).unwrap();
    assert_eq!(
        read.views[0]["sorts"],
        json!([{"key":"name","ascending":true}])
    );
    assert_eq!(read.views[0]["opaque_view"], doc.views[0]["opaque_view"]);
    assert_eq!(read.views[1], doc.views[1]);
    assert_eq!(read.raw["access"], "read");
    assert!(
        draft
            .with_sort_choice(
                &doc,
                &ViewSortChoice::Number {
                    column_id: "C_NAME".into(),
                    ascending: true
                }
            )
            .is_err()
    );
    assert!(
        draft
            .with_filter_choice(
                &doc,
                &ViewFilterChoice::Checkbox {
                    column_id: "C_STATUS".into(),
                    checked: true
                }
            )
            .is_err()
    );
    assert!(crate::slack_lists::ViewSort::capture(&doc, "V1", desired, Some("all"), None).is_err());
}
