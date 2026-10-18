use super::*;
use crate::slack_lists::{
    BoardChoiceAction, BoardChoiceChange, BoardGroupDelete, BoardRecordMove, RecordsPage,
    ViewFilterChoice, saved_views::ProjectionContext,
};
fn doc(shared: bool) -> Document {
    let mut raw = json!({"id":"F1","mode":"list","user":"U1","created":1,"access":"write","list_metadata":{"opaque":[1,null,true],"schema":[
        {"id":"C_NAME","key":"name","name":"Name","type":"text"},
        {"id":"C_STATUS","key":"status","name":"Status","type":"select","options":{"format":"single_select","choices":[{"value":"A","label":"A"},{"value":"B","label":"B"}]}},
        {"id":"C_PHASE","key":"phase","name":"Phase","type":"select","options":{"format":"single_select","opaque":"keep","choices":[{"value":"X","label":"X"},{"value":"Y","label":"Y"}]}},
        {"id":"C_CHECK","key":"done","name":"Done","type":"checkbox"}],"views":[{"id":"V1","name":"Board","type":"board","columns":[{"key":"name","visible":true}],"grouping":{"group_by":"status","opaque":{"keep":7},"order":[{"select":["B"]},{"select":["A"]}]},"filters":[],"sorts":[]}]}});
    if shared {
        raw["list_metadata"]["views"][0]["grouping"]["group_by_column_id"] = json!("C_STATUS");
    }
    Document::parse(raw).unwrap()
}
fn page() -> RecordsPage {
    RecordsPage::parse(json!({"ok":true,"records":[
        {"id":"R1","list_id":"F1","position":"0000000001","fields":[{"key":"name","text":["One"]},{"key":"status","select":["A"]},{"key":"phase","select":["X"]},{"key":"done","checkbox":false}]},
        {"id":"R2","list_id":"F1","position":"0000000002","fields":[{"key":"name","text":["Two"]},{"key":"status","select":["B"]},{"key":"phase","select":["X"]},{"key":"done","checkbox":true}]},
        {"id":"R3","list_id":"F1","position":"0000000003","fields":[{"key":"name","text":["Three"]},{"key":"status","select":["A"]},{"key":"phase","select":["Y"]},{"key":"done","checkbox":false}]}],"response_metadata":{"next_cursor":"opaque-tail","opaque":7}})).unwrap()
}
fn personal() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
#[test]
fn shared_move_uses_displayed_checkbox_order_but_spreads_canonical_select_grouping() {
    let d = doc(true);
    let local = ViewRefinementDraft::capture(&d, "V1")
        .unwrap()
        .with_grouping(&d, Some("C_CHECK"))
        .unwrap();
    assert!(BoardLaneOrder::refined_uses_shared(&d, &local).unwrap());
    assert_eq!(
        BoardLaneOrder::refined_order(&d, &local, None).unwrap(),
        vec!["", "true"]
    );
    let read = BoardLaneOrder::refined_projection_document(&d, &local, None).unwrap();
    let catalog = crate::slack_lists::saved_views::catalog(&read).unwrap();
    let projected = crate::slack_lists::saved_views::project(
        &read,
        &page(),
        &catalog[0],
        &ProjectionContext::default(),
    )
    .unwrap();
    assert_eq!(
        projected
            .groups
            .iter()
            .map(|g| g.key.clone().unwrap_or_default())
            .collect::<Vec<_>>(),
        vec!["", "true"]
    );
    assert_eq!(
        d.views[0]["grouping"]["order"],
        json!([{"select":["B"]},{"select":["A"]}])
    );
    let intent = BoardLaneOrder::capture_refined(
        &d,
        &local,
        None,
        "true",
        "",
        BoardLanePlacement::Before,
        Some("full"),
        Some(&personal()),
    )
    .unwrap();
    assert_eq!(
        intent.grouping(),
        &json!({"group_by":"status","group_by_column_id":"C_STATUS","opaque":{"keep":7},"order":[{"checkbox":true},{"checkbox":false}]})
    );
    assert_eq!(intent.original(), &d);
    assert!(
        intent
            .revalidate(&d, Some("full"), Some(&personal()))
            .is_ok()
    );
    let mut raw = d.raw.clone();
    raw["list_metadata"]["views"][0]["grouping"] = intent.grouping().clone();
    assert!(intent.observes_desired(&Document::parse(raw).unwrap()));
    assert!(!intent.observes_desired(&local.projection_document(&d).unwrap()));
}
#[test]
fn local_route_uses_canonical_missing_id_and_keeps_saved_order_fallback_for_new_group() {
    let d = doc(false);
    let local = ViewRefinementDraft::capture(&d, "V1")
        .unwrap()
        .with_grouping(&d, Some("C_CHECK"))
        .unwrap();
    assert!(!BoardLaneOrder::refined_uses_shared(&d, &local).unwrap());
    let provider = BoardLaneOrderDraft::moved_refined(
        &d,
        &local,
        None,
        "true",
        "",
        BoardLanePlacement::Before,
        Some("full"),
        Some(&personal()),
    )
    .unwrap();
    assert_eq!(
        provider.typed_order(),
        &[json!({"checkbox":true}), json!({"checkbox":false})]
    );
    assert!(
        BoardLaneOrder::capture_refined(
            &d,
            &local,
            None,
            "true",
            "",
            BoardLanePlacement::Before,
            Some("full"),
            Some(&personal())
        )
        .is_err()
    );
    let read = BoardLaneOrder::refined_projection_document(&d, &local, Some(&provider)).unwrap();
    assert_eq!(read.views[0]["grouping"]["group_by"], "done");
    assert_eq!(d.views[0]["grouping"]["group_by"], "status");
}
#[test]
fn refined_card_updates_current_group_cell_and_preserves_canonical_view_metadata() {
    let d = doc(true);
    let p = page();
    let local = ViewRefinementDraft::capture(&d, "V1")
        .unwrap()
        .with_grouping(&d, Some("C_CHECK"))
        .unwrap();
    let i = BoardRecordMove::capture_refined(
        &d,
        &p,
        &local,
        None,
        &ProjectionContext::default(),
        "R2",
        "true",
        "",
        Some("R3"),
        Some("full"),
        Some(&personal()),
    )
    .unwrap();
    assert_eq!(i.original(), &d);
    assert_eq!(i.cells().unwrap()[0]["column_id"], "C_CHECK");
    assert!(
        i.revalidate(
            &d,
            &p,
            &ProjectionContext::default(),
            Some("full"),
            Some(&personal())
        )
        .is_ok()
    );
    let mut changed = d.raw.clone();
    changed["list_metadata"]["opaque"] = json!("changed");
    assert!(
        i.revalidate(
            &Document::parse(changed).unwrap(),
            &p,
            &ProjectionContext::default(),
            Some("full"),
            Some(&personal())
        )
        .is_err()
    );
}
#[test]
fn choice_and_delete_use_current_phase_column_and_only_locally_filtered_members() {
    let d = doc(true);
    let p = page();
    let local = ViewRefinementDraft::capture(&d, "V1")
        .unwrap()
        .with_grouping(&d, Some("C_PHASE"))
        .unwrap()
        .with_filter_choice(
            &d,
            &ViewFilterChoice::Checkbox {
                column_id: "C_CHECK".into(),
                checked: true,
            },
        )
        .unwrap();
    let choice = BoardChoiceChange::capture_refined(
        &d,
        &local,
        BoardChoiceAction::Rename {
            value: "X".into(),
            label: "Renamed".into(),
        },
        Some("full"),
        Some(&personal()),
    )
    .unwrap();
    assert_eq!(choice.column_id(), "C_PHASE");
    assert_eq!(choice.original(), &d);
    let delete = BoardGroupDelete::capture_refined(
        &d,
        &p,
        &local,
        "X",
        &ProjectionContext::default(),
        Some("full"),
        Some(&personal()),
    )
    .unwrap();
    assert_eq!(delete.original(), &d);
    assert_eq!(delete.cells().len(), 1);
    assert_eq!(delete.cells()[0]["row_id"], "R2");
    assert_eq!(delete.cells()[0]["column_id"], "C_PHASE");
    assert!(
        delete
            .revalidate(
                &d,
                &p,
                &ProjectionContext::default(),
                Some("full"),
                Some(&personal())
            )
            .is_ok()
    );
    assert_eq!(p.raw["response_metadata"]["next_cursor"], "opaque-tail");
}

#[test]
fn actual_display_and_move_order_use_javascript_utf16_label_comparison() {
    let mut raw = doc(true).raw;
    raw["list_metadata"]["schema"][1]["options"]["choices"][0]["label"] = json!("\u{10000}");
    raw["list_metadata"]["schema"][1]["options"]["choices"][1]["label"] = json!("\u{e000}");
    raw["list_metadata"]["views"][0]["grouping"]["order"] = json!([]);
    raw["list_metadata"]["views"][0]["sorts"] = json!([{"key":"status","ascending":true}]);
    let d = Document::parse(raw).unwrap();
    let local = ViewRefinementDraft::capture(&d, "V1").unwrap();
    assert_eq!(
        BoardLaneOrder::refined_order(&d, &local, None).unwrap(),
        vec!["", "A", "B"]
    );
    let selected = crate::slack_lists::saved_views::SavedView::parse(d.views[0].clone()).unwrap();
    let p = crate::slack_lists::saved_views::project(
        &d,
        &page(),
        &selected,
        &ProjectionContext::default(),
    )
    .unwrap();
    let keys = p
        .groups
        .iter()
        .filter_map(|g| g.key.as_deref())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    assert_eq!(keys, vec!["A", "B"]);
}
