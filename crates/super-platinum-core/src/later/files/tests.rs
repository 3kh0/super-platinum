use super::*;
fn file(id: &str, title: &str) -> File {
    File {
        id: Some(id.into()),
        title: Some(title.into()),
        name: Some("download.txt".into()),
        ..Default::default()
    }
}
#[test]
fn standalone_file_hydration_orders_requests_and_preserves_later_rename_fields() {
    let mut state = LaterState::default();
    state.ensure_scope(1, "T1");
    let old = state.begin_hydration();
    state.record_file_rename("F1", "renamed", Some("new.txt"));
    assert!(state.hydrate_file_since("F1", file("F1", "old"), old));
    assert_eq!(state.files["F1"].title.as_deref(), Some("renamed"));
    assert_eq!(state.files["F1"].name.as_deref(), Some("new.txt"));
    let newer = state.begin_hydration();
    assert!(state.hydrate_file_since("F1", file("F1", "fresh server"), newer));
    assert!(!state.hydrate_file_since("F1", file("F1", "old response"), old));
    assert!(!state.hydrate_file_since("F1", file("F1", "duplicate result"), newer));
    let wrong = state.begin_hydration();
    assert!(!state.hydrate_file_since("F1", file("F2", "wrong id"), wrong));
    assert_eq!(state.files["F1"].title.as_deref(), Some("fresh server"));
}

#[test]
fn deletion_during_hydration_or_before_cache_entry_cannot_resurrect_file() {
    let mut state = LaterState::default();
    let request = state.begin_hydration();
    state.record_file_deletion("F1");
    assert!(state.hydrate_file_since("F1", file("F1", "late"), request));
    assert!(crate::file_actions::deleted(&state.files["F1"]));
    assert_eq!(state.file_errors["F1"], FileHydrationFailure::Deleted);
    let next = state.begin_hydration();
    assert!(state.hydrate_file_since("F1", file("F1", "still late"), next));
    assert!(crate::file_actions::deleted(&state.files["F1"]));
    let next = state.begin_hydration();
    assert!(state.file_unavailable_since("F2", next, true));
    assert!(crate::file_actions::deleted(&state.files["F2"]));
}

#[test]
fn unavailable_metadata_is_distinct_from_deleted_and_does_not_change_membership() {
    let mut state = LaterState::default();
    state.ensure_scope(1, "T1");
    let key = SavedKey::file("F1");
    state.updated(
        &key,
        Some(SavedItem {
            item_type: "file".into(),
            item_id: "F1".into(),
            ..Default::default()
        }),
    );
    let old = state.begin_hydration();
    let newer = state.begin_hydration();
    assert!(state.file_unavailable_since("F1", newer, false));
    assert!(!state.hydrate_file_since("F1", file("F1", "older success"), old));
    assert_eq!(state.file_errors["F1"], FileHydrationFailure::Unavailable);
    assert!(matches!(
        state.membership(&key),
        SavedMembership::Present(_)
    ));
    let next = state.begin_hydration();
    assert!(state.hydrate_file_since("F1", file("F1", "recovered"), next));
    assert!(!state.file_errors.contains_key("F1"));
    assert!(!state.file_unavailable_since("F1", newer, false));
    state.ensure_scope(2, "T1");
    assert!(state.files.is_empty() && state.file_errors.is_empty());
    assert!(matches!(state.membership(&key), SavedMembership::Unknown));
}

#[test]
fn shared_file_visitor_reaches_standalone_and_message_file_copies() {
    let mut state = LaterState::default();
    let request = state.begin_hydration();
    state.hydrate_file_since("F1", file("F1", "old"), request);
    state.hydrate_message(
        SavedKey::message("C1", "1.000001"),
        Message {
            files: vec![file("F1", "old")],
            ..Default::default()
        },
    );
    state.visit_files(&mut |file| {
        file.title = Some("all copies".into());
    });
    assert_eq!(state.files["F1"].title.as_deref(), Some("all copies"));
    assert_eq!(
        state.hydrated.values().next().unwrap().files[0]
            .title
            .as_deref(),
        Some("all copies")
    );
    state.record_file_deletion("F1");
    assert!(crate::file_actions::deleted(&state.files["F1"]));
}
