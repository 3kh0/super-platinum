use super::*;
fn document() -> Document {
    Document::parse(json!({"id":"F_FILTER","mode":"list","access":"write","created":1,"user":"U1","list_metadata":{
        "opaque":{"keep":1},"schema":[
            {"id":"C_NAME","key":"name","name":"Title","type":"text","is_primary_column":true},
            {"id":"C_CHECK","key":"done","name":"Checked","type":"checkbox","options":{"opaque":"preserved"},"opaque":2},
            {"id":"C_NUMBER","key":"estimate","name":"Estimate","type":"number","options":{"precision":2}}],
        "views":[{"id":"V1","name":"Custom","type":"table","columns":[{"key":"name","id":"C_NAME","visible":true},{"key":"done","id":"C_CHECK","visible":true,"width":177}],
            "filters":[],"info_column_filters":[],"sorts":[{"key":"estimate","ascending":true}],"grouping":{},"opaque_view":4},
            {"id":"V2","name":"Other","type":"board","filters":[]}]}})).unwrap()
}
fn ordinary() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn choice(checked: bool) -> ViewFilterChoice {
    ViewFilterChoice::Checkbox {
        column_id: "C_CHECK".into(),
        checked,
    }
}
fn capture(doc: &Document, desired: ViewFilterChoice) -> ViewFilter {
    ViewFilter::capture(doc, "V1", desired, Some("full"), Some(&ordinary())).unwrap()
}
fn filtered(checked: bool) -> Document {
    let mut raw = document().raw;
    raw["list_metadata"]["views"][0]["filters"] =
        json!([{"key":"done","operator":if checked {"is_not_empty"}else{"is_empty"},"values":[]}]);
    Document::parse(raw).unwrap()
}
#[test]
fn complete_vectors_use_captured_key_and_exact_checkbox_operators() {
    for checked in [true, false] {
        let intent = capture(&document(), choice(checked));
        assert_eq!(intent.filters(), &filtered(checked).views[0]["filters"]);
        assert_eq!(intent.info_column_filters(), json!([]));
        assert_eq!(intent.desired_filter(), &choice(checked));
        assert!(intent.observes_desired(&filtered(checked)));
        assert_eq!(
            ViewFilter::current(&filtered(checked), "V1"),
            Some(choice(checked))
        );
        let clear = capture(&filtered(checked), ViewFilterChoice::None);
        assert_eq!(clear.filters(), &json!([]));
        assert!(clear.observes_desired(&document()));
    }
    assert!(
        ViewFilter::capture(
            &document(),
            "V1",
            ViewFilterChoice::None,
            Some("full"),
            Some(&ordinary())
        )
        .is_err()
    );
    assert!(
        ViewFilter::capture(
            &filtered(true),
            "V1",
            choice(true),
            Some("full"),
            Some(&ordinary())
        )
        .is_err()
    );
}
#[test]
fn absent_initial_arrays_do_not_authorize_normalizing_absent_canonical_readback() {
    let mut raw = document().raw;
    raw["list_metadata"]["views"][0]
        .as_object_mut()
        .unwrap()
        .remove("filters");
    raw["list_metadata"]["views"][0]
        .as_object_mut()
        .unwrap()
        .remove("info_column_filters");
    let missing = Document::parse(raw).unwrap();
    assert_eq!(
        ViewFilter::current(&missing, "V1"),
        Some(ViewFilterChoice::None)
    );
    let intent = capture(&missing, choice(true));
    assert!(intent.observes_desired(&filtered(true)));
    let mut canonical = filtered(true).raw;
    canonical["list_metadata"]["views"][0]
        .as_object_mut()
        .unwrap()
        .remove("info_column_filters");
    assert!(!intent.observes_desired(&Document::parse(canonical).unwrap()));
    let clear = capture(&filtered(true), ViewFilterChoice::None);
    assert!(!clear.observes_desired(&missing));
}
#[test]
fn unsupported_filters_metadata_filters_and_grouped_sets_cannot_be_overwritten() {
    for rules in [
        Value::Null,
        json!({}),
        json!([{}]),
        json!([{"key":"done","operator":"equal","values":[]}]),
        json!([{"key":"C_CHECK","operator":"is_empty","values":[]}]),
        json!([{"key":"done","operator":"is_empty","values":[false]}]),
        json!([{"key":"done","operator":"is_empty","values":[],"column_id":"C_CHECK"}]),
        json!([{"key":"done","operator":"is_empty","values":[]},{"key":"done","operator":"is_not_empty","values":[]}]),
    ] {
        let mut raw = document().raw;
        raw["list_metadata"]["views"][0]["filters"] = rules;
        let doc = Document::parse(raw.clone()).unwrap();
        assert_eq!(ViewFilter::current(&doc, "V1"), None);
        for desired in [ViewFilterChoice::None, choice(true)] {
            assert!(
                ViewFilter::capture(&doc, "V1", desired, Some("full"), Some(&ordinary())).is_err()
            );
        }
        assert_eq!(doc.raw, raw);
    }
    for (key, value) in [
        ("filter_set", json!({"match":"any","groups":[]})),
        ("filter_set", json!({"match":"all","groups":[]})),
        ("info_column_filters", Value::Null),
        ("info_column_filters", json!({})),
        (
            "info_column_filters",
            json!([{"key":"created_by","operator":"is_empty","values":[]}]),
        ),
    ] {
        let mut raw = document().raw;
        raw["list_metadata"]["views"][0][key] = value;
        let doc = Document::parse(raw.clone()).unwrap();
        assert_eq!(ViewFilter::current(&doc, "V1"), None);
        assert!(
            ViewFilter::capture(&doc, "V1", choice(true), Some("full"), Some(&ordinary())).is_err()
        );
        assert_eq!(doc.raw, raw);
    }
}
#[test]
fn only_ordinary_checkbox_type_and_custom_table_are_authorable() {
    for kind in ["completed", "todo_completed", "number", "text"] {
        let mut raw = document().raw;
        raw["list_metadata"]["schema"][1]["type"] = json!(kind);
        let doc = Document::parse(raw).unwrap();
        assert!(!ViewFilter::supports_column(doc.column("C_CHECK").unwrap()));
        assert!(
            ViewFilter::capture(&doc, "V1", choice(true), Some("full"), Some(&ordinary())).is_err()
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
            ViewFilter::current(&Document::parse(raw).unwrap(), "V1"),
            None
        );
    }
}
#[test]
fn permissions_raw_identity_and_stale_metadata_refuse_before_admission() {
    for allow in [None, Some("read_only")] {
        assert!(
            ViewFilter::capture(&document(), "V1", choice(true), allow, Some(&ordinary())).is_err()
        );
    }
    assert!(ViewFilter::capture(&document(), "V1", choice(true), Some("full"), None).is_err());
    let personal = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[\"F_FILTER\"]","assigned_to_list":""}}),
    )
    .unwrap();
    assert!(
        ViewFilter::capture(
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
        ViewFilter::capture(
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
fn full_metadata_and_file_identity_required_for_observed_result() {
    let intent = capture(&document(), choice(false));
    for pointer in [
        "/list_metadata/opaque/keep",
        "/list_metadata/schema/1/opaque",
        "/list_metadata/views/0/columns/1/width",
        "/list_metadata/views/0/opaque_view",
        "/list_metadata/views/0/sorts/0/ascending",
        "/list_metadata/views/1/name",
        "/created",
        "/user",
    ] {
        let mut raw = filtered(false).raw;
        *raw.pointer_mut(pointer).unwrap() = json!("drift");
        assert!(
            !intent.observes_desired(&Document::parse(raw).unwrap()),
            "{pointer}"
        );
    }
    assert_eq!(intent.original(), &document());
}
#[test]
fn consumed_attempt_never_replays_or_promotes_ack_to_observed() {
    for not_requested in [false, true] {
        let mut attempt = ViewFilterAttempt::new(capture(&document(), choice(true)));
        assert!(!attempt.observe(&filtered(true)));
        attempt
            .admit(&document(), Some("full"), Some(&ordinary()))
            .unwrap();
        assert!(!attempt.observe(&filtered(true)));
        assert!(attempt.finish(not_requested));
        assert_eq!(
            attempt.phase(),
            if not_requested {
                ViewFilterPhase::NotRequested
            } else {
                ViewFilterPhase::Unconfirmed
            }
        );
        assert!(
            attempt
                .admit(&document(), Some("full"), Some(&ordinary()))
                .is_err()
        );
        assert!(!attempt.observe(&document()));
        assert_eq!(attempt.observe(&filtered(true)), !not_requested);
        assert!(!attempt.finish(false));
    }
}
#[test]
fn checked_unchecked_clear_keep_numeric_sort_raw_population_and_cursor() {
    use crate::slack_lists::{
        RecordsPage,
        saved_views::{self, ProjectionContext, SavedView},
    };
    let rows = vec![
        json!({"id":"R1","position":0,"date_created":1,"fields":[{"key":"done","checkbox":true},{"key":"estimate","number":[12.5]}]}),
        json!({"id":"R2","position":1,"date_created":1,"fields":[{"key":"done","checkbox":false},{"key":"estimate","number":[0]}]}),
        json!({"id":"R3","position":2,"date_created":1,"fields":[{"key":"estimate","number":[-1]}]}),
        json!({"id":"R4","position":3,"date_created":1,"fields":[{"key":"done","checkbox":true},{"key":"estimate","number":[2]}]}),
    ];
    let page = RecordsPage::parse(
        json!({"ok":true,"records":rows,"response_metadata":{"next_cursor":"next-owned-page"}}),
    )
    .unwrap();
    let untouched = page.clone();
    for (doc, expected) in [
        (filtered(true), vec!["R4", "R1"]),
        (filtered(false), vec!["R3", "R2"]),
        (document(), vec!["R3", "R2", "R4", "R1"]),
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

mod number;
