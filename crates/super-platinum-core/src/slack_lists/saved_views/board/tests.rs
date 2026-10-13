use super::super::project;
use super::*;
use serde_json::json;
fn fixture(patch: Value) -> (Document, RecordsPage, SavedView) {
    let mut view = json!({"id":"V1","name":"Board","type":"board","columns":[
        {"key":"n","id":"CN","visible":true},{"key":"name","id":"CT","visible":true},
        {"key":"stage","id":"CS","visible":true},{"key":"done","id":"CB","visible":true}],
        "filters":[],"sorts":[],"grouping":{"group_by":"stage","group_by_column_id":"CS"}});
    view.as_object_mut()
        .unwrap()
        .extend(patch.as_object().unwrap().clone());
    let doc=Document::parse(json!({"id":"F1","mode":"list","list_metadata":{"schema":[
        {"id":"CT","key":"name","name":"Name","type":"text","is_primary_column":true},
        {"id":"CN","key":"n","name":"Count","type":"number"},
        {"id":"CS","key":"stage","name":"Stage","type":"select","options":{"format":"multi_select","choices":[{"value":"A","label":"Alpha"},{"value":"B","label":"Beta"},{"value":"C","label":"Empty"}]}},
        {"id":"CB","key":"done","name":"Done","type":"checkbox"}
    ],"views":[view]}})).unwrap();
    let page=RecordsPage::parse(json!({"ok":true,"response_metadata":{"next_cursor":"more"},"records":[
        {"id":"R1","position":1,"fields":[{"key":"name","text":"One"},{"key":"stage","select":["A","B"]},{"key":"done","checkbox":true},{"key":"n","number":[2]}]},
        {"id":"R2","position":2,"fields":[{"key":"name","text":"Two"},{"key":"stage","select":["B"]},{"key":"done","checkbox":false},{"key":"n","number":[1]}]},
        {"id":"R3","position":3,"fields":[{"key":"name","text":"Three"},{"key":"stage","select":["deleted"]},{"key":"n","number":[0]}]}
    ]})).unwrap();
    let view = SavedView::parse(doc.views[0].clone()).unwrap();
    (doc, page, view)
}
fn project_fixture(patch: Value, editable: bool) -> Projection {
    let (doc, page, view) = fixture(patch);
    project(
        &doc,
        &page,
        &view,
        &ProjectionContext {
            editable,
            ..Default::default()
        },
    )
    .unwrap()
}
fn keys(p: &Projection) -> Vec<&str> {
    p.groups.iter().map(|g| g.key.as_deref().unwrap()).collect()
}
fn rows(group: &Group) -> Vec<&str> {
    group.rows.iter().map(|r| r.record_id.as_str()).collect()
}
#[test]
fn multi_membership_card_columns_and_empty_lanes_are_board_specific() {
    for editable in [false, true] {
        let p = project_fixture(json!({}), editable);
        assert_eq!(p.layout, ProjectionLayout::Board);
        assert_eq!(p.column_ids, ["CT", "CN", "CB"]);
        assert_eq!(keys(&p), ["", "A", "B", "C"]);
        assert_eq!(rows(&p.groups[0]), ["R3"]);
        assert_eq!(rows(&p.groups[1]), ["R1"]);
        assert_eq!(rows(&p.groups[2]), ["R1", "R2"]);
        assert_eq!(p.groups[3].count, 0);
        assert_ne!(
            p.groups[1].rows[0].instance_id,
            p.groups[2].rows[0].instance_id
        );
        assert_eq!(p.unique_record_count, 3);
        assert!(p.incomplete);
    }
}
#[test]
fn typed_lane_order_ignores_deleted_and_duplicate_keys_then_appends_remaining() {
    let p = project_fixture(
        json!({"grouping":{"group_by":"stage","group_by_column_id":"CS","order":[
            {"select":["deleted"]},{"select":["B","A"]},{"select":["B"]},{"select":[]}
        ]}}),
        false,
    );
    assert_eq!(keys(&p), ["B", "", "A", "C"]);
    let p = project_fixture(
        json!({"grouping":{"group_by":"stage","order":[]},"sorts":[{"key":"stage","ascending":false}]}),
        false,
    );
    assert_eq!(keys(&p), ["", "C", "B", "A"]);
}
#[test]
fn unrelated_filters_keep_empty_lanes_but_group_filter_hides_populated_duplicates() {
    let p = project_fixture(
        json!({"filters":[{"key":"n","operator":"greater_than","values":[99]}]}),
        false,
    );
    assert_eq!(keys(&p), ["A", "B", "C"]);
    assert_eq!(p.unique_record_count, 0);
    let p = project_fixture(
        json!({"filters":[{"key":"stage","operator":"contains","values":["B"]}]}),
        false,
    );
    assert_eq!(keys(&p), ["B"]);
    assert_eq!(rows(&p.groups[0]), ["R1", "R2"]);
}
#[test]
fn unchecked_or_missing_membership_is_the_empty_lane_and_order_can_move_it() {
    let p = project_fixture(
        json!({"grouping":{"group_by":"done","order":[{"checkbox":true},{"checkbox":false}]}}),
        false,
    );
    assert_eq!(keys(&p), ["true", ""]);
    assert_eq!(rows(&p.groups[0]), ["R1"]);
    assert_eq!(rows(&p.groups[1]), ["R2", "R3"]);
    let (mut doc, mut page, view) = fixture(json!({"grouping":{"group_by":"done"}}));
    page.records.retain(|r| r.id != "R1");
    let p = project(&doc, &page, &view, &ProjectionContext::default()).unwrap();
    assert_eq!(keys(&p), ["", "true"]);
    assert_eq!(p.groups[1].count, 0);
    // Promotion never appends a hidden primary.
    doc.views[0]["columns"][1]["visible"] = json!(false);
    let hidden = SavedView::parse(doc.views[0].clone()).unwrap();
    assert!(
        !project(&doc, &page, &hidden, &ProjectionContext::default())
            .unwrap()
            .column_ids
            .contains(&"CT".into())
    );
}
#[test]
fn ungrouped_board_has_one_lane_and_empty_board_no_phantom_lane() {
    let (doc, mut page, view) = fixture(json!({"grouping":null}));
    let context = ProjectionContext::default();
    let p = project(&doc, &page, &view, &context).unwrap();
    assert_eq!(keys(&p), [""]);
    assert_eq!(rows(&p.groups[0]), ["R1", "R2", "R3"]);
    page.records.clear();
    assert!(
        project(&doc, &page, &view, &context)
            .unwrap()
            .groups
            .is_empty()
    );
}
#[test]
fn malformed_order_unsupported_group_and_stale_view_reject_before_display() {
    for patch in [
        json!({"grouping":{"group_by":"stage","order":["A"]}}),
        json!({"grouping":{"group_by":"stage","order":{"select":["A"]}}}),
        json!({"grouping":{"group_by":"done","order":[{"checkbox":"false"}]}}),
        json!({"grouping":{"group_by":"n"}}),
        json!({"grouping":{"group_by":"stage","group_by_column_id":"OLD"}}),
    ] {
        let (doc, mut page, view) = fixture(patch);
        page.records.clear();
        assert!(project(&doc, &page, &view, &ProjectionContext::default()).is_err());
    }
    let (doc, page, mut view) = fixture(json!({}));
    view.raw["name"] = json!("Edited");
    assert!(project(&doc, &page, &view, &ProjectionContext::default()).is_err());
}
#[test]
fn todo_board_has_live_open_completed_lanes_and_separate_header_edit_target() {
    let (doc, page, _) = fixture(
        json!({"show_completed_items":true,"grouping":{"group_by":"done","group_by_column_id":"CB"}}),
    );
    let mut raw = doc.raw;
    let schema = raw["list_metadata"]["schema"].as_array_mut().unwrap();
    let column = schema.iter_mut().find(|c| c["id"] == "CB").unwrap();
    column["key"] = json!("todo_completed");
    column["type"] = json!("todo_completed");
    raw["list_metadata"]["views"][0]["grouping"]["group_by"] = json!("todo_completed");
    raw["list_metadata"]["views"][0]["columns"][3]["key"] = json!("todo_completed");
    let mut doc = Document::parse(raw).unwrap();
    let view = SavedView::parse(doc.views[0].clone()).unwrap();
    let mut raw = page.raw;
    for record in raw["records"].as_array_mut().unwrap() {
        for field in record["fields"].as_array_mut().unwrap() {
            if field["key"] == "done" {
                field["key"] = json!("todo_completed");
            }
        }
    }
    let page = RecordsPage::parse(raw).unwrap();
    let context = ProjectionContext {
        todo_mode: true,
        apply_todo_completion_rules: Some(true),
        ..Default::default()
    };
    let p = project(&doc, &page, &view, &context).unwrap();
    assert_eq!(
        p.groups
            .iter()
            .map(|g| g.label.as_str())
            .collect::<Vec<_>>(),
        ["Open", "Completed"]
    );
    assert_eq!(p.board_completion_column_id.as_deref(), Some("CB"));
    assert!(!p.column_ids.contains(&"CB".into()));
    let mut context = context;
    context.collapsed_groups.insert("true".into());
    assert_eq!(
        project(&doc, &page, &view, &context).unwrap().groups[1]
            .rows
            .len(),
        1
    );
    // A header control is not inferred when the primary field is hidden.
    doc.views[0]["columns"][1]["visible"] = json!(false);
    let hidden = SavedView::parse(doc.views[0].clone()).unwrap();
    assert!(
        project(&doc, &page, &hidden, &context)
            .unwrap()
            .board_completion_column_id
            .is_none()
    );
    // Nor does an arbitrary same-kind column acquire canonical todo identity.
    let (mut doc, page, view) = fixture(json!({"grouping":{"group_by":"done"}}));
    doc.columns.iter_mut().find(|c| c.id == "CB").unwrap().kind = "todo_completed".into();
    assert!(
        project(&doc, &page, &view, &context)
            .unwrap()
            .board_completion_column_id
            .is_none()
    );
}
#[test]
fn single_select_does_not_guess_first_membership_and_boards_preserve_row_sort() {
    let (mut doc, page, view) = fixture(json!({"sorts":[{"key":"n","ascending":true}]}));
    doc.columns.iter_mut().find(|c| c.id == "CS").unwrap().raw["options"]["format"] =
        json!("single_select");
    let p = project(&doc, &page, &view, &ProjectionContext::default()).unwrap();
    assert_eq!(rows(&p.groups[0]), ["R3", "R1"]);
    assert_eq!(rows(&p.groups[2]), ["R2"]);
}
