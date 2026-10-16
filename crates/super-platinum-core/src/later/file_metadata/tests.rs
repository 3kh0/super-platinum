use super::*;
use serde_json::json;
fn file(size: u64, kind: Option<&str>) -> File {
    File {
        id: Some("F1".into()),
        title: Some("server title".into()),
        name: Some("server.txt".into()),
        size: Some(size),
        pretty_type: kind.map(Into::into),
        extra: [("opaque".into(), json!([7]))].into(),
        ..Default::default()
    }
}
fn message(file: File) -> Message {
    Message {
        ts: Some("1.0".into()),
        files: vec![file.clone()],
        attachments: vec![crate::slack::models::Attachment {
            files: vec![file],
            ..Default::default()
        }],
        ..Default::default()
    }
}
#[test]
fn each_display_field_overlays_only_requests_older_than_its_own_revision() {
    let mut s = LaterState::default();
    s.ensure_scope(1, "T1");
    let key = SavedKey::message("C1", "1.0");
    let old = s.begin_hydration();
    s.record_file_display_metadata(
        "F1",
        DisplayPatch {
            size: Some(0),
            pretty_type: Some("first"),
            ..Default::default()
        },
    );
    let between = s.begin_hydration();
    s.record_file_display_metadata(
        "F1",
        DisplayPatch {
            pretty_type: Some("second"),
            ..Default::default()
        },
    );
    s.hydrate_message_since(key.clone(), message(file(99, Some("old"))), old);
    assert_eq!(s.hydrated[&key].files[0].size, Some(0));
    assert_eq!(
        s.hydrated[&key].files[0].pretty_type.as_deref(),
        Some("second")
    );
    assert!(s.hydrate_file_since("F1", file(99, Some("old")), old));
    assert_eq!(s.files["F1"].size, Some(0));
    s.hydrate_message_since(key.clone(), message(file(200, Some("server"))), between);
    for f in [
        &s.hydrated[&key].files[0],
        &s.hydrated[&key].attachments[0].files[0],
    ] {
        assert_eq!(f.size, Some(200));
        assert_eq!(f.pretty_type.as_deref(), Some("second"));
        assert_eq!(f.title.as_deref(), Some("server title"));
        assert_eq!(f.extra["opaque"], json!([7]));
    }
    assert!(s.hydrate_file_since("F1", file(200, Some("server")), between));
    assert_eq!(s.files["F1"].size, Some(200));
    let fresh = s.begin_hydration();
    s.hydrate_message_since(key.clone(), message(file(300, None)), fresh);
    assert!(s.hydrate_file_since("F1", file(300, None), fresh));
    assert_eq!(s.files["F1"].size, Some(300));
    assert!(s.files["F1"].pretty_type.is_none());
    assert!(s.hydrated[&key].files[0].pretty_type.is_none());
    assert!(!s.hydrate_file_since("F1", file(99, Some("late")), old));
}
#[test]
fn rename_and_display_updates_do_not_renew_each_others_absent_fields() {
    let mut s = LaterState::default();
    let old = s.begin_hydration();
    s.record_file_rename("F1", "renamed", Some("renamed.txt"));
    let after_name = s.begin_hydration();
    s.record_file_display_metadata(
        "F1",
        DisplayPatch {
            size: Some(4),
            ..Default::default()
        },
    );
    assert!(s.hydrate_file_since("F1", file(99, None), old));
    assert_eq!(s.files["F1"].title.as_deref(), Some("renamed"));
    assert!(s.hydrate_file_since("F1", file(100, None), after_name));
    assert_eq!(s.files["F1"].title.as_deref(), Some("server title"));
    assert_eq!(s.files["F1"].size, Some(4));
    let after_size = s.begin_hydration();
    s.record_file_rename("F1", "last", None);
    assert!(s.hydrate_file_since("F1", file(500, None), after_size));
    assert_eq!(s.files["F1"].size, Some(500));
    assert_eq!(s.files["F1"].name.as_deref(), Some("server.txt"));
}
#[test]
fn deletion_and_scope_reset_drop_display_overlay_and_pending_message_uses_updated_fields() {
    let mut s = LaterState::default();
    s.ensure_scope(1, "T1");
    let key = SavedKey::message("C1", "1.0");
    s.hydrated
        .insert(key.clone(), message(file(10, Some("old"))));
    let old = s.begin_hydration();
    s.update_message("C1", &message(file(10, Some("old"))));
    assert!(s.preview_updates.contains_key(&key));
    s.visit_files(&mut |f| {
        f.size = Some(0);
        f.pretty_type = Some("patched".into());
    });
    s.record_file_display_metadata(
        "F1",
        DisplayPatch {
            size: Some(0),
            pretty_type: Some("patched"),
            ..Default::default()
        },
    );
    let pending = s.preview_updates[&key].message.as_ref().unwrap();
    for f in [&pending.files[0], &pending.attachments[0].files[0]] {
        assert_eq!(f.size, Some(0));
        assert_eq!(f.pretty_type.as_deref(), Some("patched"));
    }
    s.hydrate_message_since(key.clone(), message(file(99, None)), old);
    for f in [
        &s.hydrated[&key].files[0],
        &s.hydrated[&key].attachments[0].files[0],
    ] {
        assert_eq!(f.size, Some(0));
        assert_eq!(f.pretty_type.as_deref(), Some("patched"));
    }
    s.record_file_deletion("F1");
    s.record_file_display_metadata(
        "F1",
        DisplayPatch {
            size: Some(55),
            ..Default::default()
        },
    );
    assert!(s.file_updates.get("F1").is_none());
    assert!(s.hydrate_file_since("F1", file(99, None), old));
    assert!(crate::file_actions::deleted(&s.files["F1"]));
    s.ensure_scope(2, "T1");
    let fresh = s.begin_hydration();
    assert!(s.hydrate_file_since("F1", file(66, Some("fresh")), fresh));
    assert_eq!(s.files["F1"].size, Some(66));
    assert!(!crate::file_actions::deleted(&s.files["F1"]));
}
