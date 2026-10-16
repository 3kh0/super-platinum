use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api, client::RequestBody},
};
use serde_json::json;

fn document() -> Document {
    Document::parse(json!({"id":"F1","mode":"list","access":"write","created":1,"user":"U1","list_limits":{"over_view_maximum":false},"list_metadata":{"schema":[
        {"id":"C_TITLE","key":"title","name":"Title","type":"text"},
        {"id":"C_CHECK","key":"checked","name":"Done","type":"checkbox","opaque":{"keep":true}},
        {"id":"C_SELECT","key":"status","name":"Status","type":"select","options":{"format":"single_select","choices":[{"value":"todo","label":"To do"}]}},
        {"id":"C_TODO","key":"todo_completed","name":"Personal completion","type":"todo_completed"}
    ],"views":[{"id":"OLD","name":"Old","type":"table","opaque":[1]}],"opaque":{"keep":2}}})).unwrap()
}
fn personal() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn intent(doc: &Document, key: &str, layout: ViewLayout) -> ViewCreate {
    ViewCreate::capture_with_grouping(
        doc,
        "Grouped",
        layout,
        Some(key),
        Some("full"),
        Some(&personal()),
    )
    .unwrap()
}
fn added(doc: &Document, layout: ViewLayout, grouping: Value) -> Document {
    let mut raw = doc.raw.clone();
    raw["list_metadata"]["views"].as_array_mut().unwrap().push(json!({"id":"NEW","name":"Grouped","type":layout.as_str(),"grouping":grouping,"opaque":{"server":true}}));
    Document::parse(raw).unwrap()
}
#[test]
fn grouping_builder_uses_column_key_and_no_other_view_settings() {
    let doc = document();
    for layout in [ViewLayout::Table, ViewLayout::Board] {
        for key in ["checked", "status"] {
            let mut attempt = ViewCreateAttempt::new(intent(&doc, key, layout));
            let permit = attempt
                .admit(&doc, Some("full"), Some(&personal()))
                .unwrap();
            let session = WorkspaceSession {
                team_id: "T1".into(),
                user_id: "U1".into(),
                enterprise_id: None,
                name: "fixture".into(),
                url: "https://example.slack.com".into(),
                token: "fixture-token".into(),
            };
            let request = api::list_view_create(&SlackClient::default(), &session, permit);
            let RequestBody::Form(fields) = request.body else {
                panic!()
            };
            assert_eq!(
                fields,
                vec![
                    ("token".into(), "fixture-token".into()),
                    ("list_id".into(), "F1".into()),
                    ("name".into(), "Grouped".into()),
                    ("type".into(), layout.as_str().into()),
                    ("grouping".into(), json!({"group_by":key}).to_string()),
                    ("_x_reason".into(), "create-view".into())
                ]
            );
            assert!(
                attempt
                    .admit(&doc, Some("full"), Some(&personal()))
                    .is_err()
            );
            assert!(attempt.finish(false));
            assert!(
                attempt
                    .admit(&doc, Some("full"), Some(&personal()))
                    .is_err()
            );
        }
    }
}
#[test]
fn choices_reject_nonordinary_missing_and_malformed_select_fields() {
    let doc = document();
    assert_eq!(
        ViewCreate::grouping_columns(&doc)
            .map(|c| c.key.as_str())
            .collect::<Vec<_>>(),
        ["checked", "status"]
    );
    for key in ["", "missing", "C_CHECK", "title", "todo_completed"] {
        assert!(
            ViewCreate::capture_with_grouping(
                &doc,
                "Grouped",
                ViewLayout::Table,
                Some(key),
                Some("full"),
                Some(&personal())
            )
            .is_err(),
            "{key}"
        );
    }
    for invalid in [
        json!({"format":"unknown"}),
        json!({"choices":[{"value":"a","label":"A"},{"value":"a","label":"Duplicate"}]}),
        json!({"choices":"not-array"}),
    ] {
        let mut raw = doc.raw.clone();
        raw["list_metadata"]["schema"][2]["options"] = invalid;
        let bad = Document::parse(raw).unwrap();
        assert!(
            ViewCreate::capture_with_grouping(
                &bad,
                "Grouped",
                ViewLayout::Board,
                Some("status"),
                Some("full"),
                Some(&personal())
            )
            .is_err()
        );
    }
    let mut forged = doc.clone();
    forged.columns[1].kind = "text".into();
    assert!(
        ViewCreate::capture_with_grouping(
            &forged,
            "Grouped",
            ViewLayout::Table,
            Some("status"),
            Some("full"),
            Some(&personal())
        )
        .is_err()
    );
}
#[test]
fn whole_schema_and_grouping_identity_are_rechecked_before_dispatch() {
    let doc = document();
    for mutation in ["id", "key", "type", "unknown", "remove"] {
        let captured = intent(&doc, "checked", ViewLayout::Board);
        let mut raw = doc.raw.clone();
        if mutation == "remove" {
            raw["list_metadata"]["schema"]
                .as_array_mut()
                .unwrap()
                .remove(1);
        } else {
            raw["list_metadata"]["schema"][1][mutation] = json!("changed");
        }
        let fresh = Document::parse(raw).unwrap();
        assert!(
            ViewCreateAttempt::new(captured)
                .admit(&fresh, Some("full"), Some(&personal()))
                .is_err(),
            "{mutation}"
        );
    }
}
#[test]
fn grouped_readback_requires_exact_grouping_and_preserves_all_other_metadata() {
    let doc = document();
    for layout in [ViewLayout::Table, ViewLayout::Board] {
        let captured = intent(&doc, "status", layout);
        let exact = added(&doc, layout, json!({"group_by":"status"}));
        assert_eq!(captured.matching_addition(&exact), Some("NEW"));
        assert_eq!(exact.views[1]["opaque"], json!({"server":true}));
        for grouping in [
            Value::Null,
            json!({}),
            json!({"group_by":"checked"}),
            json!({"groupBy":"status"}),
            json!({"group_by":"status","order":[]}),
            json!({"group_by":"status","group_by_column_id":"C_SELECT"}),
        ] {
            assert!(
                captured
                    .matching_addition(&added(&doc, layout, grouping))
                    .is_none()
            );
        }
        let mut missing = exact.raw.clone();
        missing["list_metadata"]["views"][1]
            .as_object_mut()
            .unwrap()
            .remove("grouping");
        assert!(
            captured
                .matching_addition(&Document::parse(missing).unwrap())
                .is_none()
        );
        let mut changed = exact.raw.clone();
        changed["list_metadata"]["schema"][2]["opaque"] = json!(1);
        assert!(
            captured
                .matching_addition(&Document::parse(changed).unwrap())
                .is_none()
        );
        let mut ambiguous = exact.raw.clone();
        ambiguous["list_metadata"]["views"].as_array_mut().unwrap().push(json!({"id":"ANOTHER","name":"Grouped","type":"table","grouping":{"group_by":"status"}}));
        assert!(
            captured
                .matching_addition(&Document::parse(ambiguous).unwrap())
                .is_none()
        );
    }
}

#[test]
fn none_default_does_not_adopt_a_grouped_addition() {
    let doc = document();
    let captured = ViewCreate::capture(
        &doc,
        "Grouped",
        ViewLayout::Table,
        Some("full"),
        Some(&personal()),
    )
    .unwrap();
    assert_eq!(captured.grouping_key(), None);
    for grouping in [Value::Null, json!({})] {
        assert_eq!(
            captured.matching_addition(&added(&doc, ViewLayout::Table, grouping)),
            Some("NEW")
        );
    }
    assert!(
        captured
            .matching_addition(&added(
                &doc,
                ViewLayout::Table,
                json!({"group_by":"checked"})
            ))
            .is_none()
    );
}
