use super::super::*;
use super::common::msg;

#[test]
fn ts_key_orders_numerically_not_lexically() {
    assert!(ts_key("1783372360.000009") < ts_key("1783372360.000010"));
    assert!(ts_key("999.1") < ts_key("1000.0"));
}

#[test]
fn ordinal_day_suffixes_match_slack_date_labels() {
    assert_eq!(ordinal_day(1), "1st");
    assert_eq!(ordinal_day(2), "2nd");
    assert_eq!(ordinal_day(3), "3rd");
    assert_eq!(ordinal_day(4), "4th");
    assert_eq!(ordinal_day(11), "11th");
    assert_eq!(ordinal_day(22), "22nd");
}
#[test]
fn relative_timestamp_uses_compact_hours() {
    let ts = format!("{}.000000", now_secs() - 12 * 60 * 60);
    assert_eq!(format_relative_ts(&ts), "12h ago");
}

#[test]
fn scroll_ratio_for_ts_at_start_is_zero() {
    let messages = vec![msg("1.0", "a"), msg("2.0", "b"), msg("3.0", "c")];
    assert_eq!(scroll_ratio_for_ts(&messages, "1.0"), Some(0.0));
}

#[test]
fn scroll_ratio_for_ts_at_end_is_one() {
    let messages = vec![msg("1.0", "a"), msg("2.0", "b"), msg("3.0", "c")];
    assert_eq!(scroll_ratio_for_ts(&messages, "3.0"), Some(1.0));
}

#[test]
fn scroll_ratio_for_ts_middle_is_between() {
    let messages = vec![msg("1.0", "a"), msg("2.0", "b"), msg("3.0", "c")];
    assert_eq!(scroll_ratio_for_ts(&messages, "2.0"), Some(0.5));
}

#[test]
fn scroll_ratio_for_ts_missing_is_none() {
    let messages = vec![msg("1.0", "a"), msg("2.0", "b")];
    assert_eq!(scroll_ratio_for_ts(&messages, "9.0"), None);
}

#[test]
fn scroll_ratio_for_ts_single_message_is_zero() {
    let messages = vec![msg("1.0", "a")];
    assert_eq!(scroll_ratio_for_ts(&messages, "1.0"), Some(0.0));
}

#[test]
fn scroll_ratio_for_ts_empty_is_none() {
    let messages: Vec<SlackMessage> = Vec::new();
    assert_eq!(scroll_ratio_for_ts(&messages, "1.0"), None);
}
