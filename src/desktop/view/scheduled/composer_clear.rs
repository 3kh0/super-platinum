//! A native receipt precedes local settlement. No empty checkpoint is written
//! while the DOM may still own newer input or a composition session.
use super::{ComposeSource, ComposerTarget, ShellState};
use crate::view::composer::{ScheduledClearReceipt, field_scope, settle_scheduled_field};
use dioxus::prelude::*;
use super_platinum_core::drafts::LocalDraft;

struct Before {
    live: LocalDraft,
    book: Option<LocalDraft>,
}

pub(super) async fn complete_source(mut state: Signal<ShellState>, source: &ComposeSource) {
    let before = {
        let mut shell = state.write();
        if !owns_account(source, &shell) {
            return;
        }
        if shell.composer_key(target(source)).as_ref() != Some(&source.key()) {
            // The source was already offscreen before this completion; retain
            // the existing exact-source stored-draft/attachment settlement.
            source.clear_if_unchanged(&mut shell);
            shell.checkpoint_composers();
            return;
        }
        if !same_visit(source, &shell) {
            return;
        }
        let live = live(&shell, target(source));
        if shell
            .core
            .drafts
            .items
            .get(&source.key())
            .is_some_and(|book| !same_text(book, &live, true))
        {
            settle_attachments(&mut shell, source);
            return;
        }
        if live.composer.text != source.text
            || live.composer.reply_broadcast != source.reply_broadcast
        {
            // Newer Rust input already has a checkpoint. Only retire attachments
            // included in the completed request; never touch its native field.
            source.clear_if_unchanged(&mut shell);
            shell.checkpoint_composers();
            return;
        }
        Before {
            live,
            book: shell.core.drafts.items.get(&source.key()).cloned(),
        }
    };
    let id = if source.thread.is_some() {
        "thread-composer"
    } else {
        "channel-composer"
    };
    let receipt = settle_scheduled_field(id, &source.field_scope, &source.text).await;
    apply_receipt(&mut state.write(), source, &before, receipt);
}

fn target(source: &ComposeSource) -> ComposerTarget {
    if source.thread.is_some() {
        ComposerTarget::Thread
    } else {
        ComposerTarget::Channel
    }
}
fn owns_account(source: &ComposeSource, shell: &ShellState) -> bool {
    shell.core.active_account == source.account && shell.core.account_epoch == source.epoch
}
fn same_visit(source: &ComposeSource, shell: &ShellState) -> bool {
    owns_account(source, shell)
        && field_scope(shell, target(source)) == source.field_scope
        && source
            .thread_generation
            .is_none_or(|generation| generation == shell.thread_generation)
}
fn live(shell: &ShellState, target: ComposerTarget) -> LocalDraft {
    match target {
        ComposerTarget::Channel => LocalDraft {
            composer: shell.core.composer.clone(),
            attachments: shell.core.composer_attachments.clone(),
        },
        ComposerTarget::Thread => LocalDraft {
            composer: shell.core.thread_composer.clone(),
            attachments: shell.core.thread_composer_attachments.clone(),
        },
    }
}
fn same_text(a: &LocalDraft, b: &LocalDraft, clearing: bool) -> bool {
    a.composer.text == b.composer.text
        && (!clearing || a.composer.reply_broadcast == b.composer.reply_broadcast)
}
fn same_book(a: Option<&LocalDraft>, b: Option<&LocalDraft>, clearing: bool) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => same_text(a, b, clearing),
        _ => false,
    }
}

fn apply_receipt(
    shell: &mut ShellState,
    source: &ComposeSource,
    before: &Before,
    receipt: ScheduledClearReceipt,
) {
    if !owns_account(source, shell) {
        return;
    }
    let replacement = match receipt {
        ScheduledClearReceipt::Cleared { scope } if scope == source.field_scope => None,
        ScheduledClearReceipt::Changed { scope, text }
            if scope == source.field_scope && text.len() <= 524288 =>
        {
            Some(text)
        }
        _ => {
            settle_attachments(shell, source);
            return; // Missing/error/IME is not permission to retire local text.
        }
    };
    let key = source.key();
    let clearing = replacement.is_none();
    let is_current = shell.composer_key(target(source)).as_ref() == Some(&key);
    if is_current {
        if !same_visit(source, shell)
            || !same_text(&live(shell, target(source)), &before.live, clearing)
            || !same_book(
                shell.core.drafts.items.get(&key),
                before.book.as_ref(),
                clearing,
            )
        {
            settle_attachments(shell, source);
            return;
        }
    } else if !shell
        .core
        .drafts
        .items
        .get(&key)
        .is_some_and(|draft| same_text(draft, &before.live, clearing))
    {
        // Navigation may capture the predecessor into its original key. A later
        // edit or deletion of that stored draft always wins over this receipt.
        settle_attachments(shell, source);
        return;
    }
    // Text ownership is separate from selection, attachment progress and reply
    // policy. Merge into each current copy so a receipt cannot roll back files
    // added while it was in flight or overwrite an independent stored policy.
    let settle = |mut local: LocalDraft| {
        if let Some(text) = &replacement {
            local.composer.text = text.clone();
            let end = local.composer.text.len();
            local.composer.set_selection(end, end);
        } else {
            local.composer = Default::default();
        }
        local.attachments.retain(|attachment| {
            !source
                .attachments
                .iter()
                .any(|(id, _, _)| *id == attachment.id)
        });
        local
    };
    let stored = shell
        .core
        .drafts
        .items
        .get(&key)
        .cloned()
        .unwrap_or_else(|| live(shell, target(source)));
    if is_current {
        let local = settle(live(shell, target(source)));
        match target(source) {
            ComposerTarget::Channel => {
                shell.core.composer = local.composer;
                shell.core.composer_attachments = local.attachments;
            }
            ComposerTarget::Thread => {
                shell.core.thread_composer = local.composer;
                shell.core.thread_composer_attachments = local.attachments;
            }
        }
    }
    let stored = settle(stored);
    shell
        .core
        .drafts
        .capture(&key, &stored.composer, &stored.attachments);
    let changes = shell.core.drafts.take_changes();
    crate::drafts::integration::route(shell, changes);
}

fn settle_attachments(shell: &mut ShellState, source: &ComposeSource) {
    if source.attachments.is_empty() || !owns_account(source, shell) {
        return;
    }
    let key = source.key();
    if shell.composer_key(target(source)).as_ref() == Some(&key) {
        if !same_visit(source, shell) {
            return;
        }
        let attachments = match target(source) {
            ComposerTarget::Channel => &mut shell.core.composer_attachments,
            ComposerTarget::Thread => &mut shell.core.thread_composer_attachments,
        };
        attachments.retain(|a| !source.attachments.iter().any(|(id, _, _)| *id == a.id));
    }
    // The stored value may have independently changed while the DOM receipt
    // was in flight. Retire only accepted file IDs in each copy, never replace
    // that book's newer text with the currently visible predecessor.
    if let Some(mut local) = shell.core.drafts.items.get(&key).cloned() {
        local
            .attachments
            .retain(|a| !source.attachments.iter().any(|(id, _, _)| *id == a.id));
        shell
            .core
            .drafts
            .capture(&key, &local.composer, &local.attachments);
    }
    let changes = shell.core.drafts.take_changes();
    crate::drafts::integration::route(shell, changes);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn same(a: &LocalDraft, b: &LocalDraft) -> bool {
        matches!((serde_json::to_value(a),serde_json::to_value(b)), (Ok(a),Ok(b)) if a==b)
    }
    fn setup() -> (ShellState, ComposeSource, Before) {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.sync_composer_drafts();
        shell.core.composer.text = "Scheduled original".into();
        shell.checkpoint_composers();
        let source = ComposeSource::capture(&shell, ComposerTarget::Channel).unwrap();
        let before = Before {
            live: live(&shell, ComposerTarget::Channel),
            book: shell.core.drafts.items.get(&source.key()).cloned(),
        };
        (shell, source, before)
    }
    fn changed(source: &ComposeSource) -> ScheduledClearReceipt {
        ScheduledClearReceipt::Changed {
            scope: source.field_scope.clone(),
            text: "Native text ahead of Rust".into(),
        }
    }
    fn attachment(id: u64) -> super_platinum_core::domain::ComposerAttachment {
        super_platinum_core::domain::ComposerAttachment {
            id,
            path: format!("fixture-{id}.txt").into(),
            name: format!("fixture-{id}.txt"),
            bytes: 1,
            uploading: false,
            upload_started: None,
            upload_cancel: None,
            upload_progress: None,
            preview_path: None,
        }
    }
    #[test]
    fn newer_native_text_survives_attachment_checkpoint_and_navigation() {
        for navigate_before_receipt in [false, true] {
            let (mut shell, _, _) = setup();
            let original = shell.active_channel;
            shell.core.composer_attachments.push(attachment(1));
            shell.checkpoint_composers();
            let source = ComposeSource::capture(&shell, ComposerTarget::Channel).unwrap();
            let before = Before {
                live: live(&shell, ComposerTarget::Channel),
                book: shell.core.drafts.items.get(&source.key()).cloned(),
            };
            // A file picker and selection event can checkpoint the old Rust
            // wire while the native field already contains newer text.
            shell.core.composer_attachments.push(attachment(2));
            shell.core.composer.set_selection(2, 4);
            shell.checkpoint_composers();
            let other = if original == 0 { 1 } else { 0 };
            if navigate_before_receipt {
                shell.select_channel(other, crate::state::ChannelOpen::Global);
            }
            apply_receipt(&mut shell, &source, &before, changed(&source));
            let stored = &shell.core.drafts.items[&source.key()];
            assert_eq!(stored.composer.text, "Native text ahead of Rust");
            assert_eq!(
                stored.attachments.iter().map(|a| a.id).collect::<Vec<_>>(),
                vec![2]
            );
            if !navigate_before_receipt {
                assert_eq!(shell.core.composer.text, stored.composer.text);
                shell.select_channel(other, crate::state::ChannelOpen::Global);
            }
            shell.select_channel(original, crate::state::ChannelOpen::Global);
            assert_eq!(shell.core.composer.text, "Native text ahead of Rust");
            assert_eq!(
                shell
                    .core
                    .composer_attachments
                    .iter()
                    .map(|a| a.id)
                    .collect::<Vec<_>>(),
                vec![2]
            );
        }
    }
    #[test]
    fn changed_merges_each_copys_files_selection_and_reply_policy() {
        let (mut shell, source, before) = setup();
        shell.core.composer_attachments.push(attachment(2));
        shell.core.composer.set_selection(1, 3);
        let stored = shell.core.drafts.items.get_mut(&source.key()).unwrap();
        stored.attachments.push(attachment(3));
        stored.composer.reply_broadcast = true;
        stored.composer.set_selection(4, 5);
        apply_receipt(&mut shell, &source, &before, changed(&source));
        assert_eq!(shell.core.composer.text, "Native text ahead of Rust");
        assert!(!shell.core.composer.reply_broadcast);
        assert_eq!(shell.core.composer_attachments[0].id, 2);
        let stored = &shell.core.drafts.items[&source.key()];
        assert_eq!(stored.composer.text, "Native text ahead of Rust");
        assert!(stored.composer.reply_broadcast);
        assert_eq!(stored.attachments[0].id, 3);
    }
    #[test]
    fn selection_only_event_keeps_changed_receipt_valid() {
        let (mut shell, source, before) = setup();
        shell.core.composer.set_selection(1, 3);
        shell.checkpoint_composers();
        apply_receipt(&mut shell, &source, &before, changed(&source));
        assert_eq!(shell.core.composer.text, "Native text ahead of Rust");
        assert_eq!(
            shell.core.drafts.items[&source.key()].composer.text,
            shell.core.composer.text
        );
    }
    #[test]
    fn cleared_receipt_cannot_erase_a_revised_reply_policy() {
        for book_only in [false, true] {
            let (mut shell, source, before) = setup();
            if book_only {
                shell
                    .core
                    .drafts
                    .items
                    .get_mut(&source.key())
                    .unwrap()
                    .composer
                    .reply_broadcast = true;
            } else {
                shell.core.composer.reply_broadcast = true;
            }
            let live_before = live(&shell, ComposerTarget::Channel);
            let book_before = shell.core.drafts.items[&source.key()].clone();
            apply_receipt(
                &mut shell,
                &source,
                &before,
                ScheduledClearReceipt::Cleared {
                    scope: source.field_scope.clone(),
                },
            );
            assert!(same(&live_before, &live(&shell, ComposerTarget::Channel)));
            assert!(same(&book_before, &shell.core.drafts.items[&source.key()]));
        }
    }
    #[test]
    fn receipt_cannot_settle_a_reopened_channel_with_the_same_text() {
        for clearing in [false, true] {
            let (mut shell, source, before) = setup();
            let original = shell.active_channel;
            let other = if original == 0 { 1 } else { 0 };
            shell.select_channel(other, crate::state::ChannelOpen::Global);
            shell.select_channel(original, crate::state::ChannelOpen::Global);
            assert_ne!(
                field_scope(&shell, ComposerTarget::Channel),
                source.field_scope
            );
            let receipt = if clearing {
                ScheduledClearReceipt::Cleared {
                    scope: source.field_scope.clone(),
                }
            } else {
                changed(&source)
            };
            apply_receipt(&mut shell, &source, &before, receipt);
            assert!(same(&before.live, &live(&shell, ComposerTarget::Channel)));
            assert!(same(&before.live, &shell.core.drafts.items[&source.key()]));
        }
    }
    #[test]
    fn changed_native_receipt_checkpoints_without_another_input_event() {
        let (mut shell, source, before) = setup();
        apply_receipt(&mut shell, &source, &before, changed(&source));
        assert_eq!(shell.core.composer.text, "Native text ahead of Rust");
        assert_eq!(
            shell.core.drafts.items[&source.key()].composer.text,
            "Native text ahead of Rust"
        );
    }
    #[test]
    fn receipt_after_navigation_updates_only_the_unchanged_original_book() {
        let (mut shell, source, before) = setup();
        let other = if shell.active_channel == 0 { 1 } else { 0 };
        shell.select_channel(other, crate::state::ChannelOpen::Global);
        shell.core.composer.text = "Replacement channel".into();
        shell.checkpoint_composers();
        apply_receipt(&mut shell, &source, &before, changed(&source));
        assert_eq!(shell.core.composer.text, "Replacement channel");
        assert_eq!(
            shell.core.drafts.items[&source.key()].composer.text,
            "Native text ahead of Rust"
        );
    }
    #[test]
    fn later_rust_edit_book_edit_account_or_visit_always_wins() {
        for mutation in 0..4 {
            let (mut shell, source, before) = setup();
            match mutation {
                0 => shell.core.composer.text = "Newer Rust text".into(),
                1 => {
                    shell
                        .core
                        .drafts
                        .items
                        .get_mut(&source.key())
                        .unwrap()
                        .composer
                        .text = "Newer book text".into()
                }
                2 => shell.core.active_account = Some("replacement".into()),
                _ => shell.draft_writes.view_id += 1,
            }
            let live_before = live(&shell, ComposerTarget::Channel);
            let book_before = shell.core.drafts.items[&source.key()].clone();
            apply_receipt(&mut shell, &source, &before, changed(&source));
            assert!(same(&live_before, &live(&shell, ComposerTarget::Channel)));
            assert!(same(&book_before, &shell.core.drafts.items[&source.key()]));
        }
    }
    #[test]
    fn ime_missing_and_foreign_receipts_preserve_live_and_book() {
        for receipt in [
            ScheduledClearReceipt::Composing,
            ScheduledClearReceipt::Unavailable,
            ScheduledClearReceipt::Cleared {
                scope: "foreign".into(),
            },
        ] {
            let (mut shell, source, before) = setup();
            apply_receipt(&mut shell, &source, &before, receipt);
            assert!(same(&before.live, &live(&shell, ComposerTarget::Channel)));
            assert!(same(&before.live, &shell.core.drafts.items[&source.key()]));
        }
    }
    #[test]
    fn changed_book_text_and_broadcast_survive_accepted_attachment_retirement() {
        let (mut shell, _, _) = setup();
        let attachment = |id| super_platinum_core::domain::ComposerAttachment {
            id,
            path: format!("fixture-{id}.txt").into(),
            name: format!("fixture-{id}.txt"),
            bytes: 1,
            uploading: false,
            upload_started: None,
            upload_cancel: None,
            upload_progress: None,
            preview_path: None,
        };
        shell.core.composer_attachments.push(attachment(1));
        shell.checkpoint_composers();
        let source = ComposeSource::capture(&shell, ComposerTarget::Channel).unwrap();
        let before = Before {
            live: live(&shell, ComposerTarget::Channel),
            book: shell.core.drafts.items.get(&source.key()).cloned(),
        };
        shell.core.composer_attachments.push(attachment(2));
        let stored = shell.core.drafts.items.get_mut(&source.key()).unwrap();
        stored.composer.text = "Independent stored edit".into();
        stored.composer.reply_broadcast = true;
        stored.attachments.push(attachment(2));
        apply_receipt(&mut shell, &source, &before, changed(&source));
        assert_eq!(shell.core.composer.text, "Scheduled original");
        assert_eq!(
            shell
                .core
                .composer_attachments
                .iter()
                .map(|a| a.id)
                .collect::<Vec<_>>(),
            vec![2]
        );
        let stored = &shell.core.drafts.items[&source.key()];
        assert_eq!(stored.composer.text, "Independent stored edit");
        assert!(stored.composer.reply_broadcast);
        assert_eq!(
            stored.attachments.iter().map(|a| a.id).collect::<Vec<_>>(),
            vec![2]
        );
    }
    #[test]
    fn changed_receipt_rejects_reopened_thread_and_preserves_offscreen_policy() {
        let (mut shell, _, _) = setup();
        shell.begin_thread("100.000001".into(), None);
        shell.core.thread_composer.text = "Original thread".into();
        shell.checkpoint_composers();
        let source = ComposeSource::capture(&shell, ComposerTarget::Thread).unwrap();
        let before = Before {
            live: live(&shell, ComposerTarget::Thread),
            book: shell.core.drafts.items.get(&source.key()).cloned(),
        };
        shell.close_thread();
        shell.begin_thread("100.000001".into(), None);
        apply_receipt(&mut shell, &source, &before, changed(&source));
        assert_eq!(shell.core.thread_composer.text, "Original thread");
        shell.close_thread();
        let stored = shell.core.drafts.items.get_mut(&source.key()).unwrap();
        stored.composer.reply_broadcast = true;
        apply_receipt(&mut shell, &source, &before, changed(&source));
        let stored = &shell.core.drafts.items[&source.key()];
        assert_eq!(stored.composer.text, "Native text ahead of Rust");
        assert!(stored.composer.reply_broadcast);
    }
}
