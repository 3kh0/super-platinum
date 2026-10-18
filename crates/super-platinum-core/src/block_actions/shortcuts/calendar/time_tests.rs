use super::*;
use chrono::FixedOffset;
#[derive(Clone)]
struct Eastern2026;
fn wall(s: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
}
fn offset(hours: i32) -> FixedOffset {
    FixedOffset::east_opt(hours * 3600).unwrap()
}
impl TimeZone for Eastern2026 {
    type Offset = FixedOffset;
    fn from_offset(_: &Self::Offset) -> Self {
        Self
    }
    fn offset_from_local_date(&self, d: &NaiveDate) -> LocalResult<Self::Offset> {
        self.offset_from_local_datetime(&d.and_hms_opt(12, 0, 0).unwrap())
    }
    fn offset_from_local_datetime(&self, d: &NaiveDateTime) -> LocalResult<Self::Offset> {
        if *d >= wall("2026-03-08 02:00") && *d < wall("2026-03-08 03:00") {
            return LocalResult::None;
        }
        if *d >= wall("2026-11-01 01:00") && *d < wall("2026-11-01 02:00") {
            return LocalResult::Ambiguous(offset(-5), offset(-4));
        }
        LocalResult::Single(
            if *d >= wall("2026-03-08 03:00") && *d < wall("2026-11-01 01:00") {
                offset(-4)
            } else {
                offset(-5)
            },
        )
    }
    fn offset_from_utc_date(&self, d: &NaiveDate) -> Self::Offset {
        self.offset_from_utc_datetime(&d.and_hms_opt(12, 0, 0).unwrap())
    }
    fn offset_from_utc_datetime(&self, d: &NaiveDateTime) -> Self::Offset {
        if *d >= wall("2026-03-08 07:00") && *d < wall("2026-11-01 06:00") {
            offset(-4)
        } else {
            offset(-5)
        }
    }
}
#[test]
fn captured_js_local_time_policy_handles_gap_and_fold_without_fixed_current_offset() {
    assert_eq!(
        resolve(&Eastern2026, wall("2026-03-08 02:30")),
        Some(wall("2026-03-08 07:30").and_utc().timestamp())
    );
    assert_eq!(
        resolve(&Eastern2026, wall("2026-11-01 01:30")),
        Some(wall("2026-11-01 05:30").and_utc().timestamp())
    );
    assert_eq!(
        resolve(&Eastern2026, wall("2026-02-01 09:15")),
        Some(wall("2026-02-01 14:15").and_utc().timestamp())
    );
    assert_eq!(
        resolve(&Eastern2026, wall("2026-07-01 09:15")),
        Some(wall("2026-07-01 13:15").and_utc().timestamp())
    );
}
#[test]
fn local_wall_input_is_strict_and_quarter_hour_choices_stay_bounded() {
    for d in ["2026-02-30", "2026-2-01", "", "1969-12-31"] {
        assert!(day(d).is_none());
    }
    for t in ["25:00", "9:00", "09:00:00", "10:60", ""] {
        assert!(parse("2026-10-18", t).is_none());
    }
    let start = parse("2026-10-18", "00:00").unwrap();
    let slots = custom_slots("2026-10-18", 30, start - 86400);
    assert!(!slots.is_empty());
    assert!(slots.len() <= 104);
    assert_eq!(slots[0], (start, start + 1800));
    assert!(slots.windows(2).all(|p| p[1].0 - p[0].0 == 900));
    let n = parse("2026-10-18", "09:16").unwrap() + 23;
    let today_slots = custom_slots("2026-10-18", 30, n);
    assert_eq!(today_slots[0].0, n + 14 * 60);
}
