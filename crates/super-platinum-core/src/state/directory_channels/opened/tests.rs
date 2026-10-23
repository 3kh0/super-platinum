use super::*;
use serde_json::json;

#[test]
fn actual_latest_object_and_last_read_change_only_companions() {
    let mut messages = HashMap::new();
    let mut bag = ChannelMessages::default();
    bag.unread_count = 17;
    bag.mention_count = 3;
    bag.vip_count = 2;
    bag.read_revision = 8;
    messages.insert("DOPEN001".into(), bag);
    let raw = json!({"id":"DOPEN001","latest":{"ts":"42.5","text":"not a history response"},"last_read":"41.2"});
    Companions::capture(&raw).unwrap().apply(&mut messages);
    let b = &messages["DOPEN001"];
    assert_eq!(b.latest.as_deref(), Some("42.5"));
    assert_eq!(b.last_read.as_deref(), Some("41.2"));
    assert_eq!(
        (
            b.unread_count,
            b.mention_count,
            b.vip_count,
            b.read_revision
        ),
        (17, 3, 2, 9)
    );
    assert!(b.messages.is_empty());
    Companions::capture(&raw).unwrap().apply(&mut messages);
    assert_eq!(messages["DOPEN001"].read_revision, 9);
}
#[test]
fn absent_and_falsy_companions_do_not_create_or_clear_bag() {
    let mut messages = HashMap::new();
    for raw in [
        json!({"id":"DOPEN001"}),
        json!({"id":"DOPEN001","latest":false,"last_read":0}),
        json!({"id":"DOPEN001","latest":{},"last_read":""}),
    ] {
        Companions::capture(&raw).unwrap().apply(&mut messages);
        assert!(messages.is_empty());
    }
}
#[test]
fn string_latest_and_backward_read_cursor_follow_actual_source_assignment() {
    let mut messages = HashMap::new();
    for value in ["42.5", "2.1"] {
        Companions::capture(&json!({"id":"DOPEN001","latest":value,"last_read":value}))
            .unwrap()
            .apply(&mut messages);
    }
    assert_eq!(messages["DOPEN001"].latest.as_deref(), Some("2.1"));
    assert_eq!(messages["DOPEN001"].last_read.as_deref(), Some("2.1"));
    assert_eq!(messages["DOPEN001"].read_revision, 2);
}
#[test]
fn malformed_truthy_native_timestamp_is_not_coerced() {
    for raw in [
        json!({"id":"DOPEN001","last_read":true}),
        json!({"id":"DOPEN001","last_read":42}),
        json!({"id":"DOPEN001","latest":{"ts":42}}),
        json!({"id":""}),
    ] {
        assert!(Companions::capture(&raw).is_err());
    }
}
