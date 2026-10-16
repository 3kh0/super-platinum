use super::*;
use serde_json::json;
fn document(kind: &str, options: Value) -> Document {
    Document::parse(json!({"id":"F","mode":"list","list_metadata":{"schema":[{"id":"C","key":"v","name":"Value","type":kind,"options":options}],"views":[]}})).unwrap()
}
fn row(field: Value) -> Record {
    let mut field = field;
    field["key"] = json!("v");
    Record::parse(json!({"id":"R","fields":[field]})).unwrap()
}
fn hit(kind: &str, field: Value, query: &str) -> bool {
    Search::new(
        &document(kind, Value::Null),
        &Query {
            text: query.into(),
            ..Default::default()
        },
    )
    .unwrap()
    .matches(&row(field))
    .unwrap()
}
#[test]
fn retained_oracle_literals_zero_and_excluded_types() {
    assert!(hit("text", json!({"text":"Café"}), "CAFE"));
    assert!(!hit("text", json!({"text":"Café"}), " cafe "));
    assert!(!hit("number", json!({"number":[0]}), "0"));
    assert!(hit("number", json!({"number":[0],"text":"0"}), "0"));
    for kind in [
        "rich_text",
        "currency",
        "multi_select",
        "assignee",
        "checkbox",
        "completed",
        "date",
        "timestamp",
        "link",
        "attachment",
        "canvas",
        "created_by",
        "last_edited_by",
        "reference",
    ] {
        assert!(!hit(kind, json!({"text":"match"}), "match"), "{kind}");
        assert!(hit(kind, json!({}), ""));
        assert!(!hit(kind, json!({}), "\u{0301}"));
    }
}
#[test]
fn retained_oracle_select_label_and_raw_id_fallthrough() {
    for multi in [false, true] {
        let doc = document(
            "select",
            json!({"format":if multi{"multi_select"}else{"single"},"choices":[{"value":"A","label":"Café"},{"value":"B","label":"Beta"}]}),
        );
        for (query, expected) in [("cafe", true), ("a", true), ("b", true), ("beta", multi)] {
            assert_eq!(
                Search::new(
                    &doc,
                    &Query {
                        text: query.into(),
                        ..Default::default()
                    }
                )
                .unwrap()
                .matches(&row(json!({"select":["A","B"]})))
                .unwrap(),
                expected
            );
        }
    }
}
#[test]
fn retained_oracle_whole_identity_lookup_has_no_id_fallback() {
    for (kind, key) in [
        ("user", "user"),
        ("todo_assignee", "user"),
        ("channel", "channel"),
    ] {
        let doc = document(kind, Value::Null);
        let mut query = Query {
            text: "alice".into(),
            user_names: Some(BTreeMap::from([("U1".into(), "Alice".into())])),
            channel_names: Some(BTreeMap::from([("U1".into(), "Alice".into())])),
        };
        assert!(
            Search::new(&doc, &query)
                .unwrap()
                .matches(&row(json!({key:["U1"]})))
                .unwrap()
        );
        assert!(
            !Search::new(&doc, &query)
                .unwrap()
                .matches(&row(json!({key:["U1","U2"]})))
                .unwrap()
        );
        query.text = "u1".into();
        assert!(
            !Search::new(&doc, &query)
                .unwrap()
                .matches(&row(json!({key:["U1"]})))
                .unwrap()
        );
    }
}
#[test]
fn retained_oracle_vote_raw_delimiters_and_rich_first_leaf() {
    assert!(hit("vote", json!({}), "0"));
    assert!(hit("vote", json!({"user":[""]}), "1"));
    assert!(hit("vote", json!({"user":["U1",""]}), "2"));
    assert!(hit("vote", json!({"user":["U1"],"text":"8"}), "1"));
    let rich = json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"First"},{"type":"text","text":"Second"}]}]}]);
    assert!(hit("text", json!({"rich_text":rich}), "first"));
    assert!(!hit("text", json!({"rich_text":rich}), "second"));
    assert!(hit(
        "text",
        json!({"rich_text":rich,"text":"First Second"}),
        "second"
    ));
    assert!(!hit(
        "text",
        json!({"rich_text":[{"type":"text","text":"Top leaf"}]}),
        "top"
    ));
}
#[test]
fn unicode_uses_nfd_limited_marks_and_contextual_lowercase() {
    assert_eq!(normalize("ΟΣ Café İ"), "ος cafe i");
    assert_eq!(normalize("ß ﬃ \u{1ab0}"), "ß ﬃ \u{1ab0}");
    assert!(!hit("text", json!({"text":"ß"}), "ss"));
    assert!(hit("text", json!({}), "\u{0301}"));
    assert!(!hit("text", json!({}), "\u{1ab0}"));
}
#[test]
fn decoder_precedence_and_malformed_bounds_are_explicit() {
    assert!(hit("number", json!({"channel":["C1"],"number":[42]}), "c1"));
    assert!(hit("number", json!({"number":[42,99]}), "42"));
    assert!(!hit("number", json!({"number":[42,99]}), "99"));
    let doc = document("text", Value::Null);
    let query = Query {
        text: "x".into(),
        ..Default::default()
    };
    let search = Search::new(&doc, &query).unwrap();
    for field in [
        json!({"text":"x","column_id":"OTHER"}),
        json!({"text":"x","row_id":"OTHER"}),
        json!({"rich_text":{}}),
        json!({"text":"x".repeat(65_537)}),
    ] {
        assert!(search.matches(&row(field)).is_err());
    }
    let mut nested = json!({"type":"text","text":"x"});
    for _ in 0..40 {
        nested = json!({"elements":[nested]});
    }
    assert!(search.matches(&row(json!({"rich_text":[nested]}))).is_err());
}
#[test]
fn name_maps_admit_base_rows_before_saved_filters_not_children_or_archived_rows() {
    let doc = document("user", Value::Null);
    for excluded in [
        json!({"parent_record_id":"R"}),
        json!({"archived":true}),
        json!({"is_deleted":true}),
        json!({"suggested":true}),
    ] {
        let mut hidden = json!({"id":"EXCLUDED","fields":[{"key":"v","user":["U2"]}]});
        hidden
            .as_object_mut()
            .unwrap()
            .extend(excluded.as_object().unwrap().clone());
        let page=RecordsPage::parse(json!({"ok":true,"records":[{"id":"R","fields":[{"key":"v","user":["U1"],"text":"U2"}]},hidden]})).unwrap();
        let (users, _) = name_keys(&doc, &page).unwrap();
        assert_eq!(users, BTreeSet::from(["U1".into()]));
        let names = users.into_iter().map(|id| (id, "Alice".into())).collect();
        let query = Query {
            text: "alice".into(),
            user_names: Some(names),
            ..Default::default()
        };
        assert!(
            !Search::new(&doc, &query)
                .unwrap()
                .matches(&page.records[0])
                .unwrap()
        );
    }
}
#[test]
fn full_schema_or_after_rules_preserves_sort_counts_and_raw_records() {
    use crate::slack_lists::{RecordsPage, saved_views};
    let doc=Document::parse(json!({"id":"F","mode":"list","list_metadata":{"schema":[
        {"id":"C","key":"name","name":"Name","type":"text"},{"id":"H","key":"hidden","name":"Hidden","type":"number"},{"id":"D","key":"done","name":"Done","type":"checkbox"}],
        "views":[{"id":"V","name":"Table","type":"table","columns":[{"id":"C","key":"name","visible":true},{"id":"H","key":"hidden","visible":false}],"filters":[{"key":"done","operator":"is_empty","values":[]}],"sorts":[{"key":"hidden","ascending":false}]}]}})).unwrap();
    let page=RecordsPage::parse(json!({"ok":true,"records":[
        {"id":"A","position":1,"fields":[{"key":"name","text":"alpha"},{"key":"hidden","number":[17]}]},
        {"id":"B","position":2,"fields":[{"key":"name","text":"beta"},{"key":"hidden","number":[71]}]},
        {"id":"DONE","position":3,"fields":[{"key":"hidden","number":[777]},{"key":"done","checkbox":true}]},
        {"id":"CHILD","parent_record_id":"A","fields":[{"key":"hidden","number":[777]}]}
    ],"response_metadata":{"next_cursor":"opaque"}})).unwrap();
    let before = (doc.clone(), page.clone());
    let view = saved_views::catalog(&doc).unwrap().remove(0);
    let context = saved_views::ProjectionContext {
        find: Some(Query {
            text: "7".into(),
            ..Default::default()
        }),
        ..Default::default()
    };
    let projection = saved_views::project(&doc, &page, &view, &context).unwrap();
    assert_eq!(projection.column_ids, ["C"]);
    assert_eq!(
        projection.groups[0]
            .rows
            .iter()
            .map(|r| r.record_id.as_str())
            .collect::<Vec<_>>(),
        ["B", "A"]
    );
    assert_eq!(projection.unique_record_count, 2);
    assert!(projection.incomplete);
    assert_eq!((doc, page), before);
}

#[test]
fn empty_original_query_bypasses_even_oversized_maps_and_malformed_cells() {
    let doc = document("text", Value::Null);
    let mut query = Query {
        user_names: Some(BTreeMap::from([("U1".into(), "x".repeat(65_537))])),
        ..Default::default()
    };
    assert!(
        Search::new(&doc, &query)
            .unwrap()
            .matches(&row(json!({"rich_text":{}})))
            .unwrap()
    );
    query.text = "x".into();
    assert!(Search::new(&doc, &query).is_err());
}
