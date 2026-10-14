use super::*;
use chrono::DateTime;
use serde_json::json;

fn column() -> Column {
    Column {
        id: "C_TIME".into(),
        key: "edited".into(),
        name: "Changed at".into(),
        kind: "last_edited_time".into(),
        raw: json!({"id":"C_TIME","key":"edited","type":"last_edited_time"}),
    }
}
fn record(value: Value) -> Record {
    Record::parse(json!({"id":"R_TIME","updated_timestamp":"HLC-not-a-date","fields":[{"key":"edited","timestamp":[value],"text":"raw epoch fallback"}]})).unwrap()
}
fn context(now: &str, offset: i32, locale: &str, time24: bool) -> TimestampContext {
    TimestampContext::new(
        DateContext::from_local_with_offset(now, None, offset).unwrap(),
        Some(locale),
        time24,
    )
}
fn strings(column: &Column, record: &Record, context: &TimestampContext) -> (String, String) {
    let Some(TimestampPresentation::Value { label, tooltip }) = project(column, record, context)
    else {
        panic!("timestamp expected")
    };
    (label, tooltip)
}

#[test]
fn timestamp_labels_and_tooltips_match_72_recovered_source_cases() {
    // Executed exact B 9Y08 + S CFnj/BIeE/GS5V/NIxh + captured P Dayjs
    // locale modules, with Date.now frozen and only English message join stubbed.
    let cases: Vec<Value> = serde_json::from_str(include_str!("source-oracle.json")).unwrap();
    for case in cases {
        let context = context(
            "2026-10-13T12:00:00",
            0,
            case["locale"].as_str().unwrap(),
            case["time24"].as_bool().unwrap(),
        );
        let stamp = DateTime::parse_from_rfc3339(case["stamp"].as_str().unwrap())
            .unwrap()
            .timestamp();
        let (label, tooltip) = strings(&column(), &record(json!(stamp)), &context);
        assert_eq!(label, case["label"], "{case}");
        assert_eq!(tooltip, case["tooltip"], "{case}");
    }
}

#[test]
fn display_unit_threshold_and_fractional_timeclip_match_date_fast_path() {
    assert_eq!(milliseconds(1_700_000_000.125), Some(1_700_000_000_125));
    assert_eq!(milliseconds(1_700_000_000_125.9), Some(1_700_000_000_125));
    assert_eq!(milliseconds(9_999_999_999.0), Some(9_999_999_999_000));
    assert_eq!(milliseconds(10_000_000_000.0), Some(10_000_000_000));
    assert_eq!(milliseconds(-0.0009), Some(0));
    assert_eq!(milliseconds(-0.0019), Some(-1));
    assert_eq!(
        milliseconds(8_640_000_000_000_000.0),
        Some(8_640_000_000_000_000)
    );
    for value in [f64::NAN, f64::INFINITY, 8_640_000_000_000_001.0] {
        assert_eq!(milliseconds(value), None);
    }
    let context = context("2026-10-13T12:00:00", 0, "en-US", false);
    let seconds = DateTime::parse_from_rfc3339("2026-10-13T09:30:05Z")
        .unwrap()
        .timestamp();
    let expected = strings(&column(), &record(json!(seconds)), &context);
    for value in [
        json!(seconds * 1000),
        json!(seconds.to_string()),
        json!(format!("{}.0001", seconds)),
    ] {
        assert_eq!(strings(&column(), &record(value), &context), expected);
    }
}

#[test]
fn relative_thresholds_round_like_source_and_future_stays_ago() {
    for (seconds, expected) in [
        (44.0, "< 1 minute ago"),
        (45.0, "1 minute ago"),
        (89.0, "1 minute ago"),
        (90.0, "2 minutes ago"),
        (2699.0, "45 minutes ago"),
        (2700.0, "1 hour ago"),
        (5399.0, "1 hour ago"),
        (5400.0, "2 hours ago"),
        (86399.0, "24 hours ago"),
        (86400.0, "1 day ago"),
        (172800.0, "2 days ago"),
        (2592000.0, "1 month ago"),
        (3942000.0, "2 months ago"),
        (31536000.0, "1 year ago"),
        (47304000.0, "2 years ago"),
    ] {
        assert_eq!(relative(seconds, "en-US"), expected);
    }
    let context = context("2026-10-13T12:00:00", 0, "en-US", false);
    let now = context.calendar.now().timestamp_millis();
    for delta in [-45_999, 45_999] {
        assert_eq!(
            strings(&column(), &record(json!(now + delta)), &context).0,
            "1 minute ago"
        );
    }
    for delta in [-44_999, 44_999] {
        assert_eq!(
            strings(&column(), &record(json!(now + delta)), &context).0,
            "< 1 minute ago"
        );
    }
}

#[test]
fn local_calendar_day_year_rollover_offset_and_time_preference_are_explicit() {
    let stamp = DateTime::parse_from_rfc3339("2026-10-14T03:30:05Z")
        .unwrap()
        .timestamp();
    let row = record(json!(stamp));
    assert_eq!(
        strings(
            &column(),
            &row,
            &context("2026-10-13T12:00:00", -14400, "en-US", false)
        )
        .1,
        "Today at 11:30:05 PM"
    );
    assert_eq!(
        strings(
            &column(),
            &row,
            &context("2026-10-13T12:00:00", 0, "en-US", true)
        )
        .1,
        "Tomorrow at 03:30:05"
    );
    for (stamp, expected) in [
        ("2025-12-20T12:00:00Z", "Dec 20th at 12:00:00 PM"),
        ("2025-12-01T12:00:00Z", "Dec 1st, 2025 at 12:00:00 PM"),
        ("2027-01-01T12:00:00Z", "Jan 1st, 2027 at 12:00:00 PM"),
    ] {
        let stamp = DateTime::parse_from_rfc3339(stamp).unwrap().timestamp();
        assert_eq!(
            strings(
                &column(),
                &record(json!(stamp)),
                &context("2026-01-02T12:00:00", 0, "en-US", false)
            )
            .1,
            expected
        );
    }
    assert_eq!(
        strings(
            &column(),
            &row,
            &context("2026-10-13T12:00:00", 0, "unknown", false)
        ),
        strings(
            &column(),
            &row,
            &context("2026-10-13T12:00:00", 0, "en-US", false)
        )
    );
}

#[test]
fn canonical_creation_value_wins_and_display_never_changes_edit_or_raw_state() {
    let context = context("2026-10-13T12:00:00", 0, "en-US", false);
    let mut column = column();
    column.kind = "created_time".into();
    column.key = "created_time".into();
    let row=Record::parse(json!({"id":"R_TIME","date_created":1791889200,"fields":[{"key":"created_time","timestamp":[1]}]})).unwrap();
    let before = row.clone();
    assert!(matches!(
        project(&column, &row, &context),
        Some(TimestampPresentation::Value { .. })
    ));
    assert_eq!(row, before);
    assert!(!column.editable_value(&row, "F_LIST", None));
    assert!(column.edit_value(&row, "F_LIST", None).is_err());
    let expected = strings(&column, &row, &context);
    let mut other = row.clone();
    other.fields.get_mut("created_time").unwrap()["timestamp"] = json!([9999999999_u64]);
    assert_eq!(strings(&column, &other, &context), expected);
    column.kind = "date".into();
    assert!(project(&column, &row, &context).is_none());
}

#[test]
fn missing_zero_malformed_and_conflicting_values_do_not_render_raw_epoch() {
    let context = context("2026-10-13T12:00:00", 0, "en-US", false);
    for value in [Value::Null, json!(0), json!(-0.0), json!("")] {
        assert_eq!(
            project(&column(), &record(value), &context),
            Some(TimestampPresentation::Empty)
        );
    }
    assert!(matches!(
        project(&column(), &record(json!("0")), &context),
        Some(TimestampPresentation::Value { .. })
    ));
    for value in [
        json!(true),
        json!({}),
        json!("1710000000oops"),
        json!("NaN"),
        json!("   "),
        json!("x".repeat(129)),
        json!(9e18),
    ] {
        assert!(matches!(
            project(&column(), &record(value), &context),
            Some(TimestampPresentation::Unsupported { .. })
        ));
    }
    for field in [
        json!({"key":"edited","timestamp":[1,2]}),
        json!({"key":"edited","timestamp":123}),
        json!({"key":"edited","timestamp":[123],"number":[1]}),
        json!({"key":"edited","timestamp":[123],"column_id":"WRONG"}),
    ] {
        let row = Record::parse(json!({"id":"R_TIME","fields":[field]})).unwrap();
        assert!(matches!(
            project(&column(), &row, &context),
            Some(TimestampPresentation::Unsupported { .. })
        ));
    }
}
