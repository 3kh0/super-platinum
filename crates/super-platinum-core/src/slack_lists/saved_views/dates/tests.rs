use super::*;
use serde_json::json;
fn fixture() -> (Document, RecordsPage, SavedView, ProjectionContext) {
    let document=Document::parse(json!({"id":"F1","mode":"list","list_metadata":{"schema":[{"id":"CD","key":"due","name":"Due","type":"date"}],"views":[{"id":"V1","name":"Dates","type":"table","columns":[{"id":"CD","key":"due","visible":true}],"filters":[],"sorts":[]}]}})).unwrap();
    let rows = [
        ("missing", json!({})),
        ("later", json!({"timestamp":[0]})),
        ("yesterday", json!({"date":["2026-10-12"]})),
        ("today", json!({"date":["2026-10-13"],"timestamp":[-1]})),
        ("tomorrow", json!({"date":["2026-10-14"]})),
        ("boundary", json!({"date":["2026-10-15"]})),
        ("after", json!({"date":["2026-10-16"]})),
        ("sunday", json!({"date":["2026-10-18"]})),
        ("monday", json!({"date":["2026-10-19"]})),
    ];
    let records = rows
        .into_iter()
        .enumerate()
        .map(|(i, (id, mut field))| {
            field["key"] = json!("due");
            field["column_id"] = json!("CD");
            json!({"id":id,"position":i,"fields":[field]})
        })
        .collect::<Vec<_>>();
    let page = RecordsPage::parse(json!({"ok":true,"records":records})).unwrap();
    let view = SavedView::parse(document.views[0].clone()).unwrap();
    let context = ProjectionContext {
        date: Some(
            DateContext::from_local("2026-10-13T12:00:00", Some("2026-10-13T10:00:00")).unwrap(),
        ),
        date_uses_timestamps: Some(false),
        ..Default::default()
    };
    (document, page, view, context)
}
fn run(op: &str, values: Value) -> Vec<String> {
    let (mut doc, page, mut view, context) = fixture();
    view.raw["filters"] = json!([{"key":"due","column_id":"CD","operator":op,"values":values}]);
    doc.views[0] = view.raw.clone();
    ids(&project(&doc, &page, &view, &context).unwrap())
}
fn ids(projection: &Projection) -> Vec<String> {
    projection
        .groups
        .iter()
        .flat_map(|g| g.rows.iter().map(|r| r.record_id.clone()))
        .collect()
}
#[test]
fn captured_presets_iso_weeks_zero_and_strict_boundaries() {
    assert_eq!(run("equal", json!(["today"])), ["today"]);
    assert_eq!(
        run("equal", json!(["this_week"])),
        [
            "yesterday",
            "today",
            "tomorrow",
            "boundary",
            "after",
            "sunday"
        ]
    );
    assert_eq!(run("equal", json!(["next_week"])), ["monday"]);
    assert_eq!(
        run("equal", json!(["upcoming"])),
        ["after", "sunday", "monday"]
    );
    let negative = run("not_equal", json!(["upcoming"]));
    assert!(negative.contains(&"today".into()));
    assert!(!negative.contains(&"boundary".into()));
    assert!(!negative.contains(&"missing".into()));
    assert_eq!(run("contains", json!(["overdue"])), ["yesterday", "today"]);
    assert_eq!(run("equal", json!(["later"])), ["later"]);
    assert_eq!(run("is_empty", json!([])), ["missing"]);
    assert!(!run("is_not_empty", json!([])).contains(&"missing".into()));
    assert!(!run("not_equal", json!(["later"])).contains(&"missing".into()));
}
#[test]
fn negated_values_are_some_not_a_negated_union_and_first_falsy_disables_rule() {
    let values = run("not_equal", json!(["2026-10-13", "2026-10-14"]));
    assert!(values.contains(&"today".into()) && values.contains(&"tomorrow".into()));
    assert_eq!(run("equal", json!(["", "today"])).len(), 9);
    assert_eq!(run("equal", json!([0, "today"])).len(), 9);
    assert_eq!(run("equal", json!([null, "today"])).len(), 9);
    assert_eq!(run("equal", json!([false, "today"])).len(), 9);
    assert_eq!(run("equal", json!(["today", null, false])), ["today"]);
}
#[test]
fn missing_and_later_sort_last_in_both_directions_with_distinct_order() {
    let (mut doc, page, mut view, context) = fixture();
    for ascending in [true, false] {
        view.raw["sorts"] = json!([{"key":"due","ascending":ascending}]);
        doc.views[0] = view.raw.clone();
        let sorted = ids(&project(&doc, &page, &view, &context).unwrap());
        assert_eq!(&sorted[7..], ["missing", "later"]);
        assert_eq!(sorted[0], if ascending { "yesterday" } else { "monday" });
    }
}
#[test]
fn timestamp_week_negation_retains_captured_noon_quirk_and_since_precision() {
    let (mut doc, _, mut view, mut context) = fixture();
    let noon = DateContext::from_local("2026-10-13T12:00:00", None)
        .unwrap()
        .now
        .timestamp();
    let page = RecordsPage::parse(
        json!({"ok":true,"records":[{"id":"noon","fields":[{"key":"due","timestamp":[noon]}]}]}),
    )
    .unwrap();
    for op in ["equal", "not_equal"] {
        view.raw["filters"] = json!([{"key":"due","operator":op,"values":["this_week"]}]);
        doc.views[0] = view.raw.clone();
        assert_eq!(
            ids(&project(&doc, &page, &view, &context).unwrap()),
            ["noon"]
        );
    }
    view.raw["filters"] =
        json!([{"key":"due","operator":"equal","values":["since_i_last_viewed"]}]);
    doc.views[0] = view.raw.clone();
    assert_eq!(
        ids(&project(&doc, &page, &view, &context).unwrap()),
        ["noon"]
    );
    context.date =
        Some(DateContext::from_local("2026-10-13T13:00:00", Some("2026-10-13T12:00:00")).unwrap());
    assert!(ids(&project(&doc, &page, &view, &context).unwrap()).is_empty());
}
#[test]
fn local_offset_calendar_and_prior_read_conversion_are_explicit() {
    let context =
        DateContext::from_local_with_offset("2026-10-13T00:30:00", None, -4 * 3600).unwrap();
    let prior = context.with_last_read_timestamp(Some(1791858600));
    assert_eq!(
        prior.last_read.unwrap().offset().local_minus_utc(),
        -4 * 3600
    );
    assert_eq!(context.calendar_key(), "2026-10-13:-14400");
    let utc = DateContext::from_local_with_offset("2026-10-13T00:30:00", None, 0).unwrap();
    assert_ne!(utc.calendar_key(), context.calendar_key());
    let (doc, _, _, mut projection) = fixture();
    projection.date = Some(context.clone());
    let seconds = context.now.timestamp() - 3600;
    let row =
        Record::parse(json!({"id":"R","fields":[{"key":"due","timestamp":[seconds]}]})).unwrap();
    let filter = model::Filter {
        key: "due".into(),
        column_id: None,
        operator: "equal".into(),
        values: vec![json!("yesterday")],
    };
    assert!(filter_row(&doc.columns[0], &row, &filter, &projection));
}
fn filter_row(
    column: &Column,
    row: &Record,
    rule: &model::Filter,
    context: &ProjectionContext,
) -> bool {
    super::filter(column, row, rule, context).unwrap()
}
#[test]
fn decoder_policy_preserves_date_sentinel_and_rejects_unknown_ambiguous_preference() {
    let (doc, _, _, mut context) = fixture();
    let column = &doc.columns[0];
    let row = Record::parse(
        json!({"id":"R","fields":[{"key":"due","date":["2026-10-13"],"timestamp":[-1]}]}),
    )
    .unwrap();
    context.date_uses_timestamps = None;
    assert!(matches!(
        value(column, &row, &context).unwrap(),
        DateValue::Day(_)
    ));
    let row=Record::parse(json!({"id":"R","date_due":123,"fields":[{"key":"due","date":["2026-10-13"],"timestamp":[1791900000]}]})).unwrap();
    assert!(value(column, &row, &context).is_err());
    context.date_uses_timestamps = Some(false);
    assert!(matches!(
        value(column, &row, &context).unwrap(),
        DateValue::Day(_)
    ));
    context.date_uses_timestamps = Some(true);
    assert!(matches!(
        value(column, &row, &context).unwrap(),
        DateValue::Timestamp(1791900000)
    ));
}
#[test]
fn unknown_context_and_malformed_dates_reject_even_empty_pages() {
    let (mut doc, _, mut view, mut context) = fixture();
    let page = RecordsPage::parse(json!({"ok":true,"records":[]})).unwrap();
    for v in ["today", "since_i_last_viewed", "2026-02-30"] {
        view.raw["filters"] = json!([{"key":"due","operator":"equal","values":[v]}]);
        doc.views[0] = view.raw.clone();
        context.date = None;
        assert!(project(&doc, &page, &view, &context).is_err());
    }
    view.raw["filters"] =
        json!([{"key":"due","operator":"equal","values":["overdue","since_i_last_viewed"]}]);
    doc.views[0] = view.raw.clone();
    assert_eq!(
        date_dependencies(&doc, &view),
        DateDependencies {
            calendar: true,
            clock: true,
            prior_read: true
        }
    );
}

#[test]
fn ordinary_due_and_todo_due_share_live_date_sentinel_shape() {
    for kind in ["date", "due_date", "todo_due_date"] {
        let (mut doc, page, mut view, mut context) = fixture();
        doc.columns[0].kind = kind.into();
        context.todo_mode = true;
        context.apply_todo_completion_rules = Some(false);
        view.raw["filters"] = json!([{"key":"due","column_id":"CD","operator":"equal","typed_values":[],"values":["today"]}]);
        doc.views[0] = view.raw.clone();
        assert_eq!(
            ids(&project(&doc, &page, &view, &context).unwrap()),
            ["today"]
        );
        if kind == "todo_due_date" {
            context.todo_mode = false;
            assert_eq!(
                ids(&project(&doc, &page, &view, &context).unwrap()).len(),
                9
            );
        }
    }
}
#[test]
fn created_time_uses_record_metadata_but_last_edited_does_not_invent_a_fallback() {
    let (mut doc, _, _, context) = fixture();
    let column = &mut doc.columns[0];
    column.kind = "created_time".into();
    column.key = "created_time".into();
    let row = Record::parse(json!({"id":"R","date_created":123,"updated_timestamp":999,"fields":[{"key":"created_time","timestamp":[456]}]})).unwrap();
    assert!(matches!(
        value(column, &row, &context).unwrap(),
        DateValue::Timestamp(123)
    ));
    column.kind = "last_edited_time".into();
    column.key = "last_edited_time".into();
    assert!(matches!(
        value(column, &row, &context).unwrap(),
        DateValue::Missing
    ));
}
#[test]
fn exact_timestamp_overdue_and_first_since_override_keep_strict_comparisons() {
    let (doc, _, _, context) = fixture();
    let now = context.date.as_ref().unwrap().now.timestamp();
    let make = |seconds| {
        Record::parse(json!({"id":"R","fields":[{"key":"due","timestamp":[seconds]}]})).unwrap()
    };
    for (seconds, expected) in [(now - 1, true), (now, false), (now + 1, false)] {
        let rule = model::Filter {
            key: "due".into(),
            column_id: None,
            operator: "equal".into(),
            values: vec![json!("overdue")],
        };
        assert_eq!(
            filter_row(&doc.columns[0], &make(seconds), &rule, &context),
            expected
        );
    }
    let rule = model::Filter {
        key: "due".into(),
        column_id: None,
        operator: "not_equal".into(),
        values: vec![json!("since_i_last_viewed")],
    };
    assert!(filter_row(&doc.columns[0], &make(now), &rule, &context));
    assert!(!filter_row(
        &doc.columns[0],
        &make(now - 3 * 3600),
        &rule,
        &context
    ));
    // In a later value position the preset does not force GreaterThan or retain
    // time precision: the same calendar date satisfies Equal.
    let rule = model::Filter {
        key: "due".into(),
        column_id: None,
        operator: "equal".into(),
        values: vec![json!("2026-10-12"), json!("since_i_last_viewed")],
    };
    assert!(filter_row(
        &doc.columns[0],
        &make(now - 3 * 3600),
        &rule,
        &context
    ));
}
#[test]
fn malformed_or_replaced_cell_never_masquerades_as_empty_and_sort_is_stable() {
    let (doc, _, _, context) = fixture();
    for field in [
        json!({"key":"due","date":["2026-02-30"]}),
        json!({"key":"due","date":["2026-10-13","2026-10-14"]}),
        json!({"key":"due","timestamp":[false]}),
        json!({"key":"due","date":["2026-10-13"],"column_id":"OLD"}),
        json!({"key":"due","date":["2026-10-13"],"row_id":"OLD"}),
    ] {
        let row = Record::parse(json!({"id":"R","fields":[field]})).unwrap();
        assert!(value(&doc.columns[0], &row, &context).is_err());
    }
    let (mut doc, _, mut view, context) = fixture();
    let page = RecordsPage::parse(json!({"ok":true,"records":[
        {"id":"second","position":2,"fields":[{"key":"due","date":["2026-10-13"]}]},
        {"id":"first","position":1,"fields":[{"key":"due","date":["2026-10-13"]}]}
    ]}))
    .unwrap();
    view.raw["sorts"] = json!([{"key":"due","ascending":true}]);
    doc.views[0] = view.raw.clone();
    assert_eq!(
        ids(&project(&doc, &page, &view, &context).unwrap()),
        ["first", "second"]
    );
}

#[test]
fn later_and_empty_filters_need_no_clock_or_prior_read() {
    let (mut doc, page, mut view, mut context) = fixture();
    context.date = None;
    for (operator, values, expected) in [
        ("equal", json!(["later"]), vec!["later"]),
        ("is_empty", json!([]), vec!["missing"]),
    ] {
        view.raw["filters"] = json!([{"key":"due","operator":operator,"values":values}]);
        doc.views[0] = view.raw.clone();
        assert_eq!(
            ids(&project(&doc, &page, &view, &context).unwrap()),
            expected
        );
    }
}
