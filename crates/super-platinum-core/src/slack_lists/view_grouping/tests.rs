use super::*;
fn document(kind: &str) -> Document {
    Document::parse(json!({"id":"F_GROUP","mode":"list","access":"write","created":1,"user":"U1","opaque_file":7,"list_metadata":{
        "opaque":{"keep":1},"schema":[
            {"id":"C_NAME","key":"name","name":"Item","type":"text","options":{"format":"text"},"is_primary_column":true},
            {"id":"C_CHECK","key":"checked_✓","name":"Verified","type":"checkbox","opaque":2},
            {"id":"C_SELECT","key":"status","name":"Status","type":"select","options":{"format":"single_select","choices":[{"value":"a","label":"Alpha","color":"blue"},{"value":"b","label":"Beta"}]}},
            {"id":"C_NUMBER","key":"estimate","name":"Estimate","type":"number","options":{"precision":0}}],
        "views":[{"id":"V1","name":"Custom","type":kind,"columns":[{"key":"name","id":"C_NAME","visible":true},{"key":"checked_✓","id":"C_CHECK","visible":true,"width":177},{"key":"status","id":"C_SELECT","visible":false}],
            "filters":[],"sorts":[{"key":"estimate","ascending":true}],"grouping":{},"opaque_view":4},
            {"id":"V2","name":"Sibling","type":"table","columns":[],"grouping":{"group_by":"status","order":[]}}]}})).unwrap()
}
fn ordinary() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn choice(id: &str) -> ViewGroupingChoice {
    ViewGroupingChoice::Column {
        column_id: id.into(),
    }
}
fn capture(doc: &Document, desired: ViewGroupingChoice) -> ViewGrouping {
    ViewGrouping::capture(doc, "V1", desired, Some("full"), Some(&ordinary())).unwrap()
}
fn grouped(doc: &Document, key: &str) -> Document {
    let mut raw = doc.raw.clone();
    raw["list_metadata"]["views"][0]["grouping"] = json!({"group_by":key});
    Document::parse(raw).unwrap()
}
#[test]
fn set_change_clear_emit_resolved_keys_and_preserve_all_other_metadata() {
    for kind in ["table", "board"] {
        let doc = document(kind);
        for (id, key) in [("C_CHECK", "checked_✓"), ("C_SELECT", "status")] {
            let intent = capture(&doc, choice(id));
            assert_eq!(intent.grouping(), &json!({"group_by":key}));
            assert_eq!(intent.kind(), kind);
            assert_eq!(intent.desired_grouping(), &choice(id));
            let observed = grouped(&doc, key);
            assert!(intent.observes_desired(&observed));
            assert_eq!(ViewGrouping::current(&observed, "V1"), Some(choice(id)));
            let clear = capture(&observed, ViewGroupingChoice::None);
            assert_eq!(clear.grouping(), &json!({"group_by":""}));
            assert!(clear.observes_desired(&grouped(&doc, "")));
            assert!(!clear.observes_desired(&doc));
            let change = capture(
                &observed,
                choice(if id == "C_CHECK" {
                    "C_SELECT"
                } else {
                    "C_CHECK"
                }),
            );
            assert!(change.observes_desired(&grouped(
                &doc,
                if id == "C_CHECK" {
                    "status"
                } else {
                    "checked_✓"
                }
            )));
            let mut restored = observed.raw.clone();
            restored["list_metadata"]["views"][0]["grouping"] = doc.views[0]["grouping"].clone();
            assert_eq!(restored, doc.raw);
        }
    }
}
#[test]
fn ungrouped_raw_variants_are_retained_and_never_authorize_noop_clear() {
    for grouping in [
        None,
        Some(Value::Null),
        Some(json!({})),
        Some(json!({"group_by":""})),
    ] {
        let mut raw = document("table").raw;
        let view = raw["list_metadata"]["views"][0].as_object_mut().unwrap();
        match grouping {
            Some(v) => {
                view.insert("grouping".into(), v);
            }
            None => {
                view.remove("grouping");
            }
        }
        let doc = Document::parse(raw.clone()).unwrap();
        assert_eq!(
            ViewGrouping::current(&doc, "V1"),
            Some(ViewGroupingChoice::None)
        );
        assert!(
            ViewGrouping::capture(
                &doc,
                "V1",
                ViewGroupingChoice::None,
                Some("full"),
                Some(&ordinary())
            )
            .is_err()
        );
        assert_eq!(capture(&doc, choice("C_CHECK")).original().raw, raw);
    }
    let doc = grouped(&document("table"), "status");
    assert!(
        ViewGrouping::capture(
            &doc,
            "V1",
            choice("C_SELECT"),
            Some("full"),
            Some(&ordinary())
        )
        .is_err()
    );
}
#[test]
fn enriched_malformed_unsupported_current_grouping_cannot_be_replaced_or_cleared() {
    for value in [
        json!([]),
        json!("status"),
        json!({"group_by":null}),
        json!({"group_by":"C_SELECT"}),
        json!({"group_by":"estimate"}),
        json!({"group_by":"missing"}),
        json!({"group_by":"status","order":[]}),
        json!({"group_by":"status","hide_empty_groups":false}),
        json!({"group_by":"status","group_by_column_id":"C_SELECT"}),
        json!({"group_by":"","unknown":false}),
    ] {
        let mut raw = document("table").raw;
        raw["list_metadata"]["views"][0]["grouping"] = value;
        let doc = Document::parse(raw.clone()).unwrap();
        assert_eq!(ViewGrouping::current(&doc, "V1"), None);
        for desired in [ViewGroupingChoice::None, choice("C_CHECK")] {
            assert!(
                ViewGrouping::capture(&doc, "V1", desired, Some("full"), Some(&ordinary()))
                    .is_err()
            );
        }
        assert_eq!(doc.raw, raw);
    }
}
#[test]
fn only_active_ordinary_checkbox_and_valid_single_select_choices_are_supported() {
    let doc = document("table");
    assert_eq!(
        ViewGrouping::grouping_columns(&doc)
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        vec!["C_CHECK", "C_SELECT"]
    );
    for (index, options) in [
        (1, Value::Null),
        (1, json!({"markAsDoneWhenChecked":true})),
        (2, Value::Null),
        (2, json!({})),
        (2, json!({"choices":[]})),
        (2, json!({"format":"multi_select","choices":[]})),
        (
            2,
            json!({"format":"single_select","choices":[{"value":"a","label":"A"},{"value":"a","label":"Duplicate"}]}),
        ),
        (2, json!({"choices":[{"value":"a|b","label":"Bad"}]})),
        (2, json!({"choices":[{"value":"a"}]})),
        (2, json!({"choices":[],"unknown":true})),
    ] {
        let mut raw = doc.raw.clone();
        raw["list_metadata"]["schema"][index]["options"] = options.clone();
        let current = Document::parse(raw).unwrap();
        // An actual empty choices array with absent format is a valid single-select.
        assert_eq!(
            ViewGrouping::supports_column(&current.columns[index]),
            index == 2 && options == json!({"choices":[]})
        );
    }
    for kind in ["completed", "todo_completed", "number", "text", "user"] {
        let mut raw = doc.raw.clone();
        raw["list_metadata"]["schema"][1]["type"] = json!(kind);
        assert!(!ViewGrouping::supports_column(
            &Document::parse(raw).unwrap().columns[1]
        ));
    }
    for marker in ["is_deleted", "deleted", "suggested", "is_suggested"] {
        let mut raw = doc.raw.clone();
        raw["list_metadata"]["schema"][1][marker] = json!(true);
        let current = Document::parse(raw).unwrap();
        assert!(
            ViewGrouping::capture(
                &current,
                "V1",
                choice("C_CHECK"),
                Some("full"),
                Some(&ordinary())
            )
            .is_err()
        );
    }
    for (key, value) in [
        ("type", json!("calendar")),
        ("is_locked", json!(true)),
        ("is_all_items_view", json!(true)),
        ("default_view_key", json!("all")),
    ] {
        let mut raw = doc.raw.clone();
        raw["list_metadata"]["views"][0][key] = value;
        assert_eq!(
            ViewGrouping::current(&Document::parse(raw).unwrap(), "V1"),
            None
        );
    }
}
#[test]
fn permissions_raw_parsed_identity_and_fresh_schema_are_revalidated() {
    let doc = document("table");
    for allow in [None, Some("read_only")] {
        assert!(
            ViewGrouping::capture(&doc, "V1", choice("C_CHECK"), allow, Some(&ordinary())).is_err()
        );
    }
    assert!(ViewGrouping::capture(&doc, "V1", choice("C_CHECK"), Some("full"), None).is_err());
    let personal = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[\"F_GROUP\"]","assigned_to_list":""}}),
    )
    .unwrap();
    assert!(
        ViewGrouping::capture(&doc, "V1", choice("C_CHECK"), Some("full"), Some(&personal))
            .is_err()
    );
    let mut inconsistent = doc.clone();
    inconsistent.columns[1].key = "changed".into();
    assert!(
        ViewGrouping::capture(
            &inconsistent,
            "V1",
            choice("C_CHECK"),
            Some("full"),
            Some(&ordinary())
        )
        .is_err()
    );
    for (pointer, value) in [
        ("/access", json!("read")),
        ("/created", json!(2)),
        ("/user", json!("U2")),
        ("/list_metadata/schema/1/key", json!("replaced")),
        ("/list_metadata/views/1/name", json!("concurrent")),
        ("/list_metadata/opaque/keep", json!(false)),
    ] {
        let mut raw = doc.raw.clone();
        *raw.pointer_mut(pointer).unwrap() = value;
        assert!(
            capture(&doc, choice("C_CHECK"))
                .revalidate(
                    &Document::parse(raw).unwrap(),
                    Some("full"),
                    Some(&ordinary())
                )
                .is_err()
        );
    }
    for (left, right) in [("id", "id"), ("key", "key")] {
        let mut raw = doc.raw.clone();
        raw["list_metadata"]["schema"][2][left] = raw["list_metadata"]["schema"][1][right].clone();
        assert!(Document::parse(raw).is_err());
    }
    let mut raw = doc.raw.clone();
    raw["list_metadata"]["views"][1]["id"] = json!("V1");
    assert!(Document::parse(raw).is_err());
}
#[test]
fn canonical_observation_is_exact_not_ack_or_server_normalization() {
    let doc = document("board");
    let intent = capture(&doc, choice("C_SELECT"));
    let expected = grouped(&doc, "status");
    for pointer in [
        "/list_metadata/opaque/keep",
        "/list_metadata/schema/1/opaque",
        "/list_metadata/views/0/columns/1/width",
        "/list_metadata/views/0/opaque_view",
        "/list_metadata/views/1/name",
        "/created",
        "/user",
    ] {
        let mut raw = expected.raw.clone();
        *raw.pointer_mut(pointer).unwrap() = json!("drift");
        assert!(
            !intent.observes_desired(&Document::parse(raw).unwrap()),
            "{pointer}"
        );
    }
    for (key, value) in [
        ("order", json!([])),
        ("group_by_column_id", json!("C_SELECT")),
        ("hide_empty_groups", json!(false)),
    ] {
        let mut raw = expected.raw.clone();
        raw["list_metadata"]["views"][0]["grouping"][key] = value;
        assert!(!intent.observes_desired(&Document::parse(raw).unwrap()));
    }
    let mut malformed = expected.clone();
    malformed.views[0]["grouping"] = json!({});
    assert!(!intent.observes_desired(&malformed));
    for unsent in [false, true] {
        let mut attempt = ViewGroupingAttempt::new(intent.clone());
        assert!(!attempt.observe(&expected));
        attempt
            .admit(&doc, Some("full"), Some(&ordinary()))
            .unwrap();
        assert!(!attempt.observe(&expected));
        assert!(attempt.finish(unsent));
        assert!(!attempt.finish(false));
        assert!(
            attempt
                .admit(&doc, Some("full"), Some(&ordinary()))
                .is_err()
        );
        assert!(!attempt.observe(&doc));
        assert_eq!(attempt.observe(&expected), !unsent);
    }
}

mod projection;
