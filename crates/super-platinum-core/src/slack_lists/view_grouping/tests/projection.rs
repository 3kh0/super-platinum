use super::*;
use crate::slack_lists::{
    RecordsPage,
    find::Query,
    saved_views::{self, ProjectionContext, ProjectionLayout, SavedView},
};

#[test]
fn actual_table_and_board_group_membership_preserves_filtered_sorted_find_and_raw_page() {
    let page=RecordsPage::parse(json!({"ok":true,"records":[
        {"id":"R1","position":0,"fields":[{"key":"name","text":"Keep one"},{"key":"estimate","number":[9]},{"key":"checked_✓","checkbox":true},{"key":"status","select":["a"]}]},
        {"id":"R2","position":1,"fields":[{"key":"name","text":"Keep two"},{"key":"estimate","number":[2]},{"key":"checked_✓","checkbox":false},{"key":"status","select":["b"]}]},
        {"id":"R3","position":2,"fields":[{"key":"name","text":"Keep three"},{"key":"estimate","number":[1]}]},
        {"id":"R4","position":3,"fields":[{"key":"name","text":"Drop Find"},{"key":"estimate","number":[3]},{"key":"checked_✓","checkbox":true}]},
        {"id":"R5","position":4,"fields":[{"key":"name","text":"Keep filtered"},{"key":"estimate","number":[0]}]},
        {"id":"CHILD","parent_record_id":"R1","fields":[{"key":"name","text":"Keep child"},{"key":"estimate","number":[4]}]}
    ],"response_metadata":{"next_cursor":"opaque+/="},"opaque":{"retain":true}})).unwrap();
    let untouched = page.clone();
    for kind in ["table", "board"] {
        let mut raw = document(kind).raw;
        raw["list_metadata"]["views"][0]["filters"] =
            json!([{"key":"estimate","operator":"greater_than","values":[0]}]);
        let doc = Document::parse(raw).unwrap();
        let context = ProjectionContext {
            editable: true,
            find: Some(Query {
                text: "Keep".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        for key in ["checked_✓", "status", ""] {
            let canonical = grouped(&doc, key);
            let intent = if key.is_empty() {
                capture(&grouped(&doc, "status"), ViewGroupingChoice::None)
            } else {
                capture(
                    &doc,
                    choice(if key == "status" {
                        "C_SELECT"
                    } else {
                        "C_CHECK"
                    }),
                )
            };
            assert!(intent.observes_desired(&canonical));
            let view = SavedView::parse(canonical.views[0].clone()).unwrap();
            let projection = saved_views::project(&canonical, &page, &view, &context).unwrap();
            assert_eq!(
                projection.layout,
                if kind == "table" {
                    ProjectionLayout::Table
                } else {
                    ProjectionLayout::Board
                }
            );
            assert_eq!(projection.unique_record_count, 3);
            assert!(projection.incomplete);
            let groups = projection
                .groups
                .iter()
                .map(|g| {
                    (
                        g.key.clone(),
                        g.rows
                            .iter()
                            .map(|r| r.record_id.as_str())
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>();
            match (kind, key) {
                ("table", "checked_✓") => assert_eq!(
                    groups,
                    vec![
                        (Some("true".into()), vec!["R1"]),
                        (Some("".into()), vec!["R3", "R2"])
                    ]
                ),
                ("board", "checked_✓") => assert_eq!(
                    groups,
                    vec![
                        (Some("".into()), vec!["R3", "R2"]),
                        (Some("true".into()), vec!["R1"])
                    ]
                ),
                ("table", "status") => assert_eq!(
                    groups,
                    vec![
                        (Some("a".into()), vec!["R1"]),
                        (Some("b".into()), vec!["R2"]),
                        (Some("".into()), vec!["R3"])
                    ]
                ),
                ("board", "status") => assert_eq!(
                    groups,
                    vec![
                        (Some("".into()), vec!["R3"]),
                        (Some("a".into()), vec!["R1"]),
                        (Some("b".into()), vec!["R2"])
                    ]
                ),
                ("table", "") => assert_eq!(groups, vec![(None, vec!["R3", "R2", "R1"])]),
                ("board", "") => {
                    assert_eq!(groups, vec![(Some("".into()), vec!["R3", "R2", "R1"])])
                }
                _ => unreachable!(),
            }
            if key == "status" {
                assert_eq!(
                    projection.column_ids.contains(&"C_SELECT".into()),
                    kind == "table"
                );
            }
            assert_eq!(page, untouched);
        }
    }
}
