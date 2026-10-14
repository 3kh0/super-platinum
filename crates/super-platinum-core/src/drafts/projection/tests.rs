use super::*;
use crate::{ComposerState, TextRange, domain::ComposerAttachment};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64},
};
use std::time::Instant;

fn key(channel: &str, thread: Option<&str>) -> DraftKey {
    DraftKey {
        team: "T1".into(),
        channel: channel.into(),
        thread: thread.map(str::to_owned),
    }
}
fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}
fn file(id: u64) -> ComposerAttachment {
    ComposerAttachment {
        id,
        path: format!("fixture-{id}.txt").into(),
        name: format!("file-{id}"),
        bytes: id + 4,
        uploading: false,
        upload_started: None,
        upload_cancel: None,
        upload_progress: None,
        preview_path: Some(format!("preview-{id}.png").into()),
    }
}
fn rich() -> LocalDraft {
    let mut draft = local("Unicode \u{65e5}\u{672c}\u{8a9e} \u{1f98a}");
    draft.composer.selection = TextRange { start: 0, end: 7 };
    draft.composer.reply_broadcast = true;
    draft.attachments = vec![file(1), file(2)];
    draft
}

#[test]
fn replace_then_remove_only_target_preserves_bindings_and_unrelated_dirty_edits() {
    let target = key("C1", Some("1.1"));
    let channel = key("C1", None);
    let other_thread = key("C1", Some("2.2"));
    let before = rich();
    let mut book = DraftBook::restored(vec![(target.clone(), before.clone())]);
    book.channel = Some(channel.clone());
    book.thread = Some(target.clone());
    for (key, draft) in [
        (&channel, local("channel input")),
        (&other_thread, local("other reply")),
    ] {
        book.capture(key, &draft.composer, &draft.attachments);
    }
    let dirty = book.changed.clone();
    let mut after = rich();
    after.composer.text = "Projected".into();
    after.composer.selection = TextRange { start: 1, end: 5 };
    after.composer.reply_broadcast = false;
    after.attachments.reverse();
    book.project_clean(&target, Some(&before), &after).unwrap();
    assert!(exact_idle(&book.items[&target], &after));
    book.project_clean(&target, Some(&after), &LocalDraft::default())
        .unwrap();
    assert!(!book.items.contains_key(&target));
    assert_eq!(book.channel.as_ref(), Some(&channel));
    assert_eq!(book.thread.as_ref(), Some(&target));
    let changes = book.take_changes();
    assert_eq!(
        changes.keys().collect::<Vec<_>>(),
        dirty.keys().collect::<Vec<_>>()
    );
    for (key, expected) in dirty {
        assert!(exact_idle(&changes[&key], &expected));
        assert!(exact_idle(&book.items[&key], &expected));
    }
}

#[test]
fn dirty_equal_target_and_dirty_absent_deletion_are_both_refused() {
    let target = key("C1", None);
    let before = local("local input");
    let mut book = DraftBook::default();
    book.capture(&target, &before.composer, &[]);
    assert_eq!(
        book.project_clean(&target, Some(&before), &local("remote")),
        Err(CleanProjectionError::DirtyTarget)
    );
    assert!(exact_idle(&book.items[&target], &before));
    assert!(exact_idle(&book.changed[&target], &before));
    book.capture(&target, &ComposerState::default(), &[]);
    assert_eq!(
        book.project_clean(&target, None, &local("remote")),
        Err(CleanProjectionError::DirtyTarget)
    );
    assert!(!book.items.contains_key(&target));
    assert!(book.changed[&target].empty());
}

#[test]
fn missing_predecessor_is_distinct_from_present_empty_and_install_is_clean() {
    let target = key("C1", None);
    let empty = LocalDraft::default();
    let mut book = DraftBook::default();
    assert_eq!(
        book.project_clean(&target, Some(&empty), &local("remote")),
        Err(CleanProjectionError::PredecessorMismatch)
    );
    assert!(book.items.is_empty());
    book.project_clean(&target, None, &local("remote")).unwrap();
    assert!(book.take_changes().is_empty());
    let mut book = DraftBook::restored(vec![(target.clone(), empty.clone())]);
    assert_eq!(
        book.project_clean(&target, None, &local("remote")),
        Err(CleanProjectionError::PredecessorMismatch)
    );
    assert!(book.items[&target].empty());
    book.project_clean(&target, Some(&empty), &empty).unwrap();
    assert!(book.items.is_empty());
    assert!(book.take_changes().is_empty());
}

#[test]
fn any_composer_or_ordered_attachment_predecessor_change_refuses_without_mutation() {
    let target = key("C1", Some("1.1"));
    let before = rich();
    let changes: [fn(&mut LocalDraft); 10] = [
        |d| d.composer.text.push('x'),
        |d| d.composer.selection.start += 1,
        |d| d.composer.selection.end += 1,
        |d| d.composer.reply_broadcast = false,
        |d| d.attachments.reverse(),
        |d| d.attachments[0].id += 1,
        |d| d.attachments[0].path = "different.txt".into(),
        |d| d.attachments[0].name.push('x'),
        |d| d.attachments[0].bytes += 1,
        |d| d.attachments[0].preview_path = None,
    ];
    for change in changes {
        let mut current = before.clone();
        change(&mut current);
        let mut book = DraftBook::restored(vec![(target.clone(), current.clone())]);
        assert_eq!(
            book.project_clean(&target, Some(&before), &local("remote")),
            Err(CleanProjectionError::PredecessorMismatch)
        );
        assert!(exact_idle(&book.items[&target], &current));
        assert!(book.take_changes().is_empty());
    }
    let mut current = before.clone();
    current.attachments.pop();
    let mut book = DraftBook::restored(vec![(target.clone(), current.clone())]);
    assert_eq!(
        book.project_clean(&target, Some(&before), &local("remote")),
        Err(CleanProjectionError::PredecessorMismatch)
    );
    assert!(exact_idle(&book.items[&target], &current));
}

#[test]
fn upload_state_and_capabilities_refuse_in_current_expected_and_replacement() {
    let target = key("C1", None);
    let changes: [fn(&mut LocalDraft); 4] = [
        |d| d.attachments[0].uploading = true,
        |d| d.attachments[0].upload_started = Some(Instant::now()),
        |d| d.attachments[0].upload_cancel = Some(Arc::new(AtomicBool::new(false))),
        |d| d.attachments[0].upload_progress = Some(Arc::new(AtomicU64::new(0))),
    ];
    for change in changes {
        for side in 0..3 {
            let (mut current, mut expected, mut replacement) = (rich(), rich(), rich());
            change(match side {
                0 => &mut current,
                1 => &mut expected,
                _ => &mut replacement,
            });
            let mut book = DraftBook::restored(vec![(target.clone(), current.clone())]);
            assert_eq!(
                book.project_clean(&target, Some(&expected), &replacement),
                Err(CleanProjectionError::ActiveUpload)
            );
            assert!(exact_idle(&book.items[&target], &current));
            assert_eq!(active_upload(&book.items[&target]), active_upload(&current));
            assert!(book.take_changes().is_empty());
        }
        let mut same = rich();
        change(&mut same);
        let mut book = DraftBook::restored(vec![(target.clone(), same.clone())]);
        assert_eq!(
            book.project_clean(&target, Some(&same), &same),
            Err(CleanProjectionError::ActiveUpload)
        );
    }
}

#[test]
fn broadcast_or_attachments_only_replacements_are_not_removed_as_empty() {
    let target = key("C1", Some("1.1"));
    for attachments in [false, true] {
        let mut after = LocalDraft::default();
        if attachments {
            after.attachments.push(file(1));
        } else {
            after.composer.reply_broadcast = true;
        }
        let mut book = DraftBook::default();
        book.project_clean(&target, None, &after).unwrap();
        assert!(exact_idle(&book.items[&target], &after));
        assert!(book.take_changes().is_empty());
    }
}
