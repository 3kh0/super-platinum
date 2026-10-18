use super::*;
use super_platinum_core::{
    ComposerState,
    domain::ComposerAttachment,
    drafts::{DraftBook, DraftKey},
};

fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}
fn file(id: u64) -> ComposerAttachment {
    ComposerAttachment {
        id,
        path: format!("synthetic-{id}.txt").into(),
        name: format!("file-{id}"),
        bytes: id,
        uploading: false,
        upload_started: None,
        upload_cancel: None,
        upload_progress: None,
        preview_path: Some(format!("preview-{id}.png").into()),
    }
}
fn setup(target: ComposerTarget) -> (ShellState, DraftKey, LocalDraft) {
    // No fixture environment, real settings/session, file bytes, or native runtime.
    let mut shell = ShellState::opening(crate::media::MediaRegistry::default());
    let key = DraftKey {
        team: "T1".into(),
        channel: "C1".into(),
        thread: (target == ComposerTarget::Thread).then(|| "1.000001".into()),
    };
    let mut before = local("Original \u{65e5}\u{1f98a}");
    before.attachments = vec![file(1), file(2)];
    before.composer.set_selection(0, 8);
    shell.core.drafts = DraftBook::restored(vec![(key.clone(), before.clone())]);
    match target {
        ComposerTarget::Channel => {
            shell.core.composer = before.composer.clone();
            shell.core.composer_attachments = before.attachments.clone();
            shell.core.drafts.channel = Some(key.clone());
        }
        ComposerTarget::Thread => {
            shell.core.thread_composer = before.composer.clone();
            shell.core.thread_composer_attachments = before.attachments.clone();
            shell.core.drafts.thread = Some(key.clone());
        }
    }
    (shell, key, before)
}

#[test]
fn baseline_remote_text_pair_install_removes_only_exact_old_attachment_chips() {
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let (mut shell, key, before) = setup(target);
        let other = DraftKey {
            team: "T1".into(),
            channel: "C2".into(),
            thread: None,
        };
        shell.core.drafts.capture(
            &other,
            &ComposerState::with_text("Unrelated newer draft"),
            &[],
        );
        let remote = local("Selected Remote \u{1f98a}");
        assert!(replacement(&remote).is_some());
        assert!(same_book(Policy::Baseline, Some(&before), Some(&before)));
        assert!(!same_book(Policy::Plain, Some(&before), Some(&before)));
        install_pair(
            &mut shell,
            target,
            &key,
            Policy::Baseline,
            &before,
            Some(&before),
            &remote,
        )
        .unwrap();
        assert!(Policy::Plain.same(&live(&shell, target), &remote));
        assert!(Policy::Plain.same(shell.core.drafts.items.get(&key).unwrap(), &remote));
        let changes = shell.core.drafts.take_changes();
        assert_eq!(changes.len(), 1);
        assert!(changes.contains_key(&other));
    }
}

#[test]
fn generic_projection_and_selected_after_files_remain_unsupported() {
    let (mut shell, key, before) = setup(ComposerTarget::Channel);
    let remote = local("Remote");
    assert!(matches!(
        install_pair(
            &mut shell,
            ComposerTarget::Channel,
            &key,
            Policy::Plain,
            &before,
            Some(&before),
            &remote
        ),
        Err(Fault::Stale)
    ));
    assert!(Policy::Baseline.same(&live(&shell, ComposerTarget::Channel), &before));
    let mut after = remote;
    after.attachments.push(file(3));
    assert!(replacement(&after).is_none());
    assert!(matches!(
        install_pair(
            &mut shell,
            ComposerTarget::Channel,
            &key,
            Policy::Baseline,
            &before,
            Some(&before),
            &after
        ),
        Err(Fault::Unsupported)
    ));
    assert!(Policy::Baseline.same(shell.core.drafts.items.get(&key).unwrap(), &before));
}

#[test]
fn late_file_text_selection_or_book_change_prevents_the_entire_pair_install() {
    for change in 0..6 {
        let (mut shell, key, before) = setup(ComposerTarget::Channel);
        match change {
            0 => shell.core.composer_attachments[0].preview_path = None,
            1 => shell.core.composer_attachments.swap(0, 1),
            2 => shell.core.composer.text.push('x'),
            3 => shell.core.composer.set_selection(1, 2),
            4 => shell.core.drafts.items.get_mut(&key).unwrap().attachments[0].bytes += 1,
            _ => shell.core.composer_attachments[0].uploading = true,
        }
        let live_before = live(&shell, ComposerTarget::Channel);
        let book_before = serde_json::to_value(shell.core.drafts.items.get(&key)).unwrap();
        assert!(matches!(
            install_pair(
                &mut shell,
                ComposerTarget::Channel,
                &key,
                Policy::Baseline,
                &before,
                Some(&before),
                &local("Remote")
            ),
            Err(Fault::Stale)
        ));
        assert_eq!(shell.core.composer, live_before.composer);
        assert_eq!(
            shell.core.composer_attachments.len(),
            live_before.attachments.len()
        );
        assert_eq!(
            serde_json::to_value(shell.core.drafts.items.get(&key)).unwrap(),
            book_before
        );
    }
}

#[test]
fn target_dirty_book_refusal_keeps_live_files_and_text() {
    let (mut shell, key, before) = setup(ComposerTarget::Thread);
    shell.core.drafts.capture(
        &key,
        &ComposerState::with_text("dirty transient"),
        &before.attachments,
    );
    shell.core.drafts.items.insert(key.clone(), before.clone());
    assert!(matches!(
        install_pair(
            &mut shell,
            ComposerTarget::Thread,
            &key,
            Policy::Baseline,
            &before,
            Some(&before),
            &local("Remote")
        ),
        Err(Fault::Stale)
    ));
    assert!(Policy::Baseline.same(&live(&shell, ComposerTarget::Thread), &before));
    assert!(shell.core.drafts.take_changes().contains_key(&key));
}
