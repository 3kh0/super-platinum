use super::*;

fn message() -> Message {
    Message::new(
        &json!({"shortcut_url":"https://slack.com/shortcuts/FtLOCAL"}),
        "Review this workflow",
        "CDEST0001",
        "38126951-e78d-4b54-b3e4-5dc77d41f906",
        "1781524900.000001",
    )
    .unwrap()
}
fn bag(message: &Message) -> ChannelMessages {
    let mut bag = ChannelMessages::default();
    bag.upsert(run::optimistic(message, "UOWNER01"));
    bag.pending.push(message.local_ts().into());
    bag
}
#[test]
fn original_requires_pending_and_complete_rich_payload() {
    let message = message();
    let original = bag(&message);
    assert!(pending_original(&original, &message, "UOWNER01"));
    let mut changed = original.clone();
    changed.pending.clear();
    assert!(!pending_original(&changed, &message, "UOWNER01"));
    let mut changed = original.clone();
    changed.messages[0].blocks.clear();
    assert!(!pending_original(&changed, &message, "UOWNER01"));
    assert!(!pending_original(&original, &message, "UOTHER01"));
}
#[test]
fn replacement_with_same_timestamp_is_never_removed() {
    let message = message();
    let mut bag = bag(&message);
    bag.messages[0].client_msg_id = Some("different-client".into());
    let before = serde_json::to_value(&bag.messages).unwrap();
    remove_copy(&mut bag, &message, "UOWNER01");
    assert_eq!(serde_json::to_value(&bag.messages).unwrap(), before);
    assert_eq!(bag.pending, vec![message.local_ts().to_owned()]);
}
#[test]
fn local_removal_preserves_unrelated_rows_and_allows_later_server_evidence() {
    let message = message();
    let mut bag = bag(&message);
    let mut other = run::optimistic(&message, "UOWNER01");
    other.ts = Some("1781524901.000002".into());
    other.client_msg_id = Some("other-client".into());
    bag.upsert(other.clone());
    bag.pending.push(other.ts.clone().unwrap());
    remove_copy(&mut bag, &message, "UOWNER01");
    assert_eq!(bag.messages.len(), 1);
    assert_eq!(bag.messages[0].ts, other.ts);
    assert_eq!(bag.pending, vec![other.ts.unwrap()]);
    assert!(!bag.was_deleted(message.local_ts()));
    remove_copy(&mut bag, &message, "UOWNER01");
    assert_eq!(bag.messages.len(), 1);
    bag.upsert(run::optimistic(&message, "UOWNER01"));
    assert_eq!(bag.messages.len(), 2);
    assert!(!pending_original(&bag, &message, "UOWNER01"));
}
#[test]
fn ambiguous_duplicate_timestamp_is_refused() {
    let message = message();
    let mut bag = bag(&message);
    bag.messages.push(run::optimistic(&message, "UOWNER01"));
    assert!(!pending_original(&bag, &message, "UOWNER01"));
    remove_copy(&mut bag, &message, "UOWNER01");
    assert_eq!(bag.messages.len(), 2);
}
