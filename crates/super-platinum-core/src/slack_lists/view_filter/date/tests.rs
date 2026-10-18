use super::super::*;
use super::*;
fn document() -> Document {
    Document::parse(json!({"id":"F1","mode":"list","access":"write","list_metadata":{"schema":[
        {"id":"D","key":"date","name":"Date","type":"date","options":{"date_format":"DD/MM/YYYY","opaque":true}},
        {"id":"N","key":"number","name":"Number","type":"number","options":{"precision":2}},
        {"id":"S","key":"status","name":"Status","type":"select","options":{"format":"single_select","choices":[{"value":"b","label":"B"},{"value":"a","label":"A"}]}},
        {"id":"C","key":"check","name":"Check","type":"checkbox"}],"views":[{"id":"V","name":"Table","type":"table","columns":[{"id":"D","key":"date","visible":true}],"filters":[],"info_column_filters":[],"opaque":"preserve","sorts":[]} ]}})).unwrap()
}
fn prefs() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn choice(op: DateFilterOperator, value: Option<&str>) -> ViewFilterChoice {
    ViewFilterChoice::Date {
        column_id: "D".into(),
        operator: op,
        value: value.map(str::to_owned),
    }
}
fn capture(doc: &Document, desired: ViewFilterChoice) -> Result<ViewFilter, ListError> {
    ViewFilter::capture(doc, "V", desired, Some("full"), Some(&prefs()))
}
#[test]
fn six_operators_and_calendar_or_relative_values_are_lossless() {
    for op in [
        DateFilterOperator::Equal,
        DateFilterOperator::NotEqual,
        DateFilterOperator::LessThan,
        DateFilterOperator::GreaterThan,
        DateFilterOperator::IsEmpty,
        DateFilterOperator::IsNotEmpty,
    ] {
        for value in [
            "0001-01-01",
            "9999-12-31",
            "2024-02-29",
            "today",
            "yesterday",
            "tomorrow",
            "this_week",
            "next_week",
            "last_week",
        ] {
            let doc = document();
            let value = op.requires_value().then_some(value);
            let desired = choice(op, value);
            let intent = capture(&doc, desired.clone()).unwrap();
            assert_eq!(
                intent.filters(),
                &json!([{"key":"date","operator":op.as_str(),"values":value.into_iter().collect::<Vec<_>>()}])
            );
            let mut raw = doc.raw.clone();
            raw["list_metadata"]["views"][0]["filters"] = intent.filters().clone();
            let fresh = Document::parse(raw).unwrap();
            assert_eq!(ViewFilter::current(&fresh, "V"), Some(desired));
            assert!(intent.observes_desired(&fresh));
        }
    }
}
#[test]
fn malformed_noncalendar_and_specialized_saved_rules_cannot_be_overwritten() {
    for value in [
        "",
        " 2024-02-29",
        "2024-02-29 ",
        "2023-02-29",
        "2024-2-09",
        "2024-04-31",
        "0000-01-01",
        "10000-01-01",
        "2024-01-01T00:00:00Z",
        "1700000000",
        "overdue",
        "upcoming",
        "later",
        "since_i_last_viewed",
    ] {
        assert!(
            capture(&document(), choice(DateFilterOperator::Equal, Some(value))).is_err(),
            "{value}"
        );
    }
    for rule in [
        json!({"key":"date","operator":"equal","values":[]}),
        json!({"key":"date","operator":"equal","values":["today","tomorrow"]}),
        json!({"key":"date","operator":"equal","values":[1700000000]}),
        json!({"key":"date","operator":"equal","values":["today"],"column_id":"D"}),
        json!({"key":"date","operator":"contains","values":["today"]}),
        json!({"key":"date","operator":"is_empty","values":["today"]}),
    ] {
        let mut raw = document().raw;
        raw["list_metadata"]["views"][0]["filters"] = json!([rule]);
        let doc = Document::parse(raw).unwrap();
        assert!(ViewFilter::current(&doc, "V").is_none());
        assert!(capture(&doc, ViewFilterChoice::None).is_err());
    }
    for kind in [
        "due_date",
        "todo_due_date",
        "created_time",
        "last_edited_time",
        "text",
        "future",
    ] {
        let mut raw = document().raw;
        raw["list_metadata"]["schema"][0]["type"] = json!(kind);
        let doc = Document::parse(raw).unwrap();
        assert!(!ViewFilter::supports_column(doc.column("D").unwrap()));
        assert!(capture(&doc, choice(DateFilterOperator::Equal, Some("today"))).is_err());
    }
}
#[test]
fn mixed_whole_set_cas_and_readback_remain_exact_without_retry() {
    let doc = document();
    let desired = ViewFilterChoice::All(vec![
        ViewFilterChoice::Checkbox {
            column_id: "C".into(),
            checked: true,
        },
        choice(DateFilterOperator::LessThan, Some("this_week")),
        ViewFilterChoice::Select {
            column_id: "S".into(),
            operator: SelectFilterOperator::Contains,
            values: vec!["b".into(), "a".into()],
        },
        ViewFilterChoice::Number {
            column_id: "N".into(),
            operator: NumberFilterOperator::GreaterThan,
            value: Some("12.50".into()),
        },
    ]);
    let intent = capture(&doc, desired.clone()).unwrap();
    let mut attempt = ViewFilterAttempt::new(intent.clone());
    let mut raw = doc.raw.clone();
    raw["list_metadata"]["schema"][0]["options"]["date_format"] = json!("YYYY-MM-DD");
    assert!(
        attempt
            .admit(&Document::parse(raw).unwrap(), Some("full"), Some(&prefs()))
            .is_err()
    );
    assert!(attempt.admit(&doc, Some("full"), Some(&prefs())).is_ok());
    assert!(attempt.finish(false));
    assert!(attempt.admit(&doc, Some("full"), Some(&prefs())).is_err());
    let mut raw = doc.raw.clone();
    raw["list_metadata"]["views"][0]["filters"] = intent.filters().clone();
    let observed = Document::parse(raw.clone()).unwrap();
    assert_eq!(ViewFilter::current(&observed, "V"), Some(desired));
    assert!(attempt.observe(&observed));
    raw["list_metadata"]["views"][0]["filters"][1]["values"] = json!(["2024-05-20"]);
    assert!(!intent.observes_desired(&Document::parse(raw).unwrap()));
    assert_eq!(
        capture(&observed, ViewFilterChoice::None)
            .unwrap()
            .filters(),
        &json!([])
    );
    assert_eq!(observed.columns, doc.columns);
    assert_eq!(observed.views[0]["opaque"], doc.views[0]["opaque"]);
}

#[test]
fn authored_presets_use_existing_local_calendar_and_iso_week_projection_without_wire_expansion() {
    use crate::slack_lists::{
        RecordsPage,
        saved_views::{self, DateContext, ProjectionContext, SavedView},
    };
    let doc = document();
    let page = RecordsPage::parse(json!({"ok":true,"records":[
        {"id":"SUN","position":0,"fields":[{"key":"date","date":["2026-10-18"]}]},
        {"id":"MON","position":1,"fields":[{"key":"date","date":["2026-10-19"]}]},
        {"id":"MISSING","position":2,"fields":[]}]}))
    .unwrap();
    let before = page.raw.clone();
    for (preset, now, offset, expected) in [
        ("today", "2026-10-18T23:30:00", -4 * 3600, vec!["SUN"]),
        ("today", "2026-10-19T05:30:00", 2 * 3600, vec!["MON"]),
        ("this_week", "2026-10-18T23:30:00", -4 * 3600, vec!["SUN"]),
        ("next_week", "2026-10-18T23:30:00", -4 * 3600, vec!["MON"]),
    ] {
        let intent = capture(&doc, choice(DateFilterOperator::Equal, Some(preset))).unwrap();
        let mut raw = doc.raw.clone();
        raw["list_metadata"]["views"][0]["filters"] = intent.filters().clone();
        let fresh = Document::parse(raw).unwrap();
        let view = SavedView::parse(fresh.views[0].clone()).unwrap();
        let context = ProjectionContext {
            date: Some(DateContext::from_local_with_offset(now, None, offset).unwrap()),
            date_uses_timestamps: Some(false),
            ..Default::default()
        };
        let projected = saved_views::project(&fresh, &page, &view, &context).unwrap();
        let ids = projected
            .groups
            .iter()
            .flat_map(|g| g.rows.iter().map(|r| r.record_id.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(ids, expected);
        assert_eq!(intent.filters()[0]["values"], json!([preset]));
        assert_eq!(page.raw, before);
    }
}
