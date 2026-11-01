use super::*;

#[test]
fn known_native_deletion_retires_activity_body_and_late_reads_without_hiding_other_coordinates() {
    let mut c = core();
    let extra: Vec<crate::slack::models::ActivityItem> = serde_json::from_value(json!([
        {"key":"thread-peer","feed_ts":"3.0","item":{"type":"thread_v2","bundle_info":{"payload":{"thread_entry":{"channel_id":"C1","thread_ts":"1.0","latest_ts":"2.0"}}}}},
        {"key":"other-channel","feed_ts":"4.0","item":{"type":"at_user","message":{"channel":"C2","ts":"1.0"}}}
    ])).unwrap();
    c.activity.items.extend(extra);
    c.activity.selected = Some("mention".into());
    let metadata = (
        serde_json::to_value(&c.activity.items).unwrap(),
        c.activity.selected.clone(),
        c.activity.next_cursor.clone(),
    );
    c.activity
        .hydrated
        .insert(("C1".into(), "1.0".into()), old());
    c.delete_activity_message("T1", "C1", "1.0");
    assert!(
        c.activity
            .hydrated
            .contains_key(&("C1".into(), "1.0".into()))
    );
    let request = c
        .activity
        .begin_message_hydration(&[
            ("C1".into(), vec!["1.0".into(), "2.0".into()]),
            ("C2".into(), vec!["1.0".into()]),
        ])
        .unwrap();
    let files = c.begin_file_display_read("T1");
    edit(
        &mut c,
        json!({"ts":"1.0","text":"Edited before deletion","blocks":[],"attachments":[]}),
    );
    assert!(!c.activity.sync.messages.updates.is_empty());
    c.workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .entry("C1".into())
        .or_default()
        .record_remote_deletion("1.0");
    c.delete_activity_message("T2", "C1", "1.0");
    assert!(
        c.activity
            .hydrated
            .contains_key(&("C1".into(), "1.0".into()))
    );
    c.delete_activity_message("T1", "C1", "1.0");
    assert!(
        !c.activity
            .hydrated
            .contains_key(&("C1".into(), "1.0".into()))
    );
    assert!(c.activity.sync.messages.updates.is_empty());
    assert!(c.activity_item_message_deleted(&c.activity.items[0]));
    assert!(!c.activity_item_message_deleted(&c.activity.items[1]));
    assert!(!c.activity_item_message_deleted(&c.activity.items[2]));
    edit(
        &mut c,
        json!({"ts":"1.0","text":"Late edited body","blocks":[],"attachments":[]}),
    );
    assert!(c.activity.sync.messages.updates.is_empty());
    let result = serde_json::from_value(json!({"messages_data":{
        "C1":{"messages":[old(),{"ts":"2.0","text":"Independent thread preview"}]},
        "C2":{"messages":[{"ts":"1.0","text":"Same timestamp, other channel"}]}
    }}))
    .unwrap();
    assert_eq!(
        c.apply_activity_message_page("T1", request, result, files),
        Ok(true)
    );
    assert!(
        !c.activity
            .hydrated
            .contains_key(&("C1".into(), "1.0".into()))
    );
    assert_eq!(
        c.activity.hydrated[&("C1".into(), "2.0".into())]
            .text
            .as_deref(),
        Some("Independent thread preview")
    );
    assert_eq!(
        c.activity.hydrated[&("C2".into(), "1.0".into())]
            .text
            .as_deref(),
        Some("Same timestamp, other channel")
    );
    let (request, files) = read(&mut c);
    assert_eq!(
        c.apply_activity_message_page("T1", request, page(old()), files),
        Ok(true)
    );
    assert!(
        !c.activity
            .hydrated
            .contains_key(&("C1".into(), "1.0".into()))
    );
    assert_eq!(
        (
            serde_json::to_value(&c.activity.items).unwrap(),
            c.activity.selected.clone(),
            c.activity.next_cursor.clone()
        ),
        metadata
    );
    c.account_epoch += 1;
    assert!(!c.activity_message_was_deleted("C1", "1.0"));
}
