use super::*;
use serde_json::json;
use super_platinum_core::{
    file_actions::deleted,
    pins::{PinnedItem, PinsPage},
    slack::models::{Attachment, File, Message},
};

fn file() -> File {
    File {
        id: Some("FPIN1".into()),
        title: Some("Stale title".into()),
        filetype: Some("list".into()),
        url_private: Some("https://files.slack.com/stale".into()),
        ..Default::default()
    }
}
fn message() -> Message {
    Message {
        ts: Some("991.0".into()),
        text: Some("Retained note".into()),
        files: vec![file()],
        attachments: vec![Attachment {
            files: vec![file()],
            ..Default::default()
        }],
        ..Default::default()
    }
}
fn pin_event(timestamp: &str) -> RtEvent {
    RtEvent::Unknown(serde_json::from_value(json!({"type":"pin_added","channel_id":"C2","event_ts":timestamp,"item":{"type":"file","file":file()}})).unwrap())
}

#[test]
fn pin_added_uses_current_socket_and_membership_versions_before_masking_deleted_files() {
    let mut core = crate::fixture::fixture_core();
    core.pins.ensure_scope(core.account_epoch, "T1");
    core.pins.begin("C2");
    core.remove_file("T1", "FPIN1");
    let generation = core.workspaces["T1"].rt_generation;
    apply(
        &mut core,
        "T1",
        generation.wrapping_add(1),
        pin_event("20.0"),
    );
    assert!(core.pins.items.is_empty());
    core.active_team = Some("T2".into());
    apply(&mut core, "T1", generation, pin_event("20.0"));
    assert!(core.pins.items.is_empty());
    core.active_team = Some("T1".into());
    apply(&mut core, "T1", generation, pin_event("20.0"));
    let file = core.pins.items["file:FPIN1"].file.as_ref().unwrap();
    assert!(deleted(file) && file.filetype.is_none() && file.full_url().is_none());
    let pins_generation = core.pins.generation;
    apply(&mut core, "T1", generation, pin_event("19.0"));
    assert_eq!(core.pins.generation, pins_generation);
    assert_eq!(core.pins.version("C2", "file:FPIN1"), Some("20.0"));
}

#[test]
fn message_changed_cannot_reintroduce_deleted_pin_files_before_pending_list_arrives() {
    let mut core = crate::fixture::fixture_core();
    core.pins.ensure_scope(core.account_epoch, "T1");
    let generation = core.pins.begin("C2");
    core.pins.apply(
        generation,
        PinsPage {
            items: vec![PinnedItem {
                message: Some(message()),
                ..Default::default()
            }],
        },
    );
    let pending = core.pins.begin("C2");
    let read = core.begin_file_display_read("T1");
    core.remove_file("T1", "FPIN1");
    let socket = core.workspaces["T1"].rt_generation;
    let changed = Message {
        text: Some("Fresh message text".into()),
        ..message()
    };
    apply(
        &mut core,
        "T1",
        socket,
        RtEvent::MessageChanged {
            original_update: None,
            channel: "C2".into(),
            message: changed,
        },
    );
    let current = core.pins.items["message:991.0"].message.as_ref().unwrap();
    assert_eq!(current.text.as_deref(), Some("Fresh message text"));
    assert!(deleted(&current.files[0]) && deleted(&current.attachments[0].files[0]));
    assert_eq!(
        core.apply_pin_page_since(
            "T1",
            pending,
            PinsPage {
                items: vec![PinnedItem {
                    message: Some(message()),
                    ..Default::default()
                }]
            },
            read
        ),
        Ok(true)
    );
    let current = core.pins.items["message:991.0"].message.as_ref().unwrap();
    assert_eq!(current.text.as_deref(), Some("Fresh message text"));
    assert!(deleted(&current.files[0]) && deleted(&current.attachments[0].files[0]));
}
