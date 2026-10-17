use super::*;
use crate::slack_lists::{
    RecordsPage,
    saved_views::{ProjectionContext, SavedView, project},
};

const KINDS: [&str; 12] = [
    "text",
    "number",
    "select",
    "checkbox",
    "date",
    "created_time",
    "last_edited_time",
    "user",
    "channel",
    "rating",
    "email",
    "phone",
];
fn typed(kind: &str) -> Document {
    let mut raw = document().raw;
    raw["list_metadata"]["schema"][1] =
        json!({"id":"C_NUMBER","key":"estimate","name":"Candidate","type":kind,"opaque":7});
    if kind == "number" {
        raw["list_metadata"]["schema"][1]["options"] = json!({"precision":2});
    }
    if kind == "select" {
        raw["list_metadata"]["schema"][1]["options"] = json!({"format":"multi_select","choices":[{"value":"z","label":"Last alphabetically"},{"value":"a","label":"First alphabetically"}],"opaque":9});
    }
    Document::parse(raw).unwrap()
}
fn with_sort(doc: &Document, ascending: bool) -> Document {
    let mut raw = doc.raw.clone();
    raw["list_metadata"]["views"][0]["sorts"] = json!([{"key":"estimate","ascending":ascending}]);
    Document::parse(raw).unwrap()
}
#[test]
fn supported_types_capture_exact_key_direction_and_clear_without_touching_schema() {
    for kind in KINDS {
        for ascending in [false, true] {
            let doc = typed(kind);
            let column = doc.column("C_NUMBER").unwrap();
            let desired = ViewSort::choice(column, ascending).unwrap();
            assert_eq!(
                matches!(desired, ViewSortChoice::Number { .. }),
                kind == "number"
            );
            let intent = capture(&doc, desired.clone());
            assert_eq!(
                intent.sorts(),
                &json!([{"key":"estimate","ascending":ascending}])
            );
            let fresh = with_sort(&doc, ascending);
            assert_eq!(ViewSort::current(&fresh, "V1"), Some(desired.clone()));
            assert!(intent.observes_desired(&fresh));
            assert_eq!(
                fresh.raw["list_metadata"]["schema"],
                doc.raw["list_metadata"]["schema"]
            );
            assert_eq!(capture(&fresh, ViewSortChoice::None).sorts(), &json!([]));
            assert!(
                ViewSort::capture(&fresh, "V1", desired, Some("full"), Some(&ordinary())).is_err()
            );
        }
    }
}
#[test]
fn unsupported_menu_or_comparator_kinds_and_cross_kind_aliases_remain_refused() {
    for kind in [
        "link",
        "vote",
        "currency",
        "rich_text",
        "multi_select",
        "assignee",
        "created_by",
        "last_edited_by",
        "todo_assignee",
        "todo_due_date",
        "todo_completed",
        "completed",
        "due_date",
        "unknown",
    ] {
        let doc = typed(kind);
        assert!(
            !ViewSort::supports_column(doc.column("C_NUMBER").unwrap()),
            "{kind}"
        );
        assert!(ViewSort::choice(doc.column("C_NUMBER").unwrap(), true).is_none());
        assert!(ViewSort::current(&with_sort(&doc, true), "V1").is_none());
        assert!(
            ViewSort::capture(
                &doc,
                "V1",
                ViewSortChoice::Field {
                    column_id: "C_NUMBER".into(),
                    ascending: true
                },
                Some("full"),
                Some(&ordinary())
            )
            .is_err()
        );
    }
    for (kind, desired) in [
        (
            "number",
            ViewSortChoice::Field {
                column_id: "C_NUMBER".into(),
                ascending: true,
            },
        ),
        ("text", choice(true)),
    ] {
        assert!(!ViewSort::valid_choice(&typed(kind), &desired));
        assert!(
            ViewSort::capture(&typed(kind), "V1", desired, Some("full"), Some(&ordinary()))
                .is_err()
        );
    }
}
#[test]
fn select_rank_table_is_checked_without_normalizing_or_replacing_options() {
    for options in [
        json!({"format":"single_entity"}),
        json!({"choices":null}),
        json!({"choices":[{"value":"z","label":"Z"},{"value":"z","label":"Other"}]}),
        json!({"choices":[{"value":"a|b","label":"AB"}]}),
        json!({"choices":[{"value":"a","label":null}]}),
    ] {
        let mut raw = typed("select").raw;
        raw["list_metadata"]["schema"][1]["options"] = options;
        let doc = Document::parse(raw).unwrap();
        assert!(!ViewSort::supports_column(doc.column("C_NUMBER").unwrap()));
    }
    for format in ["single_select", "multi_select"] {
        let mut raw = typed("select").raw;
        raw["list_metadata"]["schema"][1]["options"]["format"] = json!(format);
        let doc = Document::parse(raw).unwrap();
        let desired = ViewSort::choice(doc.column("C_NUMBER").unwrap(), true).unwrap();
        assert!(capture(&doc, desired).observes_desired(&with_sort(&doc, true)));
    }
}
#[test]
fn every_new_type_retains_full_metadata_staleness_and_one_attempt_observation() {
    for kind in KINDS {
        let doc = typed(kind);
        let intent = capture(
            &doc,
            ViewSort::choice(doc.column("C_NUMBER").unwrap(), true).unwrap(),
        );
        let mut changed = doc.raw.clone();
        changed["list_metadata"]["schema"][1]["opaque"] = json!(99);
        assert!(
            intent
                .revalidate(
                    &Document::parse(changed).unwrap(),
                    Some("full"),
                    Some(&ordinary())
                )
                .is_err()
        );
        let mut attempt = ViewSortAttempt::new(intent);
        attempt
            .admit(&doc, Some("full"), Some(&ordinary()))
            .unwrap();
        assert!(attempt.finish(false));
        assert!(!attempt.observe(&doc));
        assert!(
            attempt
                .admit(&doc, Some("full"), Some(&ordinary()))
                .is_err()
        );
        let fresh = with_sort(&doc, true);
        let mut changed = fresh.raw.clone();
        changed["list_metadata"]["views"][0]["opaque_view"] = json!(99);
        assert!(!attempt.observe(&Document::parse(changed).unwrap()));
        assert!(attempt.observe(&fresh));
        assert_eq!(attempt.phase(), ViewSortPhase::Observed);
    }
}
fn field(kind: &str, high: bool) -> Value {
    let mut result = json!({"key":"estimate"});
    let member = match kind {
        "text" => "rich_text",
        "date" => "date",
        "created_time" | "last_edited_time" => "timestamp",
        other => other,
    };
    result[member] = match kind {
        "text" => {
            json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":if high {"Zulu"}else{"alpha"}}]}]}])
        }
        "number" | "rating" => json!([if high { 9 } else { 1 }]),
        "checkbox" => json!(high),
        "select" => {
            if high {
                json!(["a"])
            } else {
                json!(["a", "z"])
            }
        }
        "date" => json!([if high { "2026-10-20" } else { "2026-10-10" }]),
        "created_time" | "last_edited_time" => json!([if high { 2000 } else { 1000 }]),
        "user" => json!([if high { "U_Z" } else { "U_A" }]),
        "channel" => json!([if high { "C_Z" } else { "C_A" }]),
        _ => json!([if high { "z" } else { "a" }]),
    };
    result
}
#[test]
fn authored_rules_use_existing_typed_comparators_and_stable_loaded_order() {
    let mut context = ProjectionContext::default();
    context.user_real_names.insert("U_A".into(), "Alpha".into());
    context.user_real_names.insert("U_Z".into(), "Zulu".into());
    context.channel_names.insert("C_A".into(), "alpha".into());
    context.channel_names.insert("C_Z".into(), "zulu".into());
    for kind in KINDS {
        let page = RecordsPage::parse(json!({"ok":true,"records":[
            {"id":"HIGH","position":1,"fields":[field(kind,true)]},
            {"id":"LOW_1","position":2,"fields":[field(kind,false)]},
            {"id":"LOW_2","position":3,"fields":[field(kind,false)]}
        ],"response_metadata":{"next_cursor":"opaque-unloaded"}}))
        .unwrap();
        let before = page.clone();
        for ascending in [true, false] {
            let doc = typed(kind);
            let intent = capture(
                &doc,
                ViewSort::choice(doc.column("C_NUMBER").unwrap(), ascending).unwrap(),
            );
            let mut raw = doc.raw.clone();
            raw["list_metadata"]["views"][0]["sorts"] = intent.sorts().clone();
            let doc = Document::parse(raw).unwrap();
            let view = SavedView::parse(doc.views[0].clone()).unwrap();
            let projected = project(&doc, &page, &view, &context).unwrap();
            let ids = projected
                .groups
                .iter()
                .flat_map(|g| g.rows.iter().map(|r| r.record_id.as_str()))
                .collect::<Vec<_>>();
            assert_eq!(
                ids,
                if ascending {
                    vec!["LOW_1", "LOW_2", "HIGH"]
                } else {
                    vec!["HIGH", "LOW_1", "LOW_2"]
                },
                "{kind}"
            );
            assert!(projected.incomplete);
            assert_eq!(page, before);
        }
    }
}
