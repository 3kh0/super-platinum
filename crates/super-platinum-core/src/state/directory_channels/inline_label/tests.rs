use super::*;
use chrono::{FixedOffset, LocalResult, NaiveDate, NaiveDateTime, Utc};
use serde_json::json;

fn channel() -> Value {
    json!({"id":"C123","name":"Shared 中🦊","is_channel":true,
        "is_frozen":true,"grace_period_end":1,"unlinked":1_790_000_000})
}
fn plan(start: impl Into<Value>) -> Value {
    let start: Value = start.into();
    json!({"reconnect_grace_period_start_ts":start})
}
fn utc(ms: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ms).unwrap()
}
fn standard(channel: &Value, plan: Option<&Value>) -> Result<Policy, Error> {
    project(channel, plan, &utc(1_790_000_000_000))
}

#[test]
fn frozen_icon_and_archived_suffix_are_independent() {
    let mut raw = channel();
    let expired = plan(1);
    let active = standard(&raw, Some(&expired)).unwrap();
    assert!(active.public && active.archive_icon);
    assert!(!active.private && !active.archived_suffix);
    raw["is_archived"] = json!(true);
    let archived = standard(&raw, Some(&expired)).unwrap();
    assert!(archived.archive_icon && archived.archived_suffix);
    raw["is_private"] = json!(true);
    let private = standard(&raw, Some(&expired)).unwrap();
    assert!(private.private && !private.public);
    assert!(private.archive_icon && private.archived_suffix);
}

#[test]
fn in_grace_suppresses_both_archive_decisions() {
    let mut raw = channel();
    raw["is_archived"] = json!(true);
    let live = standard(&raw, Some(&plan(1_790_000_000))).unwrap();
    assert!(live.in_grace && live.grace_eligible);
    assert!(!live.archive_icon && !live.archived_suffix);
    assert!(live.public && !live.private);
    raw["is_group"] = json!(true);
    let private = standard(&raw, Some(&plan(1_790_000_000))).unwrap();
    assert!(private.in_grace && private.private && !private.public);
}

#[test]
fn shared_flags_and_connected_teams_never_change_base_policy() {
    let base = channel();
    let expected = standard(&base, Some(&plan(1))).unwrap();
    for key in ["is_shared", "is_ext_shared", "is_org_shared"] {
        let mut changed = base.clone();
        changed[key] = json!(true);
        changed["connected_teams"] = json!([{"id":"T2","name":"External"}]);
        assert_eq!(standard(&changed, Some(&plan(1))).unwrap(), expected);
    }
}

#[test]
fn structural_false_needs_no_plan_observation() {
    for (key, value) in [
        ("is_frozen", json!(false)),
        ("unlinked", Value::Null),
        ("unlinked", json!(0)),
        ("unlinked", json!(false)),
        ("unlinked", json!("")),
        ("grace_period_end", Value::Null),
        ("grace_period_end", json!(false)),
        ("grace_period_end", json!(0)),
    ] {
        let mut raw = channel();
        raw[key] = value;
        let result = standard(&raw, None).unwrap();
        assert!(!result.in_grace && result.grace_end_ms.is_none());
    }
    let mut raw = channel();
    raw.as_object_mut().unwrap().remove("unlinked");
    assert!(!standard(&raw, None).unwrap().in_grace);
    let mut raw = channel();
    raw.as_object_mut().unwrap().remove("grace_period_end");
    assert!(!standard(&raw, None).unwrap().in_grace);
}

#[test]
fn private_grace_does_not_require_public_end_gate() {
    let mut raw = channel();
    raw["is_private"] = json!(true);
    raw.as_object_mut().unwrap().remove("grace_period_end");
    assert_eq!(standard(&raw, None), Err(Error::MissingPlan));
    assert!(standard(&raw, Some(&plan(1_790_000_000))).unwrap().in_grace);
}

#[test]
fn unobserved_plan_is_not_an_observed_default_zero() {
    assert_eq!(standard(&channel(), None), Err(Error::MissingPlan));
    for value in [json!({}), plan(Value::Null), plan(0), plan(-0.0)] {
        let result = standard(&channel(), Some(&value)).unwrap();
        assert!(!result.in_grace && result.grace_end_ms.is_none());
    }
    assert_eq!(
        standard(&channel(), Some(&Value::Null)),
        Err(Error::UnsupportedTimestamp)
    );
}

#[test]
fn irrelevant_malformed_numeric_fields_are_not_read_before_short_circuit() {
    let mut raw = channel();
    raw["unlinked"] = json!("unsupported if evaluated");
    raw["grace_period_end"] = Value::Null;
    assert!(standard(&raw, None).is_ok());
    raw["grace_period_end"] = json!(true);
    assert!(standard(&raw, Some(&plan(0))).is_ok());
    assert_eq!(
        standard(&raw, Some(&plan(1))),
        Err(Error::UnsupportedTimestamp)
    );
}

#[test]
fn malformed_numeric_operands_are_unavailable_not_false_grace() {
    for value in [json!("1790000000"), json!(true), json!([]), json!({})] {
        assert_eq!(
            standard(&channel(), Some(&plan(value.clone()))),
            Err(Error::UnsupportedTimestamp)
        );
        let mut raw = channel();
        raw["unlinked"] = value;
        assert_eq!(
            standard(&raw, Some(&plan(1_790_000_000))),
            Err(Error::UnsupportedTimestamp)
        );
    }
}

#[test]
fn original_mixed_unit_comparison_is_not_repaired_to_seconds() {
    let mut raw = channel();
    raw["unlinked"] = json!(2000);
    // 2000 seconds is before 1,000,000 seconds, but after 1,000,000 ms.
    let result = project(&raw, Some(&plan(1_000_000)), &utc(1_000_000_000)).unwrap();
    assert!(result.grace_eligible && result.in_grace);
    raw["unlinked"] = json!(999);
    let result = project(&raw, Some(&plan(1_000_000)), &utc(1_000_000_000)).unwrap();
    assert!(!result.grace_eligible && !result.in_grace);
    assert!(result.archive_icon);
}

#[test]
fn strict_raw_equality_remains_a_distinct_grace_admission() {
    let mut raw = channel();
    raw["unlinked"] = json!(-1);
    // -1000 ms is not after -1 ms; strict raw equality is the accepting arm.
    let result = project(&raw, Some(&plan(-1)), &utc(-1000)).unwrap();
    assert!(result.grace_eligible && result.in_grace);
    raw["unlinked"] = json!(-2);
    assert!(
        !project(&raw, Some(&plan(-1)), &utc(-1000))
            .unwrap()
            .grace_eligible
    );
}

#[test]
fn dates_truncate_fractional_milliseconds_on_both_comparison_operands() {
    assert_eq!(time::clip(1.9), Ok(1));
    assert_eq!(time::clip(-1.9), Ok(-1));
    assert_eq!(time::clip(-0.9), Ok(0));
    let mut raw = channel();
    raw["unlinked"] = json!(0.0019);
    // Date(1.9ms) is Date(1ms); both comparison dates are exactly equal.
    let result = project(&raw, Some(&plan(1.9)), &utc(1900)).unwrap();
    assert!(!result.grace_eligible);
    raw["unlinked"] = json!(0.002);
    assert!(
        project(&raw, Some(&plan(1.9)), &utc(1900))
            .unwrap()
            .grace_eligible
    );
}

#[test]
fn expiry_is_strict_and_recheck_does_not_change_captured_provenance() {
    let result = standard(&channel(), Some(&plan(1_790_000_000))).unwrap();
    let end = result.grace_end_ms.unwrap();
    assert_eq!(end, 1_790_000_000_000 + 5 * 86_400_000);
    assert!(result.at_millis(end - 1).unwrap().in_grace);
    let expired = result.at_millis(end).unwrap();
    assert!(!expired.in_grace && expired.archive_icon);
    assert!(expired.grace_eligible);
    assert_eq!(expired.captured_now_ms, result.captured_now_ms);
    assert!(!expired.at_millis(end + 1).unwrap().in_grace);
    assert!(expired.at_millis(end - 1).unwrap().in_grace);
}

#[test]
fn date_limits_and_nonfinite_values_refuse_without_panics() {
    for value in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 8.64e15 + 1.0] {
        assert_eq!(time::clip(value), Err(Error::UnsupportedTimestamp));
    }
    assert_eq!(time::clip(8.64e15), Ok(8_640_000_000_000_000));
    assert!(time::valid_instant(8_640_000_000_000_000).is_err());
    assert!(standard(&channel(), Some(&plan(8.64e15))).is_err());
    let start = chrono::NaiveDate::MAX
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp_millis();
    assert_eq!(
        time::five_days(&Utc, start),
        Err(Error::UnsupportedLocalTime)
    );
}

#[test]
fn special_channels_remain_outside_ordinary_label_policy() {
    for key in ["is_im", "is_mpim", "is_file", "isUnknown", "isNonExistent"] {
        let mut raw = channel();
        raw[key] = json!(true);
        assert_eq!(standard(&raw, None), Err(Error::UnsupportedChannel));
    }
    for property in [
        json!({"huddles":{"is_huddle_channel":true}}),
        json!({"meetings_channel":{"is_meeting_channel":true}}),
        json!({"record_channel":{"record_id":"REC"}}),
        json!({"huddles":"malformed"}),
    ] {
        let mut raw = channel();
        raw["properties"] = property;
        assert_eq!(standard(&raw, None), Err(Error::UnsupportedChannel));
    }
    for name in [json!(""), json!("mc:meeting"), Value::Null] {
        let mut raw = channel();
        raw["name"] = name;
        assert_eq!(standard(&raw, None), Err(Error::UnsupportedChannel));
    }
}

#[derive(Clone)]
struct Eastern2026;
fn wall(value: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f").unwrap()
}
fn offset(hours: i32) -> FixedOffset {
    FixedOffset::east_opt(hours * 3600).unwrap()
}
impl TimeZone for Eastern2026 {
    type Offset = FixedOffset;
    fn from_offset(_: &Self::Offset) -> Self {
        Self
    }
    fn offset_from_local_date(&self, value: &NaiveDate) -> LocalResult<Self::Offset> {
        self.offset_from_local_datetime(&value.and_hms_opt(12, 0, 0).unwrap())
    }
    fn offset_from_local_datetime(&self, value: &NaiveDateTime) -> LocalResult<Self::Offset> {
        if *value >= wall("2026-03-08 02:00:00") && *value < wall("2026-03-08 03:00:00") {
            LocalResult::None
        } else if *value >= wall("2026-11-01 01:00:00") && *value < wall("2026-11-01 02:00:00") {
            // Intentionally return later offset first; policy must choose by instant.
            LocalResult::Ambiguous(offset(-5), offset(-4))
        } else {
            LocalResult::Single(
                if *value >= wall("2026-03-08 03:00:00") && *value < wall("2026-11-01 01:00:00") {
                    offset(-4)
                } else {
                    offset(-5)
                },
            )
        }
    }
    fn offset_from_utc_date(&self, value: &NaiveDate) -> Self::Offset {
        self.offset_from_utc_datetime(&value.and_hms_opt(12, 0, 0).unwrap())
    }
    fn offset_from_utc_datetime(&self, value: &NaiveDateTime) -> Self::Offset {
        if *value >= wall("2026-03-08 07:00:00") && *value < wall("2026-11-01 06:00:00") {
            offset(-4)
        } else {
            offset(-5)
        }
    }
}
fn local_ms(value: &str) -> i64 {
    Eastern2026
        .from_local_datetime(&wall(value))
        .single()
        .unwrap()
        .timestamp_millis()
}

#[test]
fn five_calendar_days_across_dst_are_not_fixed_seconds() {
    for (start, expected, hours) in [
        ("2026-03-04 12:00:00", "2026-03-09 12:00:00", 119),
        ("2026-10-28 12:00:00", "2026-11-02 12:00:00", 121),
    ] {
        let start = local_ms(start);
        let end = time::five_days(&Eastern2026, start).unwrap();
        assert_eq!(end - start, hours * 3_600_000);
        assert_eq!(end, local_ms(expected));
    }
}

#[test]
fn target_gap_shifts_forward_and_preserves_subseconds() {
    let start = local_ms("2026-03-03 02:30:00.123");
    assert_eq!(
        time::five_days(&Eastern2026, start).unwrap(),
        local_ms("2026-03-08 03:30:00.123")
    );
}

#[test]
fn target_fold_chooses_earlier_occurrence() {
    let start = local_ms("2026-10-27 01:30:00.456");
    assert_eq!(
        time::five_days(&Eastern2026, start).unwrap(),
        wall("2026-11-01 05:30:00.456").and_utc().timestamp_millis()
    );
}

#[test]
fn source_and_plan_are_never_mutated() {
    let raw = channel();
    let plan = plan(1_790_000_000);
    let before = (raw.clone(), plan.clone());
    let _ = standard(&raw, Some(&plan)).unwrap().at_millis(0).unwrap();
    assert_eq!((raw, plan), before);
}
