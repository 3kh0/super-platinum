use super::*;
#[test]
fn newer_partial_frames_do_not_replay_older_fields_or_reverse_request_order() {
    let mut state = LaterState::default();
    let key = SavedKey::message("C1", "1.0");
    let message = |text: &str| Message {
        ts: Some("1.0".into()),
        text: Some(text.into()),
        blocks: vec![serde_json::json!({"type":"section","text":text})],
        ..Default::default()
    };
    state.hydrate_message(key.clone(), message("Original"));
    state.update_message("C1", &message("Old event"));
    let older = state.begin_hydration();
    let newer = state.begin_hydration();
    state.update_message(
        "C1",
        &Message {
            ts: Some("1.0".into()),
            reply_count: Some(3),
            ..Default::default()
        },
    );
    state.hydrate_message_since(key.clone(), message("Fresh history"), newer);
    assert_eq!(state.hydrated[&key].text.as_deref(), Some("Fresh history"));
    assert_eq!(state.hydrated[&key].blocks, message("Fresh history").blocks);
    assert_eq!(state.hydrated[&key].reply_count, Some(3));
    state.hydrate_message_since(key.clone(), message("Late older history"), older);
    assert_eq!(state.hydrated[&key].text.as_deref(), Some("Fresh history"));
}
#[test]
fn file_changes_cover_first_hydration_and_expire_for_fresh_requests() {
    let mut state = LaterState::default();
    state.ensure_scope(1, "T1");
    let key = SavedKey::message("C1", "1.2");
    let message = || Message {
        ts: Some("1.2".into()),
        text: Some("Keep this".into()),
        files: vec![File {
            id: Some("F1".into()),
            title: Some("Original".into()),
            name: Some("original.txt".into()),
            ..Default::default()
        }],
        ..Default::default()
    };
    let old = state.hydration_revision();
    state.record_file_rename("F1", "First", Some("first.txt"));
    let after_first = state.hydration_revision();
    state.record_file_rename("F1", "Second", None);
    state.hydrate_message_since(key.clone(), message(), old);
    assert_eq!(
        state.hydrated[&key].files[0].title.as_deref(),
        Some("Second")
    );
    assert_eq!(
        state.hydrated[&key].files[0].name.as_deref(),
        Some("first.txt")
    );
    state.hydrate_message_since(key.clone(), message(), after_first);
    assert_eq!(
        state.hydrated[&key].files[0].title.as_deref(),
        Some("Second")
    );
    assert_eq!(
        state.hydrated[&key].files[0].name.as_deref(),
        Some("original.txt")
    );
    let fresh = state.hydration_revision();
    state.hydrate_message_since(key.clone(), message(), fresh);
    assert_eq!(
        state.hydrated[&key].files[0].title.as_deref(),
        Some("Original")
    );
    state.record_file_deletion("F1");
    state.hydrate_message_since(key.clone(), message(), fresh);
    assert!(crate::file_actions::deleted(&state.hydrated[&key].files[0]));
    state.ensure_scope(2, "T1");
    state.hydrate_message(key.clone(), message());
    assert!(!crate::file_actions::deleted(
        &state.hydrated[&key].files[0]
    ));
}
#[test]
fn late_hydration_cannot_restore_edited_or_deleted_saved_messages() {
    let mut state = LaterState::default();
    let generation = state.begin(LaterFilter::Saved, false);
    assert!(state.apply_page(generation, page(), false, None));
    let key = SavedKey::message("C_PRIVATE", "1.2");
    let original = Message {
        ts: Some("1.2".into()),
        user: Some("U1".into()),
        text: Some("old".into()),
        ..Default::default()
    };
    let edit = Message {
        ts: original.ts.clone(),
        text: Some("edited while history was loading".into()),
        ..Default::default()
    };
    state.update_message("C_PRIVATE", &edit);
    state.hydrate_message(key.clone(), original.clone());
    assert_eq!(state.hydrated[&key].text, edit.text);
    assert_eq!(state.hydrated[&key].user, original.user);
    state.delete_message("C_PRIVATE", "1.2");
    state.update_message("C_PRIVATE", &edit);
    state.hydrate_message(key.clone(), original);
    assert_eq!(
        state.hydrated[&key].text.as_deref(),
        Some("This message was deleted.")
    );
    assert!(state.hydrated[&key].blocks.is_empty());
}
#[test]
fn edited_and_deleted_messages_refresh_saved_previews_without_losing_identity() {
    let key = SavedKey::message("C1", "1.2");
    let mut state = LaterState::default();
    state.hydrated.insert(
        key.clone(),
        Message {
            ts: Some("1.2".into()),
            user: Some("U1".into()),
            text: Some("old".into()),
            blocks: vec![
                serde_json::json!({"type":"section","text":{"type":"plain_text","text":"old"}}),
            ],
            ..Default::default()
        },
    );
    state.update_message(
        "C1",
        &Message {
            ts: Some("1.2".into()),
            text: Some("new".into()),
            blocks: vec![
                serde_json::json!({"type":"section","text":{"type":"plain_text","text":"new"}}),
            ],
            ..Default::default()
        },
    );
    assert_eq!(state.hydrated[&key].user.as_deref(), Some("U1"));
    assert_eq!(state.hydrated[&key].text.as_deref(), Some("new"));
    state.delete_message("C1", "1.2");
    assert!(state.hydrated[&key].blocks.is_empty());
    assert_eq!(
        state.hydrated[&key].text.as_deref(),
        Some("This message was deleted.")
    );
    assert_eq!(state.hydrated[&key].ts.as_deref(), Some("1.2"));
}
#[test]
fn saved_events_merge_counts_reject_replays_and_invalidate_older_pages() {
    let mut state = LaterState::default();
    let generation = state.begin(LaterFilter::Saved, false);
    let event = |kind: &str, timestamp: &str, completed: bool| {
        serde_json::from_value::<crate::slack::events::RawEvent>(serde_json::json!({"type":kind,"event_ts":timestamp,"saved":{"item_type":"message","item_id":"C_PRIVATE","ts":"1.2","state":if completed {"completed"}else{"in_progress"}},"counts":{"uncompleted_count":if completed {0}else{1},"completed_count":if completed {1}else{0}}})).unwrap()
    };
    assert!(state.apply_event(&event("saved_added", "10.1", false)));
    assert_eq!(state.visible.len(), 1);
    assert!(!state.apply_page(generation, page(), false, None));
    assert!(state.apply_event(&event("saved_updated", "11.1", true)));
    assert!(state.visible.is_empty());
    assert_eq!(state.counts.completed_count, 1);
    assert!(!state.apply_event(&event("saved_added", "10.1", false)));
    assert!(state.apply_event(&event("saved_deleted", "12.1", true)));
    assert!(state.items.is_empty());
}
#[test]
fn reminders_keep_rich_description_without_requiring_a_message_timestamp() {
    let description = serde_json::json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Review 🚀\nnotes","style":{"bold":true}},{"type":"user","user_id":"U1"}]}]}]);
    let item: SavedItem=serde_json::from_value(serde_json::json!({"item_id":"Sa1","item_type":"reminder","description":description,"date_due":123,"state":"in_progress"})).unwrap();
    assert_eq!(item.key().ts, "");
    assert_eq!(
        serde_json::to_value(item.description_message().blocks).unwrap(),
        description
    );
    assert_eq!(
        serde_json::to_value(item).unwrap()["description"],
        description
    );
}
fn page() -> SavedPage {
    serde_json::from_value(serde_json::json!({"saved_items":[{"item_type":"message","item_id":"C_PRIVATE","ts":"1.2","state":"in_progress","todo_state":"saved"}],"counts":{"uncompleted_count":1},"response_metadata":{"next_cursor":"next"}})).unwrap()
}
#[test]
fn stale_pages_and_workspace_changes_cannot_restore_other_accounts_items() {
    let mut state = LaterState::default();
    state.ensure_scope(1, "T1");
    let old = state.begin(LaterFilter::Saved, false);
    state.ensure_scope(2, "T1");
    assert!(!state.apply_page(old, page(), false, None));
    assert!(state.visible.is_empty());
    let current = state.begin(LaterFilter::Saved, false);
    assert!(state.apply_page(current, page(), false, None));
    state.ensure_scope(2, "T2");
    assert!(state.items.is_empty());
}
#[test]
fn pagination_deduplicates_and_stops_repeated_cursors() {
    let mut state = LaterState::default();
    let generation = state.begin(LaterFilter::Saved, false);
    state.apply_page(generation, page(), false, None);
    let generation = state.begin(LaterFilter::Saved, true);
    state.apply_page(generation, page(), true, Some("next"));
    assert_eq!(state.visible.len(), 1);
    assert!(state.next_cursor.is_none());
}
#[test]
fn completed_and_archived_items_leave_the_active_list_and_invalidate_fetches() {
    let mut state = LaterState::default();
    let generation = state.begin(LaterFilter::Saved, false);
    state.apply_page(generation, page(), false, None);
    let key = state.visible[0].clone();
    let mut item = state.items[&key].clone();
    item.date_completed = 123;
    state.updated(&key, Some(item));
    assert!(state.visible.is_empty());
    assert!(!state.apply_page(generation, page(), false, None));
}

#[test]
fn todo_state_only_completion_projects_in_completed_and_archive_still_takes_precedence() {
    let item: SavedItem = serde_json::from_value(serde_json::json!({
        "item_type": "file", "item_id": "F1", "todo_state": "completed"
    }))
    .unwrap();
    assert_eq!(item.filter(), LaterFilter::Completed);
    let mut later = LaterState::default();
    later.updated(&item.key(), Some(item.clone()));
    assert!(
        later.visible.is_empty(),
        "completed receipt must leave In progress"
    );
    later.begin(LaterFilter::Completed, false);
    later.updated(&item.key(), Some(item.clone()));
    assert_eq!(later.visible, vec![item.key()]);
    let mut archived = item;
    archived.is_archived = true;
    assert_eq!(archived.filter(), LaterFilter::Archived);
}
#[test]
fn saved_overdue_authority_distinguishes_missing_null_and_zero() {
    for raw in [
        serde_json::json!({}),
        serde_json::json!({"uncompleted_overdue_count":null}),
    ] {
        let counts: super::SavedCounts = serde_json::from_value(raw).unwrap();
        assert_eq!(counts.uncompleted_overdue_count, None);
    }
    let counts: super::SavedCounts =
        serde_json::from_value(serde_json::json!({"uncompleted_overdue_count":0})).unwrap();
    assert_eq!(counts.uncompleted_overdue_count, Some(0));
}
