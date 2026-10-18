use super::*;
use super_platinum_core::{
    MediaAssetKind,
    config::{Session, WorkspaceSession},
    slack::Transport,
};

pub(crate) fn file() -> File {
    File {
        id: Some("F_REFRESH".into()),
        name: Some("notes.txt".into()),
        title: Some("Notes".into()),
        mimetype: Some("text/plain".into()),
        filetype: Some("text".into()),
        is_external: Some(false),
        url_private: Some("https://files.slack.com/notes".into()),
        extra: std::collections::BTreeMap::from([("mode".into(), serde_json::json!("hosted"))]),
        ..Default::default()
    }
}
pub(crate) fn shell() -> ShellState {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    shell.core.active_account = Some("refresh-test".into());
    shell.core.session = Some(Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: "fixture-only".into(),
        workspaces: std::collections::BTreeMap::from([(
            "T1".into(),
            WorkspaceSession {
                team_id: "T1".into(),
                user_id: "U1".into(),
                enterprise_id: None,
                name: "fixture".into(),
                url: "https://fixture.slack.com".into(),
                token: "fixture-only".into(),
            },
        )]),
    });
    shell.core.transport = Some(Arc::new(Transport::disconnected_fixture("fixture-only")));
    let f = file();
    let id = shell.media.register_deferred(
        MediaAssetKind::Attachment,
        f.url_private.as_deref().unwrap(),
        "text/plain",
    );
    shell.media.register_file_download(&id, &f);
    shell
        .media
        .insert(id.clone(), "text/plain", b"old contents".as_slice());
    shell.viewer = Some(ViewerVm {
        file: file_actions::identity(&f),
        id,
        name: "Notes".into(),
        download_name: "notes.txt".into(),
        mime: "text/plain".into(),
    });
    shell.overlay = Some(Overlay::Viewer);
    opened(&mut shell);
    shell
}
fn begin(shell: &mut ShellState) -> Ticket {
    let lease = shell.file_refresh.active.as_ref().unwrap().lease.clone();
    Ticket::begin(shell, lease).unwrap()
}
pub(crate) fn publish(shell: &mut ShellState) {
    let ticket = begin(shell);
    let f = file();
    let source = source::Source::parse(&f, "F_REFRESH").unwrap();
    assert!(ticket.publish(shell, &f, &source, Arc::from(b"new contents".as_slice())));
}
#[test]
fn same_url_and_size_refresh_gets_distinct_bytes_without_mutating_shared_old_asset() {
    let mut shell = shell();
    let old = shell.viewer.clone().unwrap();
    let old_bytes = shell.media.ready_bytes(&old.id).unwrap();
    let old_lease = lease(&shell, &old.id).unwrap();
    let cancel = old_lease.cancellation();
    publish(&mut shell);
    let new = shell.viewer.as_ref().unwrap();
    assert_ne!(new.id, old.id);
    assert_eq!(
        shell.media.ready_download_bytes(&new.id).unwrap().as_ref(),
        b"new contents"
    );
    assert!(Arc::ptr_eq(
        &old_bytes,
        &shell.media.ready_bytes(&old.id).unwrap()
    ));
    assert_eq!(
        shell.media.register_deferred(
            MediaAssetKind::Attachment,
            file().url_private.as_deref().unwrap(),
            "text/plain"
        ),
        old.id
    );
    assert_eq!(
        shell.media.original_download_url(&new.id),
        shell.media.original_download_url(&old.id)
    );
    assert!(cancel.load(Ordering::Acquire));
    assert!(!old_lease.current(&shell));
    assert!(
        !lease(&shell, &new.id)
            .unwrap()
            .cancellation()
            .load(Ordering::Acquire)
    );
    // A late old-asset media completion cannot overwrite the new identity.
    shell
        .media
        .insert(old.id, "text/plain", b"late old fetch".as_slice());
    assert_eq!(
        shell.media.ready_bytes(&new.id).unwrap().as_ref(),
        b"new contents"
    );
}
#[test]
fn failed_refresh_keeps_exact_viewer_bytes_and_download_latch() {
    let mut shell = shell();
    let old = shell.viewer.clone().unwrap();
    let bytes = shell.media.ready_bytes(&old.id).unwrap();
    let latch = lease(&shell, &old.id).unwrap().cancellation();
    let t = begin(&mut shell);
    t.fail(&mut shell, "Failed. Try again.", None);
    assert_eq!(shell.viewer.as_ref(), Some(&old));
    assert!(Arc::ptr_eq(
        &bytes,
        &shell.media.ready_bytes(&old.id).unwrap()
    ));
    assert!(!latch.load(Ordering::Acquire));
    assert!(retained_preview(&shell, &old.id));
    assert!(Arc::ptr_eq(
        &bytes,
        shell
            .file_refresh
            .active
            .as_ref()
            .unwrap()
            .held_preview
            .as_ref()
            .unwrap()
    ));
    assert!(!shell.file_refresh.active.as_ref().unwrap().busy);
    assert!(begin(&mut shell).current(&shell));
}
#[test]
fn metadata_requires_complete_exact_supported_hosted_text_source() {
    assert!(source::Source::parse(&file(), "F_REFRESH").is_some());
    for case in 0..11 {
        let mut f = file();
        match case {
            0 => f.id = Some("OTHER".into()),
            1 => f.mimetype = None,
            2 => f.url_private = None,
            3 => f.is_external = None,
            4 => f.is_external = Some(true),
            5 => f.mimetype = Some("video/mp4".into()),
            6 => {
                f.extra.insert("mode".into(), serde_json::json!("external"));
            }
            7 => f.filetype = Some("canvas".into()),
            8 => f.url_private = Some("https://outside.invalid/notes".into()),
            9 => {
                f.extra
                    .insert("mode".into(), serde_json::json!("tombstone"));
            }
            _ => f.url_private = Some("https://user@files.slack.com/notes".into()),
        }
        assert!(
            source::Source::parse(&f, "F_REFRESH").is_none(),
            "case {case}"
        );
    }
    let mut rename = file();
    rename.title = Some("Renamed".into());
    assert!(source::Source::parse(&rename, "F_REFRESH").is_some());
}
#[test]
fn event_during_either_read_refuses_publication_and_allows_explicit_new_attempt() {
    let mut shell = shell();
    let old = shell.viewer.clone();
    let t = begin(&mut shell);
    changed(&mut shell, "T1", "OTHER");
    assert!(t.current(&shell));
    changed(&mut shell, "T1", "F_REFRESH");
    assert!(!t.current(&shell));
    assert!(!t.publish(
        &mut shell,
        &file(),
        &source::Source::parse(&file(), "F_REFRESH").unwrap(),
        Arc::from(b"stale".as_slice())
    ));
    t.fail(&mut shell, "old failure", None);
    assert_eq!(shell.viewer, old);
    let new = begin(&mut shell);
    t.fail(&mut shell, "late error", None);
    assert!(new.current(&shell));
}
#[test]
fn account_credentials_socket_registry_and_same_file_reopen_retire_old_ticket() {
    for case in 0..9 {
        let mut shell = shell();
        let t = begin(&mut shell);
        match case {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.active_account = Some("other".into()),
            2 => shell.core.session.as_mut().unwrap().d_cookie = "rotated".into(),
            3 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .token = "rotated".into()
            }
            4 => shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
            5 => {
                shell.core.transport =
                    Some(Arc::new(Transport::disconnected_fixture("fixture-only")))
            }
            6 => shell.media = MediaRegistry::default(),
            7 => opened(&mut shell),
            _ => shell.dismiss_overlay(),
        }
        assert!(!t.current(&shell), "case {case}");
        assert!(!t.publish(
            &mut shell,
            &file(),
            &source::Source::parse(&file(), "F_REFRESH").unwrap(),
            Arc::from(b"stale".as_slice())
        ));
    }
}
#[test]
fn scope_rotation_failure_does_not_leave_same_visible_preview_busy_forever() {
    let mut shell = shell();
    let t = begin(&mut shell);
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1;
    assert!(!t.current(&shell));
    t.fail(&mut shell, "Refresh canceled. Try again.", None);
    assert!(!shell.file_refresh.active.as_ref().unwrap().busy);
    assert!(begin(&mut shell).current(&shell));
}
#[test]
fn exact_attempt_and_revision_exhaustion_refuse_without_reusing_identity() {
    let mut shell = shell();
    shell.file_refresh.active.as_mut().unwrap().attempt = u64::MAX;
    let l = shell.file_refresh.active.as_ref().unwrap().lease.clone();
    assert!(Ticket::begin(&mut shell, l).is_none());
    opened(&mut shell);
    let t = begin(&mut shell);
    shell.file_refresh.active.as_mut().unwrap().revision = u64::MAX;
    changed(&mut shell, "T1", "F_REFRESH");
    assert!(!t.current(&shell));
    t.fail(&mut shell, "stopped", None);
    let l = shell.file_refresh.active.as_ref().unwrap().lease.clone();
    assert!(Ticket::begin(&mut shell, l).is_none());
}
#[test]
fn retry_after_refuses_early_manual_attempt_and_preserves_preview() {
    let mut shell = shell();
    let old = shell.viewer.clone();
    let t = begin(&mut shell);
    t.error(
        &mut shell,
        &Error::RateLimited {
            retry_after_secs: Some(60),
        },
    );
    let l = shell.file_refresh.active.as_ref().unwrap().lease.clone();
    assert!(Ticket::begin(&mut shell, l.clone()).is_none());
    assert_eq!(shell.viewer, old);
    shell.file_refresh.retry_at = Some(Instant::now());
    assert!(Ticket::begin(&mut shell, l).is_some());
}

#[test]
fn reopening_same_asset_rotates_download_mount_key_and_cannot_revive_old_latch() {
    let mut shell = shell();
    let id = shell.viewer.as_ref().unwrap().id.clone();
    let old = lease(&shell, &id).unwrap();
    let old_key = old.key();
    opened(&mut shell);
    let new = lease(&shell, &id).unwrap();
    assert_ne!(new.key(), old_key);
    assert!(!old.current(&shell));
    assert!(new.current(&shell));
    assert!(old.cancellation().load(Ordering::Acquire));
    assert!(!new.cancellation().load(Ordering::Acquire));
}

#[test]
fn retry_after_survives_close_reopen_and_overflow_refuses() {
    let mut shell = shell();
    let t = begin(&mut shell);
    t.error(
        &mut shell,
        &Error::RateLimited {
            retry_after_secs: Some(60),
        },
    );
    close(&mut shell);
    opened(&mut shell);
    let l = shell.file_refresh.active.as_ref().unwrap().lease.clone();
    assert!(Ticket::begin(&mut shell, l).is_none());
    shell.file_refresh.retry_at = None;
    let t = begin(&mut shell);
    t.error(
        &mut shell,
        &Error::RateLimited {
            retry_after_secs: Some(u64::MAX),
        },
    );
    let l = shell.file_refresh.active.as_ref().unwrap().lease.clone();
    assert!(Ticket::begin(&mut shell, l).is_none());
}

#[test]
fn content_read_omitting_display_names_does_not_erase_known_caption_or_filename() {
    let mut shell = shell();
    let old = shell.viewer.clone().unwrap();
    let t = begin(&mut shell);
    let mut f = file();
    f.name = None;
    f.title = None;
    let source = source::Source::parse(&f, "F_REFRESH").unwrap();
    assert!(t.publish(&mut shell, &f, &source, Arc::from(b"fresh".as_slice())));
    let v = shell.viewer.as_ref().unwrap();
    assert_eq!(v.name, old.name);
    assert_eq!(v.download_name, old.download_name);
    assert_ne!(v.id, old.id);
}

#[test]
fn xml_suffix_image_is_not_a_text_viewer_refresh() {
    let mut shell = shell();
    shell.viewer.as_mut().unwrap().mime = "image/svg+xml".into();
    opened(&mut shell);
    assert!(shell.file_refresh.active.is_none());
    let mut f = file();
    f.mimetype = Some("image/svg+xml".into());
    assert!(source::Source::parse(&f, "F_REFRESH").is_none());
    assert!(source::text_mime("application/ld+json"));
}

// Called inside rename's own tests so its production reconciliation remains private.
pub(crate) fn local_rename_interleaving(reconcile: impl FnOnce(&mut ShellState, &File)) {
    let mut shell = shell();
    let old = shell.viewer.clone().unwrap();
    let old_bytes = shell.media.ready_bytes(&old.id).unwrap();
    let old_cancel = lease(&shell, &old.id).unwrap().cancellation();
    let ticket = begin(&mut shell);
    // Metadata has returned; the byte read is still in flight. URL and size do
    // not change when the later confirmed title/name arrives locally.
    let earlier_metadata = file();
    let source = source::Source::parse(&earlier_metadata, "F_REFRESH").unwrap();
    let mut renamed = earlier_metadata.clone();
    renamed.title = Some("Confirmed local title".into());
    renamed.name = Some("confirmed-local-name.txt".into());
    reconcile(&mut shell, &renamed);
    let confirmed = shell.viewer.clone().unwrap();
    assert_eq!(confirmed.name, "Confirmed local title");
    assert_eq!(confirmed.download_name, "confirmed-local-name.txt");
    assert_eq!(confirmed.id, old.id);
    assert!(!ticket.current(&shell));
    assert!(!ticket.publish(
        &mut shell,
        &earlier_metadata,
        &source,
        Arc::from(b"late body after local rename".as_slice()),
    ));
    ticket.fail(&mut shell, "Unexpected fallback", None);
    assert_eq!(shell.viewer.as_ref(), Some(&confirmed));
    assert!(Arc::ptr_eq(
        &old_bytes,
        &shell.media.ready_bytes(&old.id).unwrap()
    ));
    assert!(!old_cancel.load(Ordering::Acquire));
    assert!(!shell.file_refresh.active.as_ref().unwrap().busy);
    assert_eq!(
        shell.file_refresh.active.as_ref().unwrap().notice,
        "This file changed during refresh. Refresh again."
    );
    let next = begin(&mut shell);
    assert!(next.publish(
        &mut shell,
        &renamed,
        &source,
        Arc::from(b"current retry".as_slice())
    ));
    assert_eq!(shell.viewer.as_ref().unwrap().name, confirmed.name);
    assert_eq!(
        shell.viewer.as_ref().unwrap().download_name,
        confirmed.download_name
    );
}
