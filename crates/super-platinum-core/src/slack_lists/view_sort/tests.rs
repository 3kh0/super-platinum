use super::*;
fn document() -> Document {
    Document::parse(json!({"id":"F_SORT","mode":"list","access":"write","created":1,"user":"U1","opaque_file":7,"list_metadata":{
        "opaque":{"keep":1},"schema":[
            {"id":"C_NAME","key":"name","name":"Title","type":"text","options":{"format":"text"},"is_primary_column":true},
            {"id":"C_NUMBER","key":"estimate","name":"Estimate","type":"number","options":{"precision":2},"opaque":2},
            {"id":"C_OTHER","key":"second","name":"Second","type":"number","options":{"precision":0}}],
        "views":[{"id":"V1","name":"Custom","type":"table","columns":[{"key":"name","id":"C_NAME","visible":true},{"key":"estimate","id":"C_NUMBER","visible":true,"width":177}],
            "filters":[],"sorts":[],"grouping":{},"opaque_view":4},
            {"id":"V2","name":"Other","type":"board","sorts":[]}]}})).unwrap()
}
fn ordinary() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn choice(ascending: bool) -> ViewSortChoice {
    ViewSortChoice::Number {
        column_id: "C_NUMBER".into(),
        ascending,
    }
}
fn capture(doc: &Document, desired: ViewSortChoice) -> ViewSort {
    ViewSort::capture(doc, "V1", desired, Some("full"), Some(&ordinary())).unwrap()
}
fn sorted(ascending: bool) -> Document {
    let mut raw = document().raw;
    raw["list_metadata"]["views"][0]["sorts"] = json!([{"key":"estimate","ascending":ascending}]);
    Document::parse(raw).unwrap()
}
#[test]
fn complete_vectors_resolve_exact_key_not_id_and_clear_explicitly() {
    for ascending in [true, false] {
        let intent = capture(&document(), choice(ascending));
        assert_eq!(
            intent.sorts(),
            &json!([{"key":"estimate","ascending":ascending}])
        );
        assert_eq!(intent.desired_sort(), &choice(ascending));
        assert!(intent.observes_desired(&sorted(ascending)));
        assert_eq!(
            ViewSort::current(&sorted(ascending), "V1"),
            Some(choice(ascending))
        );
        let clear = capture(&sorted(ascending), ViewSortChoice::None);
        assert_eq!(clear.sorts(), &json!([]));
        assert!(clear.observes_desired(&document()));
    }
    assert!(
        ViewSort::capture(
            &document(),
            "V1",
            ViewSortChoice::None,
            Some("full"),
            Some(&ordinary())
        )
        .is_err()
    );
    assert!(
        ViewSort::capture(
            &sorted(true),
            "V1",
            choice(true),
            Some("full"),
            Some(&ordinary())
        )
        .is_err()
    );
    let mut raw = document().raw;
    raw["list_metadata"]["views"][0]
        .as_object_mut()
        .unwrap()
        .remove("sorts");
    let missing = Document::parse(raw).unwrap();
    assert_eq!(
        ViewSort::current(&missing, "V1"),
        Some(ViewSortChoice::None)
    );
    assert!(capture(&missing, choice(true)).observes_desired(&sorted(true)));
}
#[test]
fn unsupported_current_rules_are_preserved_and_cannot_be_cleared_or_overwritten() {
    for rules in [
        Value::Null,
        json!({}),
        json!([{}]),
        json!([{"key":"estimate","ascending":"true"}]),
        json!([{"key":"C_NUMBER","ascending":true}]),
        json!([{"key":"name","ascending":true}]),
        json!([{"key":"estimate","ascending":true,"column_id":"C_NUMBER"}]),
        json!([{"key":"estimate","ascending":true},{"key":"second","ascending":false}]),
    ] {
        let mut raw = document().raw;
        raw["list_metadata"]["views"][0]["sorts"] = rules;
        let doc = Document::parse(raw.clone()).unwrap();
        assert_eq!(ViewSort::current(&doc, "V1"), None);
        for desired in [ViewSortChoice::None, choice(true)] {
            assert!(
                ViewSort::capture(&doc, "V1", desired, Some("full"), Some(&ordinary())).is_err()
            );
        }
        assert_eq!(doc.raw, raw);
    }
}
#[test]
fn supported_number_shapes_and_custom_table_boundary_are_exact() {
    for precision in 0..=5 {
        let mut raw = document().raw;
        raw["list_metadata"]["schema"][1]["options"] = json!({"precision":precision});
        let doc = Document::parse(raw).unwrap();
        assert!(ViewSort::supports_column(doc.column("C_NUMBER").unwrap()));
        capture(&doc, choice(true));
    }
    for options in [
        json!({}),
        json!({"precision":6}),
        json!({"precision":"2"}),
        json!({"precision":2,"unit":"x"}),
    ] {
        let mut raw = document().raw;
        raw["list_metadata"]["schema"][1]["options"] = options;
        let doc = Document::parse(raw).unwrap();
        assert!(!ViewSort::supports_column(doc.column("C_NUMBER").unwrap()));
        assert!(
            ViewSort::capture(&doc, "V1", choice(true), Some("full"), Some(&ordinary())).is_err()
        );
    }
    for (key, value) in [
        ("type", json!("board")),
        ("is_locked", json!(true)),
        ("is_all_items_view", json!(true)),
        ("default_view_key", json!("all")),
    ] {
        let mut raw = document().raw;
        raw["list_metadata"]["views"][0][key] = value;
        assert_eq!(
            ViewSort::current(&Document::parse(raw).unwrap(), "V1"),
            None
        );
    }
}
#[test]
fn permission_personal_raw_identity_and_stale_schema_fail_before_admission() {
    for allow in [None, Some("read_only")] {
        assert!(
            ViewSort::capture(&document(), "V1", choice(true), allow, Some(&ordinary())).is_err()
        );
    }
    assert!(ViewSort::capture(&document(), "V1", choice(true), Some("full"), None).is_err());
    let personal = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[\"F_SORT\"]","assigned_to_list":""}}),
    )
    .unwrap();
    assert!(
        ViewSort::capture(
            &document(),
            "V1",
            choice(true),
            Some("full"),
            Some(&personal)
        )
        .is_err()
    );
    let mut inconsistent = document();
    inconsistent.columns[1].key = "wrong".into();
    assert!(
        ViewSort::capture(
            &inconsistent,
            "V1",
            choice(true),
            Some("full"),
            Some(&ordinary())
        )
        .is_err()
    );
    for (pointer, value) in [
        ("/access", json!("read")),
        ("/list_metadata/opaque/keep", json!(2)),
        ("/list_metadata/schema/1/key", json!("replaced")),
    ] {
        let mut raw = document().raw;
        *raw.pointer_mut(pointer).unwrap() = value;
        assert!(
            capture(&document(), choice(true))
                .revalidate(
                    &Document::parse(raw).unwrap(),
                    Some("full"),
                    Some(&ordinary())
                )
                .is_err()
        );
    }
}
#[test]
fn observation_requires_exact_full_metadata_and_original_file_identity() {
    let intent = capture(&document(), choice(false));
    for pointer in [
        "/list_metadata/opaque/keep",
        "/list_metadata/schema/1/opaque",
        "/list_metadata/views/0/columns/1/width",
        "/list_metadata/views/0/opaque_view",
        "/list_metadata/views/1/name",
        "/created",
        "/user",
    ] {
        let mut raw = sorted(false).raw;
        *raw.pointer_mut(pointer).unwrap() = json!("drift");
        assert!(
            !intent.observes_desired(&Document::parse(raw).unwrap()),
            "{pointer}"
        );
    }
    assert_eq!(intent.original(), &document());
    let mut changed = sorted(false);
    changed.views[0]["sorts"] = json!([]);
    assert!(!intent.observes_desired(&changed));
}
#[test]
fn consumed_attempt_retains_ambiguity_until_separate_read_and_never_replays() {
    for not_requested in [false, true] {
        let mut attempt = ViewSortAttempt::new(capture(&document(), choice(true)));
        assert!(!attempt.observe(&sorted(true)));
        attempt
            .admit(&document(), Some("full"), Some(&ordinary()))
            .unwrap();
        assert!(!attempt.observe(&sorted(true)));
        assert!(attempt.finish(not_requested));
        assert_eq!(
            attempt.phase(),
            if not_requested {
                ViewSortPhase::NotRequested
            } else {
                ViewSortPhase::Unconfirmed
            }
        );
        assert!(
            attempt
                .admit(&document(), Some("full"), Some(&ordinary()))
                .is_err()
        );
        assert!(!attempt.observe(&document()));
        assert_eq!(attempt.observe(&sorted(true)), !not_requested);
        assert!(!attempt.finish(false));
    }
}
#[test]
fn observed_sort_reprojects_numeric_order_without_changing_raw_records_or_cursor() {
    use crate::slack_lists::{
        RecordsPage,
        saved_views::{self, ProjectionContext, SavedView},
    };
    let numbers = [
        Some(12.5),
        Some(2.0),
        Some(-1.0),
        Some(0.0),
        Some(2.0),
        None,
    ];
    let rows:Vec<Value>=numbers.into_iter().enumerate().map(|(i,n)| {
        let mut fields=vec![json!({"key":"name","text":format!("Row {i}")})];
        if let Some(n)=n { fields.push(json!({"key":"estimate","number":[n]})); }
        json!({"id":format!("R{i}"),"position":i,"date_created":1,"fields":fields,"opaque":{"keep":i}})
    }).collect();
    let page = RecordsPage::parse(
        json!({"ok":true,"records":rows,"response_metadata":{"next_cursor":"next-owned-page"}}),
    )
    .unwrap();
    let untouched = page.clone();
    for (doc, expected) in [
        (sorted(true), vec!["R2", "R3", "R1", "R4", "R0", "R5"]),
        (sorted(false), vec!["R0", "R1", "R4", "R3", "R2", "R5"]),
        (document(), vec!["R0", "R1", "R2", "R3", "R4", "R5"]),
    ] {
        let view = SavedView::parse(doc.views[0].clone()).unwrap();
        let projection =
            saved_views::project(&doc, &page, &view, &ProjectionContext::default()).unwrap();
        let ids: Vec<&str> = projection
            .groups
            .iter()
            .flat_map(|g| g.rows.iter().map(|r| r.record_id.as_str()))
            .collect();
        assert_eq!(ids, expected);
        assert!(projection.incomplete);
        assert_eq!(page, untouched);
    }
}
