use super::*;
use crate::{
    file_actions::{deleted, identity},
    slack::models::Attachment,
};
use serde_json::json;

fn file() -> File {
    serde_json::from_value(json!({"id":"F1","title":"Snapshot","name":"snapshot.txt","size":12,"pretty_type":"Text","mimetype":"text/plain","filetype":"text","url_private":"https://files.slack.com/snapshot","thumb_64":"https://files.slack.com/thumb","shares":{"private":{"C1":[{"ts":"1"}]}},"opaque":{"keep":true}})).unwrap()
}
fn message() -> Message {
    Message {
        ts: Some("1.0".into()),
        text: Some("Snapshot body".into()),
        files: vec![file()],
        attachments: vec![Attachment {
            text: Some("Snapshot attachment".into()),
            files: vec![file()],
            ..Default::default()
        }],
        ..Default::default()
    }
}
fn page() -> PinsPage {
    PinsPage {
        items: vec![
            PinnedItem {
                kind: "file".into(),
                file: Some(file()),
                ..Default::default()
            },
            PinnedItem {
                kind: "file_comment".into(),
                file: Some(file()),
                comment: Some(json!({"id":"FC1","comment":"Keep comment"})),
                ..Default::default()
            },
            PinnedItem {
                kind: "message".into(),
                message: Some(message()),
                file: Some(file()),
                ..Default::default()
            },
            PinnedItem {
                file: Some(File {
                    id: Some("F2".into()),
                    ..file()
                }),
                ..Default::default()
            },
        ],
    }
}
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
    core.pins.ensure_scope(0, "T1");
    core
}
fn begin(core: &mut CoreAppState) -> (u64, FileDisplayRead) {
    (core.pins.begin("C1"), core.begin_file_display_read("T1"))
}
fn install(core: &mut CoreAppState) {
    let (generation, read) = begin(core);
    assert_eq!(
        core.apply_pin_page_since("T1", generation, page(), read),
        Ok(true)
    );
}
fn copies(core: &CoreAppState) -> Vec<&File> {
    let item = &core.pins.items["message:1.0"];
    let message = item.message.as_ref().unwrap();
    vec![
        core.pins.items["file:F1"].file.as_ref().unwrap(),
        core.pins.items["comment:FC1"].file.as_ref().unwrap(),
        item.file.as_ref().unwrap(),
        &message.files[0],
        &message.attachments[0].files[0],
    ]
}
fn event(item: PinnedItem, timestamp: &str) -> PinUpdate {
    PinUpdate {
        item,
        channel: "C1".into(),
        pinned: true,
        event_ts: timestamp.into(),
    }
}

#[test]
fn inflight_page_preserves_display_fields_across_new_copies_without_unwrapping() {
    let mut core = core();
    let (generation, read) = begin(&mut core);
    core.rename_file("T1", "F1", "Renamed", Some("renamed.txt"));
    core.merge_file_display_metadata(
        "T1",
        &File {
            id: Some("F1".into()),
            size: Some(0),
            pretty_type: Some("Plain text".into()),
            ..Default::default()
        },
    );
    let mut incoming = page();
    let msg = incoming.items[2].message.as_mut().unwrap();
    msg.subtype = Some("message_replied".into());
    msg.message = Some(Box::new(Message {
        text: Some("Keep nested shape".into()),
        ..Default::default()
    }));
    let nested = serde_json::to_value(&msg.message).unwrap();
    assert_eq!(
        core.apply_pin_page_since("T1", generation, incoming, read),
        Ok(true)
    );
    let expected = File {
        title: Some("Renamed".into()),
        name: Some("renamed.txt".into()),
        size: Some(0),
        pretty_type: Some("Plain text".into()),
        ..file()
    };
    for actual in copies(&core) {
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
    }
    let msg = core.pins.items["message:1.0"].message.as_ref().unwrap();
    assert_eq!(msg.text.as_deref(), Some("Snapshot body"));
    assert_eq!(msg.subtype.as_deref(), Some("message_replied"));
    assert_eq!(serde_json::to_value(&msg.message).unwrap(), nested);
    assert_eq!(
        core.pins.items["file:F2"]
            .file
            .as_ref()
            .unwrap()
            .title
            .as_deref(),
        Some("Snapshot")
    );
    assert_eq!(
        core.pins.items["comment:FC1"].comment.as_ref().unwrap()["comment"],
        "Keep comment"
    );
}

#[test]
fn pending_message_file_order_survives_page_merge_in_both_directions() {
    for later_message in [false, true] {
        let mut core = core();
        install(&mut core);
        let (generation, read) = begin(&mut core);
        let mut update = message();
        update.text = Some("Realtime body".into());
        visit_message(&mut update, &mut |f| {
            f.title = Some("Message title".into());
            f.size = Some(99);
            f.url_private = Some("https://files.slack.com/message".into());
        });
        if !later_message {
            core.update_pinned_message("T1", "C1", &update);
        }
        core.merge_file_display_metadata(
            "T1",
            &File {
                id: Some("F1".into()),
                title: Some("Display title".into()),
                size: Some(0),
                ..Default::default()
            },
        );
        if later_message {
            core.update_pinned_message("T1", "C1", &update);
        }
        core.update_pinned_message(
            "T1",
            "C1",
            &Message {
                ts: Some("1.0".into()),
                text: Some("Later text only".into()),
                ..Default::default()
            },
        );
        core.pins.apply_reaction("C1", "1.0", "U1", "eyes", true);
        assert_eq!(
            core.apply_pin_page_since("T1", generation, page(), read),
            Ok(true)
        );
        let actual = core.pins.items["message:1.0"].message.as_ref().unwrap();
        let mut expected = update;
        if !later_message {
            visit_message(&mut expected, &mut |f| {
                f.title = Some("Display title".into());
                f.size = Some(0);
            });
        }
        expected.text = Some("Later text only".into());
        crate::state::apply_message_reaction(&mut expected, "U1", "eyes", true);
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }
}

#[test]
fn later_list_starts_empty_and_partial_fields_do_not_renew_old_evidence() {
    let mut core = core();
    let (old_generation, old_read) = begin(&mut core);
    core.merge_file_display_metadata(
        "T1",
        &File {
            id: Some("F1".into()),
            size: Some(0),
            ..Default::default()
        },
    );
    let (generation, read) = begin(&mut core);
    for raw in [
        json!({"id":"F1","size":null}),
        json!({"id":"F1","title":" ","name":"\t","pretty_type":" "}),
    ] {
        assert!(!core.merge_file_display_metadata("T1", &serde_json::from_value(raw).unwrap()));
    }
    core.rename_file("T1", "F1", "Newest title", None);
    assert_eq!(
        core.apply_pin_page_since("T1", old_generation, page(), old_read),
        Ok(false)
    );
    assert_eq!(
        core.apply_pin_page_since("T1", generation, page(), read),
        Ok(true)
    );
    for file in copies(&core) {
        assert_eq!(file.size, Some(12));
        assert_eq!(file.title.as_deref(), Some("Newest title"));
    }
    install(&mut core);
    assert_eq!(
        core.pins.items["file:F1"]
            .file
            .as_ref()
            .unwrap()
            .title
            .as_deref(),
        Some("Snapshot")
    );
}

#[test]
fn list_masks_known_deletions_before_or_during_read_after_message_merges() {
    for before in [false, true] {
        let mut core = core();
        install(&mut core);
        if before {
            core.remove_file("T1", "F1");
        }
        let (generation, read) = begin(&mut core);
        if !before {
            core.remove_file("T1", "F1");
        }
        core.update_pinned_message("T1", "C1", &message());
        assert_eq!(
            core.apply_pin_page_since("T1", generation, page(), read),
            Ok(true)
        );
        for actual in copies(&core) {
            assert_eq!(
                serde_json::to_value(actual).unwrap(),
                serde_json::to_value(tombstone("F1")).unwrap()
            );
            assert!(identity(actual).is_none() && actual.full_url().is_none());
        }
        assert_eq!(core.pins.items.len(), 4);
        assert!(!deleted(core.pins.items["file:F2"].file.as_ref().unwrap()));
        assert_eq!(
            core.pins.items["message:1.0"]
                .message
                .as_ref()
                .unwrap()
                .text
                .as_deref(),
            Some("Snapshot body")
        );
    }
}

#[test]
fn pin_event_deletions_do_not_change_membership_order_or_future_display_authority() {
    let mut core = core();
    install(&mut core);
    core.remove_file("T1", "F1");
    for item in page().items.into_iter().take(3) {
        let update = event(item, "20.0");
        assert!(core.apply_pin_event("T1", &update));
        let generation = core.pins.generation;
        for ts in ["19.0", "20.0"] {
            assert!(!core.apply_pin_event("T1", &event(update.item.clone(), ts)));
        }
        assert_eq!(core.pins.generation, generation);
    }
    for actual in copies(&core) {
        assert!(deleted(actual));
    }
    core.rename_file("T1", "F2", "Earlier rename", None);
    let update = event(
        PinnedItem {
            file: Some(File {
                id: Some("F2".into()),
                title: Some("Later snapshot".into()),
                ..file()
            }),
            ..Default::default()
        },
        "30.0",
    );
    assert!(core.apply_pin_event("T1", &update));
    assert_eq!(
        core.pins.items["file:F2"]
            .file
            .as_ref()
            .unwrap()
            .title
            .as_deref(),
        Some("Later snapshot")
    );
    let mut removed = update;
    removed.pinned = false;
    removed.event_ts = "31.0".into();
    assert!(core.apply_pin_event("T1", &removed));
    assert!(!core.pins.items.contains_key("file:F2"));
}

#[test]
fn local_ack_and_message_changed_mask_known_deletions_in_visible_and_pending_copies() {
    let mut core = core();
    install(&mut core);
    let captured = page().items.remove(2);
    core.remove_file("T1", "F1");
    core.set_pin_item("T1", "C1", captured, true);
    let (generation, read) = begin(&mut core);
    let mut changed = message();
    changed.text = Some("New text survives".into());
    core.update_pinned_message("T1", "C1", &changed);
    let visible = core.pins.items["message:1.0"].message.as_ref().unwrap();
    assert!(deleted(&visible.files[0]) && deleted(&visible.attachments[0].files[0]));
    let pending = core.pins.message_updates["1.0"].as_ref().unwrap();
    assert!(deleted(&pending.files[0]) && deleted(&pending.attachments[0].files[0]));
    assert_eq!(
        core.apply_pin_page_since("T1", generation, page(), read),
        Ok(true)
    );
    assert_eq!(
        core.pins.items["message:1.0"]
            .message
            .as_ref()
            .unwrap()
            .text
            .as_deref(),
        Some("New text survives")
    );
    for actual in copies(&core) {
        assert!(deleted(actual));
    }
}

#[test]
fn pressure_error_does_not_partially_install_page_or_complete_loading() {
    let mut core = core();
    install(&mut core);
    let held = (0..32)
        .map(|_| core.begin_file_display_read("T1"))
        .collect::<Vec<_>>();
    let (generation, refused) = begin(&mut core);
    assert!(
        core.apply_pin_page_since("T1", generation, PinsPage::default(), refused)
            .is_err()
    );
    assert_eq!(core.pins.items.len(), 4);
    assert!(core.pins.loading && core.pins.loaded);
    assert_eq!(core.pins.generation, generation);
    drop(held);
    let read = core.begin_file_display_read("T1");
    assert_eq!(
        core.apply_pin_page_since("T1", generation, page(), read),
        Ok(true)
    );
}

#[test]
fn foreign_scope_and_replaced_generation_leave_newer_pin_state_unchanged() {
    for foreign_epoch in [false, true] {
        let mut core = core();
        install(&mut core);
        let (generation, read) = begin(&mut core);
        let team = if foreign_epoch {
            core.account_epoch = 1;
            "T1"
        } else {
            "T2"
        };
        assert_eq!(
            core.apply_pin_page_since(team, generation, PinsPage::default(), read),
            Ok(false)
        );
        assert!(!core.apply_pin_event(team, &event(page().items.remove(0), "20.0")));
        core.set_pin_item(team, "C1", page().items.remove(0), false);
        core.update_pinned_message(
            team,
            "C1",
            &Message {
                ts: Some("1.0".into()),
                text: Some("Wrong scope".into()),
                ..Default::default()
            },
        );
        assert_eq!(core.pins.items.len(), 4);
        assert_eq!(core.pins.generation, generation);
        assert_eq!(
            core.pins.items["message:1.0"]
                .message
                .as_ref()
                .unwrap()
                .text
                .as_deref(),
            Some("Snapshot body")
        );
    }
    let mut core = core();
    let (generation, read) = begin(&mut core);
    let update = event(page().items.remove(0), "20.0");
    assert!(core.apply_pin_event("T1", &update));
    assert_eq!(
        core.apply_pin_page_since("T1", generation, PinsPage::default(), read),
        Ok(false)
    );
    assert_eq!(core.pins.items.len(), 1);
}
