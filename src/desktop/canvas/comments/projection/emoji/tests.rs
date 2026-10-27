use super::*;
use crate::canvas::comments::{Owner, Query, Row};
use crate::media::MediaRegistry;
use crate::model::RichNode;
use serde_json::json;
use super_platinum_core::composer_emoji::standard::Version;
use super_platinum_core::slack::models::Message;
use super_platinum_core::slack_lists::comments::edit_seed::prepare::StandardEmoji;

const URL: &str = "https://fixture.invalid/canvas-comment-emoji.png";

fn message(root: bool) -> Message {
    serde_json::from_value(json!({
        "type":"message", "subtype":if root {Some("document_comment_root")} else {None},
        "ts":if root {"1.0"} else {"1.1"}, "thread_ts":"1.0", "user":"U1",
        "document_comment":{"thread_id":"original-control","opaque":{"keep":true}},
        "text":"superseded fallback",
        "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
            {"type":"emoji","name":"conflicting","unicode":"1f44d-1f3fd","display_url":URL},
            {"type":"emoji","name":"conflicting","unicode":"0041","display_url":URL},
            {"type":"emoji","name":"conflicting","unicode":"1F44D","display_url":URL}
        ]}]}]
    }))
    .unwrap()
}

fn children(body: &crate::model::MessageVm) -> &[RichNode] {
    let [RichNode::Paragraph(children)] = body.body.as_slice() else {
        panic!("one original rich section")
    };
    children
}

fn row(shell: &ShellState) -> Row {
    let preview = message(false);
    let message = message(true);
    Row {
        body: super::super::row_vm(&shell.core.workspaces["T1"], &message, &shell.media, None),
        message,
        preview: Some(preview.clone()),
        preview_body: Some(crate::message_vm::message_vm_with_standard(
            &shell.core.workspaces["T1"],
            &preview,
            &shell.media,
            false,
            None,
        )),
    }
}

#[test]
fn selected_map_reprojects_root_and_retained_preview_then_restores_unobserved_glyphs() {
    let shell = ShellState::fixture(MediaRegistry::default());
    let mut rows = vec![row(&shell)];
    let original = serde_json::to_value(&rows[0].message).unwrap();
    let preview = serde_json::to_value(&rows[0].preview).unwrap();
    let standard = StandardEmoji::build(Version::V17).unwrap();
    reproject_rows(
        &shell.core.workspaces["T1"],
        &shell.media,
        &mut rows,
        Some(&standard),
    );
    for body in [&rows[0].body, rows[0].preview_body.as_ref().unwrap()] {
        assert!(
            matches!(&children(body)[0], RichNode::EmojiImage {name,..} if name == "+1::skin-tone-4")
        );
        assert!(matches!(&children(body)[1], RichNode::Emoji {glyph,..} if glyph == "A"));
        assert_eq!(
            children(body).len(),
            2,
            "invalid Unicode has no image or fallback"
        );
    }
    reproject_rows(&shell.core.workspaces["T1"], &shell.media, &mut rows, None);
    for body in [&rows[0].body, rows[0].preview_body.as_ref().unwrap()] {
        assert!(
            matches!(&children(body)[0], RichNode::Emoji {glyph,..} if glyph == "\u{1f44d}\u{1f3fd}")
        );
        assert!(
            !children(body)
                .iter()
                .any(|v| matches!(v, RichNode::EmojiImage { .. }))
        );
    }
    assert_eq!(serde_json::to_value(&rows[0].message).unwrap(), original);
    assert_eq!(serde_json::to_value(&rows[0].preview).unwrap(), preview);
    assert_eq!(rows[0].body.ts, "1.0");
    assert_eq!(rows[0].preview_body.as_ref().unwrap().ts, "1.1");
}

#[test]
fn selected_file_root_defers_body_and_hidden_root_never_registers_body_media() {
    let core = super::super::tests::body_core();
    let standard = StandardEmoji::build(Version::V17).unwrap();
    let mut raw = serde_json::to_value(message(true)).unwrap();
    raw["files"] = json!([{"id":"F12345678","name":"first canonical file"}]);
    let message: Message = serde_json::from_value(raw.clone()).unwrap();
    let media = MediaRegistry::default();
    let cached = super::super::row_vm(&core.workspaces["T1"], &message, &media, Some(&standard));
    assert!(cached.body.is_empty());
    assert_eq!(media.registered_source_count(URL), 0);
    let fallback = super::super::message_vm_with_standard(
        &core.workspaces["T1"],
        &message,
        &media,
        Some(&standard),
    );
    assert!(matches!(
        &children(&fallback)[0],
        RichNode::EmojiImage { .. }
    ));
    assert_eq!(media.registered_source_count(URL), 1);
    for files in [json!([]), raw["files"].clone()] {
        raw["files"] = files;
        raw["document_comment"]["is_visible"] = json!(false);
        let hidden: Message = serde_json::from_value(raw.clone()).unwrap();
        let media = MediaRegistry::default();
        let body = super::super::message_vm_with_standard(
            &core.workspaces["T1"],
            &hidden,
            &media,
            Some(&standard),
        );
        assert_eq!(
            body.body,
            vec![RichNode::Text("This content was deleted.".into())]
        );
        assert_eq!(media.registered_source_count(URL), 0);
        assert!(!media.has_pending());
    }
}

fn model() -> (ShellState, State) {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Fixture".into(),
        permalink: None,
    });
    shell.overlay = Some(crate::state::Overlay::Canvas);
    let model = State {
        owner: Some(Owner::capture(&shell).unwrap()),
        query: Some(Query::Replies("1.0".into())),
        serial: 42,
        cursor: Some("original-cursor".into()),
        rows: vec![row(&shell)],
        ..Default::default()
    };
    (shell, model)
}

#[test]
fn unobserved_repeated_refresh_preserves_allocations_and_reader_state() {
    let (shell, mut model) = model();
    let body = model.rows[0].body.body.as_ptr();
    let preview = model.rows[0].preview_body.as_ref().unwrap().body.as_ptr();
    let raw = serde_json::to_value(&model.rows[0].message).unwrap();
    for _ in 0..3 {
        assert!(!refresh(&shell, &mut model));
    }
    assert_eq!(body, model.rows[0].body.body.as_ptr());
    assert_eq!(
        preview,
        model.rows[0].preview_body.as_ref().unwrap().body.as_ptr()
    );
    assert_eq!(model.serial, 42);
    assert_eq!(model.query, Some(Query::Replies("1.0".into())));
    assert_eq!(model.cursor.as_deref(), Some("original-cursor"));
    assert_eq!(serde_json::to_value(&model.rows[0].message).unwrap(), raw);
}

#[test]
fn retired_canvas_owner_and_closed_query_cannot_refresh_cached_rows() {
    let (mut shell, mut model) = model();
    shell.shortcut_generation = shell.shortcut_generation.wrapping_add(1);
    assert!(current_owner(&shell, &model).is_none());
    assert!(!refresh(&shell, &mut model));
    model.owner = Owner::capture(&shell);
    assert!(current_owner(&shell, &model).is_some());
    model.query = None;
    assert!(current_owner(&shell, &model).is_none());
    assert!(!refresh(&shell, &mut model));
}

#[test]
fn selected_map_does_not_restore_text_superseded_by_unsupported_blocks() {
    let core = super::super::tests::body_core();
    let mut raw = serde_json::to_value(message(true)).unwrap();
    raw["blocks"] = json!([{"type":"future_unsupported"}]);
    let message: Message = serde_json::from_value(raw).unwrap();
    let standard = StandardEmoji::build(Version::V17).unwrap();
    let media = MediaRegistry::default();
    let body = super::super::row_vm(&core.workspaces["T1"], &message, &media, Some(&standard));
    assert!(body.body.is_empty());
    assert!(!media.has_pending());
}
