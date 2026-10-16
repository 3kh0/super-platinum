use super::*;
use crate::slack::models::{Attachment, DmEntry, ThreadViewItem};
use serde_json::json;
fn file() -> File {
    serde_json::from_value(json!({"id":"F1","title":"Known","name":"known.txt",
        "size":12,"pretty_type":"Text","mimetype":"text/plain","filetype":"text",
        "url_private":"https://files.slack.com/old","thumb_64":"https://files.slack.com/thumb",
        "is_external":false,"shares":{"private":{"C1":[{"ts":"1"}]}},
        "opaque":{"keep":[1,2]},"url_private_download":"https://files.slack.com/original"}))
    .unwrap()
}
fn message() -> Message {
    Message {
        ts: Some("1.0".into()),
        text: Some("unchanged message".into()),
        files: vec![file()],
        attachments: vec![Attachment {
            files: vec![file()],
            ..Default::default()
        }],
        ..Default::default()
    }
}
fn core() -> CoreAppState {
    let mut c = CoreAppState::new(Default::default());
    c.active_team = Some("T1".into());
    for team in ["T1", "T2"] {
        let mut w = crate::state::Workspace::from_session(&crate::config::WorkspaceSession {
            team_id: team.into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: team.into(),
            url: "https://example.invalid".into(),
            token: String::new(),
        });
        let bag = crate::state::ChannelMessages {
            messages: vec![message()],
            ..Default::default()
        };
        w.messages.insert("C1".into(), bag.clone());
        c.workspaces.insert(team.into(), w);
        c.threads
            .insert((team.into(), "C1".into(), "1.0".into()), bag);
    }
    c.files.ensure_scope(0, "T1");
    c.channel_files.ensure_scope(0, "T1");
    c.files.items = vec![file()];
    c.channel_files.items = vec![file()];
    c.files.page = 1;
    c.channel_files.page = 1;
    c.search = Some(crate::domain::SearchState {
        query: "q".into(),
        team: "T1".into(),
        page: 1,
        page_count: 1,
        total: 1,
        loading: false,
        hits: vec![crate::domain::SearchHit {
            channel: "C1".into(),
            channel_label: "channel".into(),
            message: message(),
        }],
    });
    c.pins.ensure_scope(0, "T1");
    c.pins.items.insert(
        "m".into(),
        crate::pins::PinnedItem {
            message: Some(message()),
            ..Default::default()
        },
    );
    c.threads_view.ensure_scope(0, "T1");
    c.threads_view.items = vec![ThreadViewItem {
        root_msg: message(),
        latest_replies: vec![message()],
        unread_replies: vec![message()],
        ..Default::default()
    }];
    c.activity.ensure_scope(0, "T1");
    c.activity
        .hydrated
        .insert(("C1".into(), "1.0".into()), message());
    c.dms.entries.push(DmEntry {
        id: "D1".into(),
        message: Some(message()),
        ..Default::default()
    });
    c.later.ensure_scope(0, "T1");
    c.later.files.insert("F1".into(), file());
    let key = crate::later::SavedKey::message("C1", "1.0");
    c.later.hydrated.insert(key, message());
    c.later.update_message("C1", &message());
    c
}
#[test]
fn display_patch_reaches_existing_visitor_copies_and_invalidates_inflight_indexes() {
    let mut c = core();
    let old = c.files.begin(Default::default(), 1);
    let old_channel = c.channel_files.begin(Default::default(), 1);
    let mut changed = file();
    changed.title = None;
    changed.name = None;
    changed.size = Some(0);
    changed.pretty_type = Some("Plain text".into());
    changed.mimetype = Some("video/mp4".into());
    changed.filetype = Some("list".into());
    changed.url_private = Some("https://files.slack.com/new".into());
    changed.extra.clear();
    changed.is_external = Some(true);
    assert!(c.merge_file_display_metadata("T1", &changed));
    let mut expected = file();
    expected.size = Some(0);
    expected.pretty_type = Some("Plain text".into());
    assert_eq!(crate::state::file_summary(&expected), "Plain text - 0 B");
    let expected = serde_json::to_value(expected).unwrap();
    let mut count = 0;
    c.visit_files("T1", |f| {
        if f.id.as_deref() == Some("F1") {
            assert_eq!(serde_json::to_value(f).unwrap(), expected);
            count += 1;
        }
    });
    assert_eq!(count, 25); // 11 message/attachment pairs plus two indexes and standalone Later.
    assert_eq!(
        serde_json::to_value(&c.workspaces["T2"].messages["C1"].messages[0].files[0]).unwrap(),
        serde_json::to_value(file()).unwrap()
    );
    assert!(!c.files.apply(old, Default::default()));
    assert!(!c.channel_files.apply(old_channel, Default::default()));
    assert!(!c.files.loading && !c.channel_files.loading);
}
#[test]
fn empty_omitted_null_and_unsafe_only_fields_do_not_clear_or_invalidate() {
    let mut c = core();
    let generation = c.files.begin(Default::default(), 1);
    for raw in [
        json!({"id":"F1"}),
        json!({"id":"F1","size":null,"pretty_type":null}),
        json!({"id":"F1","pretty_type":""}),
        json!({"id":"F1","pretty_type":"  "}),
        json!({"id":"F1","mimetype":"video/mp4","filetype":"list","is_external":true,"shares":{}}),
    ] {
        assert!(!c.merge_file_display_metadata("T1", &serde_json::from_value(raw).unwrap()));
    }
    assert_eq!(c.files.generation, generation);
    assert!(c.files.loading);
    assert_eq!(
        serde_json::to_value(&c.files.items[0]).unwrap(),
        serde_json::to_value(file()).unwrap()
    );
    assert!(serde_json::from_value::<File>(json!({"id":"F1","size":-1})).is_err());
    assert!(serde_json::from_value::<File>(json!({"id":"F1","pretty_type":false})).is_err());
}
#[test]
fn tombstones_and_foreign_scope_remain_unchanged() {
    let mut c = core();
    let mut patch = file();
    patch.size = Some(900);
    c.remove_file("T1", "F1");
    assert!(!c.merge_file_display_metadata("T1", &patch));
    assert!(deleted(
        &c.workspaces["T1"].messages["C1"].messages[0].files[0]
    ));
    c.later.ensure_scope(1, "T1");
    c.later.files.insert("F1".into(), file());
    assert!(c.merge_file_display_metadata("T2", &patch));
    assert_eq!(c.later.files["F1"].size, Some(12));
    patch.extra.insert("mode".into(), json!("tombstone"));
    assert!(!c.merge_file_display_metadata("T2", &patch));
}

#[test]
fn existing_title_normalization_and_download_name_behavior_is_preserved() {
    let mut c = core();
    let mut update = file();
    update.title = Some("  Trimmed title  ".into());
    update.name = None;
    assert!(c.merge_file_display_metadata("T1", &update));
    assert_eq!(c.files.items[0].title.as_deref(), Some("Trimmed title"));
    assert_eq!(c.files.items[0].name.as_deref(), Some("known.txt"));
}

#[test]
fn whitespace_names_do_not_override_copies_when_independent_fields_are_present() {
    let mut c = core();
    let patch = File {
        id: Some("F1".into()),
        title: Some("  ".into()),
        name: Some("\t".into()),
        size: Some(0),
        pretty_type: Some("Label".into()),
        ..Default::default()
    };
    assert!(c.merge_file_display_metadata("T1", &patch));
    assert_eq!(c.files.items[0].title.as_deref(), Some("Known"));
    assert_eq!(c.files.items[0].name.as_deref(), Some("known.txt"));
    assert_eq!(c.files.items[0].size, Some(0));
    let valid_name = File {
        name: Some("new.txt".into()),
        ..patch
    };
    assert!(c.merge_file_display_metadata("T1", &valid_name));
    assert_eq!(c.files.items[0].title.as_deref(), Some("new.txt"));
}
