use super::*;
use super_platinum_core::{MediaAssetKind, slack::models::Message};
fn setup() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    let file = File {
        id: Some("F_DISPLAY".into()),
        title: Some("Known title".into()),
        name: Some("known.txt".into()),
        size: Some(10),
        pretty_type: Some("Text".into()),
        mimetype: Some("text/plain".into()),
        url_private: Some("https://files.slack.com/display".into()),
        ..Default::default()
    };
    let id = shell.media.register_deferred(
        MediaAssetKind::Attachment,
        file.url_private.as_deref().unwrap(),
        "text/plain",
    );
    shell.media.register_file_download(&id, &file);
    shell
        .media
        .insert(id.clone(), "text/plain", b"known bytes".to_vec());
    shell.viewer = Some(crate::state::ViewerVm {
        file: policy::identity(&file),
        id,
        name: "Known title".into(),
        download_name: "known.txt".into(),
        mime: "text/plain".into(),
    });
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .entry("C1".into())
        .or_default()
        .upsert(Message {
            ts: Some("990.0".into()),
            files: vec![file],
            ..Default::default()
        });
    shell
}
#[test]
fn partial_size_only_and_type_only_refresh_preserves_names_and_open_viewer_source_bytes() {
    let mut shell = setup();
    let viewer = shell.viewer.clone().unwrap();
    let bytes = shell.media.ready_bytes(&viewer.id).unwrap();
    let source = shell.media.original_download_url(&viewer.id);
    let size = File {
        id: Some("F_DISPLAY".into()),
        size: Some(0),
        ..Default::default()
    };
    assert!(reconcile_metadata(&mut shell, "T1", &size));
    let kind = File {
        pretty_type: Some("Plain text".into()),
        mimetype: Some("video/mp4".into()),
        filetype: Some("list".into()),
        url_private: Some("https://files.slack.com/other".into()),
        ..size.clone()
    };
    assert!(reconcile_metadata(&mut shell, "T1", &kind));
    assert!(reconcile_metadata(
        &mut shell,
        "T1",
        &File {
            title: Some("  ".into()),
            name: Some("\t".into()),
            ..kind
        }
    ));
    let current = shell.viewer.as_ref().unwrap();
    assert_eq!(current.id, viewer.id);
    assert_eq!(current.mime, viewer.mime);
    assert_eq!(current.name, viewer.name);
    assert_eq!(current.download_name, viewer.download_name);
    assert_eq!(shell.media.original_download_url(&viewer.id), source);
    assert!(Arc::ptr_eq(
        &shell.media.ready_bytes(&viewer.id).unwrap(),
        &bytes
    ));
    let file = &shell.core.workspaces["T1"].messages["C1"]
        .messages
        .iter()
        .find(|m| m.ts.as_deref() == Some("990.0"))
        .unwrap()
        .files[0];
    assert_eq!(file.title.as_deref(), Some("Known title"));
    assert_eq!(file.size, Some(0));
    assert_eq!(file.pretty_type.as_deref(), Some("Plain text"));
    assert_eq!(file.mimetype.as_deref(), Some("text/plain"));
    assert!(file.filetype.is_none());
}
#[test]
fn empty_and_tombstoned_metadata_cannot_project() {
    let mut shell = setup();
    let empty = File {
        id: Some("F_DISPLAY".into()),
        pretty_type: Some(String::new()),
        ..Default::default()
    };
    assert!(!reconcile_metadata(&mut shell, "T1", &empty));
    assert!(!reconcile_metadata(
        &mut shell,
        "OTHER",
        &File {
            size: Some(0),
            ..empty.clone()
        }
    ));
    shell.core.remove_file("T1", "F_DISPLAY");
    assert!(!reconcile_metadata(
        &mut shell,
        "T1",
        &File {
            size: Some(0),
            ..empty
        }
    ));
}
