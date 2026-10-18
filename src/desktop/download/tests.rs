use super::*;
use super_platinum_core::{
    MediaAssetKind,
    config::{Session, WorkspaceSession},
    slack::{Error, Transport},
};
fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some("download-test".into());
    shell.core.session = Some(Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: "synthetic-cookie".into(),
        workspaces: std::collections::BTreeMap::from([(
            "T1".into(),
            WorkspaceSession {
                team_id: "T1".into(),
                user_id: "U1".into(),
                enterprise_id: None,
                name: "test".into(),
                url: "https://fixture.slack.com".into(),
                token: "synthetic-token".into(),
            },
        )]),
    });
    shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap()));
    shell
}
fn original(shell: &ShellState) -> MediaAssetId {
    let file = File {
        url_private: Some("https://files.slack.com/original.mov".into()),
        extra: std::collections::BTreeMap::from([(
            "url_private_download".into(),
            serde_json::json!("https://files.slack.com/download.mov"),
        )]),
        ..Default::default()
    };
    let id = shell.media.register_deferred(
        MediaAssetKind::Attachment,
        "https://files.slack.com/preview.mp4",
        "video/mp4",
    );
    shell.media.register_file_download(&id, &file);
    id
}
#[test]
fn original_is_available_without_preview_and_transcoded_cache_is_not_saved_as_original() {
    let shell = shell();
    let id = original(&shell);
    let (owner, bytes) = owner::Owner::capture(&shell, id.clone()).unwrap();
    assert!(bytes.is_none());
    assert!(owner.current(&shell));
    assert_eq!(
        owner.source.as_deref(),
        Some("https://files.slack.com/download.mov")
    );
    shell
        .media
        .insert(id.clone(), "video/mp4", b"transcoded".as_slice());
    assert!(shell.media.ready_bytes(&id).is_some());
    assert!(owner::Owner::capture(&shell, id).unwrap().1.is_none());
}
#[test]
fn cached_small_original_and_disconnected_fixture_keep_existing_save_admission() {
    let mut shell = shell();
    let file = File {
        url_private: Some("https://files.slack.com/original".into()),
        ..Default::default()
    };
    let id = shell.media.register_deferred(
        MediaAssetKind::Attachment,
        file.full_url().unwrap(),
        "text/plain",
    );
    shell.media.register_file_download(&id, &file);
    shell
        .media
        .insert(id.clone(), "text/plain", b"same original".as_slice());
    assert_eq!(
        owner::Owner::capture(&shell, id)
            .unwrap()
            .1
            .unwrap()
            .as_ref(),
        b"same original"
    );
    shell.core.session = None;
    shell.core.transport = None;
    let id = MediaAssetId::new(MediaAssetKind::Attachment);
    shell
        .media
        .insert(id.clone(), "text/plain", b"fixture".as_slice());
    assert!(owner::Owner::capture(&shell, id).unwrap().1.is_some());
}
#[test]
fn owner_rejects_epoch_credentials_transport_registry_and_original_source_replacement() {
    for case in 0..7 {
        let mut shell = shell();
        let id = original(&shell);
        let (owner, _) = owner::Owner::capture(&shell, id.clone()).unwrap();
        assert!(owner.current(&shell));
        match case {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.active_account = Some("other".into()),
            2 => shell.core.session.as_mut().unwrap().d_cookie = "changed".into(),
            3 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .token = "changed".into()
            }
            4 => shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap())),
            5 => shell.media = crate::media::MediaRegistry::default(),
            _ => shell.media.register_file_download(
                &id,
                &File {
                    url_private: Some("https://files.slack.com/replacement".into()),
                    ..Default::default()
                },
            ),
        }
        assert!(!owner.current(&shell), "case {case}");
    }
}
#[test]
fn late_limit_belongs_to_captured_stable_owner_across_shell_replacement() {
    let mut shell = shell();
    shell.core.active_account = Some(format!(
        "download-cooldown-{}",
        super_platinum_core::MediaAssetId::new(MediaAssetKind::Attachment).uri()
    ));
    let id = original(&shell);
    let (owner, _) = owner::Owner::capture(&shell, id).unwrap();
    assert!(cooldown::available(&owner.key));
    shell.core.account_epoch += 1;
    assert!(!owner.current(&shell));
    cooldown::record(
        &owner.key,
        &Err(DownloadError::Network(Error::HttpStatus {
            status: 503,
            retry_after_secs: Some(60),
        })),
    );
    assert!(!cooldown::available(&owner.key));
    let other = (
        Some("independent-download-account".into()),
        "T1".into(),
        "U1".into(),
    );
    assert!(cooldown::available(&other));
    cooldown::record(
        &owner.key,
        &Err(DownloadError::Network(Error::RateLimited {
            retry_after_secs: Some(1),
        })),
    );
    assert!(!cooldown::available(&owner.key));
}

#[test]
fn original_only_metadata_reaches_actual_files_open_and_message_projection() {
    use std::{cell::Cell, rc::Rc};
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(shell);
            slot.set(Some(state));
            VNode::empty()
        },
        slot.clone(),
    );
    dom.rebuild_to_vec();
    let state = slot.get().unwrap();
    for has_preview in [false, true] {
        let file = File {
            id: Some("F_ORIGINAL_ONLY".into()),
            name: Some("large.bin".into()),
            url_private: has_preview.then(|| "https://files.slack.com/rendition".into()),
            extra: std::collections::BTreeMap::from([(
                "url_private_download".into(),
                serde_json::json!("https://files.slack.com/original-only"),
            )]),
            ..Default::default()
        };
        let expected = file
            .full_url()
            .unwrap_or("https://files.slack.com/original-only");
        dom.in_runtime(|| crate::files::open(state, file.clone()));
        let shell = state.peek();
        let viewer = shell
            .viewer
            .as_ref()
            .expect("Files opens download-only metadata");
        assert!(matches!(shell.overlay, Some(crate::state::Overlay::Viewer)));
        let expected_id = shell.media.register_deferred(
            MediaAssetKind::Attachment,
            expected,
            super_platinum_core::file_preview::mime_for_file(&file),
        );
        assert_eq!(viewer.id, expected_id);
        assert_eq!(
            shell.media.original_download_url(&viewer.id).as_deref(),
            Some("https://files.slack.com/original-only")
        );
        assert!(owner::Owner::capture(&shell, viewer.id.clone()).is_some());
        let crate::model::RichNode::Media {
            full: Some(full), ..
        } = crate::message_vm::file_node(&shell.media, &file)
        else {
            panic!("message file remains an actionable media card")
        };
        assert_eq!(full, expected_id);
        assert!(owner::Owner::capture(&shell, full).is_some());
    }
}
