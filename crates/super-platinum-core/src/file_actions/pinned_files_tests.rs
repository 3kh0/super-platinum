use super::*;
use crate::pins::PinnedItem;
use serde_json::json;

fn file() -> File {
    serde_json::from_value(
        json!({"id":"F1","user":"U1","title":"Known","name":"known.txt",
        "size":12,"pretty_type":"Text","mimetype":"text/plain","filetype":"text",
        "url_private":"https://files.slack.com/original","thumb_64":"https://files.slack.com/thumb",
        "permalink":"https://team.slack.com/files/U1/F1","shares":{"private":{"C1":[{"ts":"1"}]}},
        "opaque":{"keep":true}}),
    )
    .unwrap()
}
fn core() -> CoreAppState {
    let mut core = CoreAppState::new(Default::default());
    core.pins.ensure_scope(0, "T1");
    core.pins.channel = Some("C1".into());
    for (key, item) in [
        (
            "file:F1",
            PinnedItem {
                kind: "file".into(),
                file: Some(file()),
                ..Default::default()
            },
        ),
        (
            "comment:FC1",
            PinnedItem {
                kind: "file_comment".into(),
                file: Some(file()),
                comment: Some(json!({"id":"FC1","comment":"Keep comment"})),
                ..Default::default()
            },
        ),
        (
            "message:1.0",
            PinnedItem {
                kind: "message".into(),
                // Both fields are optional, not a tagged union: visit both if present.
                file: Some(file()),
                message: Some(Message {
                    ts: Some("1.0".into()),
                    text: Some("Keep message".into()),
                    files: vec![file()],
                    attachments: vec![crate::slack::models::Attachment {
                        text: Some("Keep attachment".into()),
                        files: vec![file()],
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            },
        ),
        (
            "file:F2",
            PinnedItem {
                file: Some(File {
                    id: Some("F2".into()),
                    ..file()
                }),
                ..Default::default()
            },
        ),
        (
            "missing-id",
            PinnedItem {
                file: Some(File { id: None, ..file() }),
                ..Default::default()
            },
        ),
    ] {
        core.pins.items.insert(key.into(), item);
    }
    core
}
fn copies(core: &CoreAppState) -> Vec<&File> {
    let message = core.pins.items["message:1.0"].message.as_ref().unwrap();
    vec![
        core.pins.items["file:F1"].file.as_ref().unwrap(),
        core.pins.items["comment:FC1"].file.as_ref().unwrap(),
        core.pins.items["message:1.0"].file.as_ref().unwrap(),
        &message.files[0],
        &message.attachments[0].files[0],
    ]
}
fn assert_controls(core: &CoreAppState) {
    for (key, id) in [("file:F2", Some("F2")), ("missing-id", None)] {
        let expected = File {
            id: id.map(str::to_owned),
            ..file()
        };
        assert_eq!(
            serde_json::to_value(core.pins.items[key].file.as_ref().unwrap()).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }
    assert_eq!(core.pins.items.len(), 5);
    assert_eq!(core.pins.items["file:F1"].key().as_deref(), Some("file:F1"));
    assert_eq!(
        core.pins.items["comment:FC1"].key().as_deref(),
        Some("comment:FC1")
    );
    assert_eq!(
        core.pins.items["comment:FC1"].comment.as_ref().unwrap()["comment"],
        "Keep comment"
    );
    let message = core.pins.items["message:1.0"].message.as_ref().unwrap();
    assert_eq!(message.text.as_deref(), Some("Keep message"));
    assert_eq!(
        message.attachments[0].text.as_deref(),
        Some("Keep attachment")
    );
}

#[test]
fn pin_display_fields_reach_standalone_comment_and_message_copies_only() {
    let mut core = core();
    let patch: File = serde_json::from_value(json!({"id":"F1","title":"  Renamed  ","name":"renamed.txt","size":0,"pretty_type":"Plain text","url_private":"https://files.slack.com/other","mimetype":"video/mp4","filetype":"list","shares":{}})).unwrap();
    assert!(core.merge_file_display_metadata("T1", &patch));
    let mut expected = file();
    expected.title = Some("Renamed".into());
    expected.name = Some("renamed.txt".into());
    expected.size = Some(0);
    expected.pretty_type = Some("Plain text".into());
    for raw in [
        json!({"id":"F1"}),
        json!({"id":"F1","title":" ","name":"\t","size":null,"pretty_type":" "}),
    ] {
        assert!(!core.merge_file_display_metadata("T1", &serde_json::from_value(raw).unwrap()));
    }
    for actual in copies(&core) {
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
    }
    assert_controls(&core);
}

#[test]
fn pin_rename_and_deletion_preserve_rows_and_remove_all_file_actions() {
    let mut core = core();
    core.rename_file("T1", "F1", "Local rename", Some("local.txt"));
    let expected = File {
        title: Some("Local rename".into()),
        name: Some("local.txt".into()),
        ..file()
    };
    for actual in copies(&core) {
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
    }
    core.remove_file("T1", "F1");
    core.remove_file("T1", "F1");
    // With no workspace ledger in this fixture, the per-copy tombstone must suffice.
    core.rename_file("T1", "F1", "Do not revive", Some("revived.txt"));
    core.merge_file_display_metadata("T1", &file());
    for actual in copies(&core) {
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(tombstone("F1")).unwrap()
        );
        assert_eq!(crate::state::file_title(actual), "Deleted file");
        assert!(deleted(actual));
        assert!(identity(actual).is_none());
        assert!(!can_delete(actual, "U1"));
        assert!(actual.full_url().is_none());
        assert!(actual.preview().is_none());
    }
    assert_controls(&core);
}

#[test]
fn pin_copies_are_isolated_by_current_team_and_account_epoch() {
    for (epoch, team) in [(0, "T2"), (1, "T1")] {
        let mut core = core();
        core.account_epoch = epoch;
        core.rename_file(team, "F1", "Foreign", Some("foreign.txt"));
        core.merge_file_display_metadata(
            team,
            &File {
                size: Some(0),
                ..file()
            },
        );
        core.remove_file(team, "F1");
        for actual in copies(&core) {
            assert_eq!(
                serde_json::to_value(actual).unwrap(),
                serde_json::to_value(file()).unwrap()
            );
        }
        assert_controls(&core);
    }
}
