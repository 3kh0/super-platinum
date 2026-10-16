use super::*;
use serde_json::json;

fn core() -> CoreAppState {
    let mut core = CoreAppState::new(Default::default());
    for team in ["T1", "T2"] {
        core.workspaces.insert(
            team.into(),
            crate::state::Workspace::from_session(&crate::config::WorkspaceSession {
                team_id: team.into(),
                enterprise_id: None,
                user_id: "U1".into(),
                name: team.into(),
                url: "https://example.invalid".into(),
                token: String::new(),
            }),
        );
    }
    core
}
fn file() -> File {
    serde_json::from_value(json!({"id":"F1","title":"History title","name":"history.txt","size":100,"pretty_type":"History type","mimetype":"text/plain","filetype":"text","url_private":"https://files.slack.com/history","thumb_64":"https://files.slack.com/thumb","shares":{"private":{"C1":[{"ts":"1"}]}},"opaque":{"keep":true}})).unwrap()
}
fn message() -> Message {
    Message {
        ts: Some("1.0".into()),
        text: Some("New authoritative body".into()),
        files: vec![file()],
        attachments: vec![crate::slack::models::Attachment {
            files: vec![file()],
            ..Default::default()
        }],
        ..Default::default()
    }
}
fn patch(raw: serde_json::Value) -> File {
    serde_json::from_value(raw).unwrap()
}

#[test]
fn in_flight_history_retains_only_accepted_display_fields_and_new_body_media() {
    let mut core = core();
    let read = core.begin_file_display_read("T1");
    assert!(core.merge_file_display_metadata("T1", &patch(json!({"id":"F1","title":"Event title","name":"event.txt","size":0,"pretty_type":"Event type","mimetype":"video/mp4","url_private":"https://files.slack.com/event"}))));
    let overlay = core.finish_file_display_read("T1", read).unwrap();
    let incoming = message();
    let mut expected = incoming.clone();
    visit_message(&mut expected, &mut |f| {
        f.title = Some("Event title".into());
        f.name = Some("event.txt".into());
        f.size = Some(0);
        f.pretty_type = Some("Event type".into());
    });
    let merged = overlay.message(incoming);
    assert_eq!(
        serde_json::to_value(&merged).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    let mut bag = crate::state::ChannelMessages::default();
    bag.upsert(Message {
        ts: Some("1.0".into()),
        text: Some("Old body".into()),
        files: vec![file()],
        ..Default::default()
    });
    bag.upsert(merged);
    assert_eq!(
        bag.messages[0].text.as_deref(),
        Some("New authoritative body")
    );
    assert_eq!(bag.messages[0].files[0].size, Some(0));
    assert_eq!(
        bag.messages[0].files[0].url_private.as_deref(),
        Some("https://files.slack.com/history")
    );
}

#[test]
fn each_request_keeps_only_fields_accepted_after_its_capture() {
    let mut core = core();
    let older = core.begin_file_display_read("T1");
    core.merge_file_display_metadata(
        "T1",
        &patch(json!({"id":"F1","size":0,"pretty_type":"First type"})),
    );
    let later = core.begin_file_display_read("T1");
    for raw in [
        json!({"id":"F1","title":" ","name":"\t","pretty_type":" "}),
        json!({"id":"F1","size":null,"pretty_type":null}),
    ] {
        assert!(!core.merge_file_display_metadata("T1", &patch(raw)));
    }
    core.merge_file_display_metadata("T1", &patch(json!({"id":"F1","title":"New title"})));
    let old = core
        .finish_file_display_read("T1", older)
        .unwrap()
        .message(message());
    let new = core
        .finish_file_display_read("T1", later)
        .unwrap()
        .message(message());
    assert_eq!(old.files[0].size, Some(0));
    assert_eq!(old.files[0].pretty_type.as_deref(), Some("First type"));
    assert_eq!(new.files[0].size, Some(100));
    assert_eq!(new.files[0].pretty_type.as_deref(), Some("History type"));
    for m in [old, new] {
        assert_eq!(m.files[0].title.as_deref(), Some("New title"));
        assert_eq!(m.files[0].name.as_deref(), Some("history.txt"));
    }
}

#[test]
fn accepted_rename_and_successive_partial_updates_compose_without_erasing_fields() {
    let mut core = core();
    let read = core.begin_file_display_read("T1");
    core.rename_file("T1", "F1", "Renamed", Some("renamed.txt"));
    core.merge_file_display_metadata(
        "T1",
        &patch(json!({"id":"F1","size":5,"pretty_type":"First"})),
    );
    core.merge_file_display_metadata("T1", &patch(json!({"id":"F1","size":6})));
    let m = core
        .finish_file_display_read("T1", read)
        .unwrap()
        .message(message());
    assert_eq!(m.files[0].title.as_deref(), Some("Renamed"));
    assert_eq!(m.files[0].name.as_deref(), Some("renamed.txt"));
    assert_eq!(m.files[0].size, Some(6));
    assert_eq!(m.files[0].pretty_type.as_deref(), Some("First"));
}

#[test]
fn finished_or_canceled_requests_leave_no_authority_for_later_history() {
    let mut core = core();
    let canceled = core.begin_file_display_read("T1");
    core.merge_file_display_metadata("T1", &patch(json!({"id":"F1","size":0})));
    drop(canceled);
    let fresh = core.begin_file_display_read("T1");
    assert_eq!(core.file_display_reads.pending.len(), 1);
    let overlay = core.finish_file_display_read("T1", fresh).unwrap();
    assert!(overlay.0.is_empty());
    assert_eq!(overlay.message(message()).files[0].size, Some(100));
    assert!(
        core.file_display_reads
            .pending
            .iter()
            .all(|p| p.upgrade().is_none())
    );
}

#[test]
fn workspace_and_account_scope_never_adopt_foreign_display_evidence() {
    let mut core = core();
    let read = core.begin_file_display_read("T1");
    core.merge_file_display_metadata("T2", &patch(json!({"id":"F1","size":0})));
    assert_eq!(
        core.finish_file_display_read("T1", read)
            .unwrap()
            .message(message())
            .files[0]
            .size,
        Some(100)
    );
    let wrong_team = core.begin_file_display_read("T1");
    assert!(core.finish_file_display_read("T2", wrong_team).is_err());
    let old_account = core.begin_file_display_read("T1");
    core.account_epoch += 1;
    let new_account = core.begin_file_display_read("T1");
    assert_eq!(core.file_display_reads.pending.len(), 1);
    core.merge_file_display_metadata("T1", &patch(json!({"id":"F1","size":7})));
    assert!(core.finish_file_display_read("T1", old_account).is_err());
    assert_eq!(
        core.finish_file_display_read("T1", new_account)
            .unwrap()
            .message(message())
            .files[0]
            .size,
        Some(7)
    );
}

#[test]
fn visible_wrapper_attachments_and_tombstones_keep_existing_semantics() {
    let mut core = core();
    let read = core.begin_file_display_read("T1");
    core.merge_file_display_metadata("T1", &patch(json!({"id":"F1","size":0})));
    let overlay = core.finish_file_display_read("T1", read).unwrap();
    let nested = message();
    let wrapper = Message {
        subtype: Some("message_replied".into()),
        message: Some(Box::new(nested.clone())),
        ..Default::default()
    };
    let empty = FileDisplayOverlay(BTreeMap::new()).message(wrapper.clone());
    assert_eq!(
        serde_json::to_value(empty).unwrap(),
        serde_json::to_value(crate::state::visible_message(wrapper.clone())).unwrap()
    );
    assert_eq!(
        serde_json::to_value(overlay.message(wrapper)).unwrap(),
        serde_json::to_value(overlay.message(nested)).unwrap()
    );
    let tomb = crate::file_actions::tombstone("F1");
    let incoming = Message {
        files: vec![
            tomb.clone(),
            File::default(),
            File {
                id: Some("F2".into()),
                ..file()
            },
        ],
        ..Default::default()
    };
    let expected = serde_json::to_value(&incoming).unwrap();
    assert_eq!(
        serde_json::to_value(overlay.message(incoming)).unwrap(),
        expected
    );
}

#[test]
fn reader_budget_refuses_without_retaining_unbounded_slots_and_recovers_after_drop() {
    let mut core = core();
    let reads = (0..MAX_READS)
        .map(|_| core.begin_file_display_read("T1"))
        .collect::<Vec<_>>();
    let refused = core.begin_file_display_read("T1");
    assert_eq!(core.file_display_reads.pending.len(), MAX_READS);
    assert!(core.finish_file_display_read("T1", refused).is_err());
    drop(reads);
    let next = core.begin_file_display_read("T1");
    assert_eq!(core.file_display_reads.pending.len(), 1);
    assert!(core.finish_file_display_read("T1", next).is_ok());
}

#[test]
fn field_storage_limits_refuse_whole_response_instead_of_evicting_changes() {
    for bytes in [false, true] {
        let mut core = core();
        let read = core.begin_file_display_read("T1");
        if bytes {
            core.rename_file("T1", "F1", &"x".repeat(MAX_BYTES), None);
        } else {
            for n in 0..=MAX_FILES {
                core.merge_file_display_metadata(
                    "T1",
                    &patch(json!({"id":format!("F{n}"),"size":0})),
                );
            }
        }
        let pending = read.0.pending.lock().unwrap();
        assert!(pending.failed);
        assert!(pending.updates.is_empty());
        assert_eq!(pending.bytes, 0);
        drop(pending);
        assert!(core.finish_file_display_read("T1", read).is_err());
    }
}

#[test]
fn poisoned_request_refuses_response_after_an_accepted_display_change() {
    let mut core = core();
    let read = core.begin_file_display_read("T1");
    let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _pending = read.0.pending.lock().unwrap();
        panic!("deliberate test-only display read poison");
    }));
    assert!(poisoned.is_err());
    assert!(read.0.pending.is_poisoned());
    // An accepted patch cannot repair missing evidence in a poisoned read.
    assert!(core.merge_file_display_metadata("T1", &patch(json!({"id":"F1","size":0}))));
    assert!(matches!(
        core.finish_file_display_read("T1", read),
        Err(RETIRED)
    ));
    // The failed read leaves no permanent authority or poisoned global state.
    let fresh = core.begin_file_display_read("T1");
    assert_eq!(
        core.finish_file_display_read("T1", fresh)
            .unwrap()
            .message(message())
            .files[0]
            .size,
        Some(100)
    );
}
