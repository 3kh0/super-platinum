use super::*;

fn private_root() -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("sp-paste-test-{}-{stamp}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    root
}

fn payload(name: &str, bytes: &[u8]) -> serde_json::Value {
    serde_json::json!({"name":name,"data":base64::engine::general_purpose::STANDARD.encode(bytes)})
}

#[test]
fn fixture_storage_requires_disconnected_explicit_absolute_root() {
    let absolute = std::env::temp_dir().join("sp-paste-gate-only");
    assert_eq!(storage_mode(false, false, None), Some(Storage::Production));
    assert!(storage_mode(true, true, None).is_none());
    assert!(storage_mode(true, true, Some("relative".into())).is_none());
    assert!(storage_mode(true, true, Some(absolute.join(".."))).is_none());
    assert!(storage_mode(true, false, Some(absolute.clone())).is_none());
    assert!(storage_mode(false, true, Some(absolute.clone())).is_none());
    assert_eq!(
        storage_mode(true, true, Some(absolute.clone())),
        Some(Storage::Fixture(absolute))
    );
}

#[test]
fn isolated_files_stay_in_private_root_and_never_replace_existing_bytes() {
    let root = private_root();
    let existing = root.join("existing.txt");
    write_new(&existing, b"original").unwrap();
    assert_eq!(
        write_new(&existing, b"replacement").unwrap_err().kind(),
        std::io::ErrorKind::AlreadyExists
    );
    let paths = save_files(
        Storage::Fixture(root.clone()),
        vec![
            payload("../../a.png", b"first"),
            payload("../../a.png", b"second"),
        ],
    );
    assert_eq!(paths.len(), 2);
    assert_ne!(paths[0].path, paths[1].path);
    assert!(paths.iter().all(|file| file.name == "a.png"));
    for (file, expected) in paths
        .iter()
        .zip([b"first".as_slice(), b"second".as_slice()])
    {
        assert_eq!(
            file.path.parent(),
            Some(root.canonicalize().unwrap().as_path())
        );
        assert_ne!(file.path.file_name().unwrap().to_str().unwrap(), file.name);
        assert_eq!(std::fs::read(&file.path).unwrap(), expected);
        std::fs::remove_file(&file.path).unwrap();
    }
    assert_eq!(std::fs::read(&existing).unwrap(), b"original");
    std::fs::remove_file(existing).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn absent_root_and_oversized_fixture_batch_fail_closed() {
    let root = private_root();
    assert!(
        save_files(
            Storage::Fixture(root.join("missing")),
            vec![payload("a", b"x")]
        )
        .is_empty()
    );
    assert!(!root.join("missing").exists());
    assert!(save_files(Storage::Fixture(root.clone()), vec![payload("a", b"x"); 17]).is_empty());
    assert!(
        save_files(
            Storage::Fixture(root.clone()),
            vec![serde_json::json!({"name":"a","data":"!invalid"})]
        )
        .is_empty()
    );
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn paste_owner_rejects_channel_account_and_thread_replacements() {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    let channel = crate::view::composer::field_scope(&shell, ComposerTarget::Channel);
    assert!(owns_destination(
        &shell,
        ComposerTarget::Channel,
        &channel,
        true
    ));
    shell.channel_generation += 1;
    assert!(!owns_destination(
        &shell,
        ComposerTarget::Channel,
        &channel,
        true
    ));
    let channel = crate::view::composer::field_scope(&shell, ComposerTarget::Channel);
    shell.core.account_epoch += 1;
    assert!(!owns_destination(
        &shell,
        ComposerTarget::Channel,
        &channel,
        true
    ));
    shell.thread_root = Some("100.000001".into());
    let thread = crate::view::composer::field_scope(&shell, ComposerTarget::Thread);
    assert!(owns_destination(
        &shell,
        ComposerTarget::Thread,
        &thread,
        true
    ));
    shell.thread_generation += 1;
    assert!(!owns_destination(
        &shell,
        ComposerTarget::Thread,
        &thread,
        true
    ));
    let thread = crate::view::composer::field_scope(&shell, ComposerTarget::Thread);
    shell.thread_root = None;
    assert!(!owns_destination(
        &shell,
        ComposerTarget::Thread,
        &thread,
        true
    ));
}

#[test]
fn named_pastes_keep_unicode_metadata_and_distinct_paths_through_attachment_roundtrip() {
    let root = private_root();
    let files = save_files(
        Storage::Fixture(root.clone()),
        vec![
            payload("日本語 🦊.txt", b"first"),
            payload("日本語 🦊.txt", b"second"),
        ],
    );
    assert_eq!(files.len(), 2);
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.add_named_attachments(
        files.iter().map(|f| (f.path.clone(), f.name.clone())),
        ComposerTarget::Channel,
    );
    assert_eq!(shell.core.composer_attachments.len(), 2);
    for attachment in &shell.core.composer_attachments {
        assert_eq!(attachment.name, "日本語 🦊.txt");
        assert_ne!(
            attachment.path.file_name().unwrap().to_str().unwrap(),
            attachment.name
        );
        let value = serde_json::to_value(attachment).unwrap();
        let restored: super_platinum_core::domain::ComposerAttachment =
            serde_json::from_value(value).unwrap();
        assert_eq!(restored.path, attachment.path);
        assert_eq!(restored.name, attachment.name);
        assert_eq!(restored.bytes, attachment.bytes);
    }
    let ordinary = root.join("chooser.txt");
    write_new(&ordinary, b"ordinary").unwrap();
    shell.thread_root = Some("100.000001".into());
    shell.add_attachments([ordinary.clone()], ComposerTarget::Thread);
    assert_eq!(
        shell.core.thread_composer_attachments[0].name,
        "chooser.txt"
    );
    for file in files {
        std::fs::remove_file(file.path).unwrap();
    }
    std::fs::remove_file(ordinary).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn overlong_paste_metadata_creates_no_file() {
    let root = private_root();
    assert!(
        save_files(
            Storage::Fixture(root.clone()),
            vec![payload(&"a".repeat(1025), b"bytes")]
        )
        .is_empty()
    );
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    std::fs::remove_dir(root).unwrap();
}
