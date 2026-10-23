use super::*;
use crate::canvas::text::structure_fixture;
use super_platinum_core::canvas::edit::{
    structure::{Side, StructureIntent},
    text::TextDraft,
};

struct Store(std::path::PathBuf);
impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn store() -> Store {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    for _ in 0..256 {
        let n = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .unwrap();
        let path = std::env::temp_dir().join(format!(
            "canvas-review-ui-{}-{n}.sqlite",
            std::process::id()
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => {
                drop(file);
                return Store(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => panic!("isolated journal: {e}"),
        }
    }
    panic!("too many occupied isolated test journal paths");
}
fn restored() -> (ShellState, Store) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some("00000000-0000-4000-8000-000000000995".into());
    shell.self_account.user_id = "U1".into();
    shell.core.workspaces.get_mut("T1").unwrap().self_user_id = "U1".into();
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Synthetic".into(),
        permalink: None,
    });
    shell.overlay = Some(crate::state::Overlay::Canvas);
    open_visit(&mut shell);
    let snapshot = structure_fixture::snapshot();
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(snapshot.clone())));
    let owner = Owner::capture(&shell, "Ffixture").unwrap();
    let store = store();
    // Explicit injected path only. Tests never execute Owner::run/open_default.
    let mut journal = Journal::open(&store.0, &owner.account).unwrap();
    let prepared = StructureIntent::insert(
        &snapshot,
        "paragraph",
        Side::After,
        &TextDraft::from_rtml("Unconfirmed").unwrap(),
        100,
    )
    .unwrap()
    .prepare(&snapshot, 100)
    .unwrap();
    let receipt = match journal.begin(prepared.checkpoint().unwrap()).unwrap() {
        Begin::Started(r) => r,
        _ => panic!("fresh"),
    };
    shell.canvas_checklist.recovery = State {
        generation: 77,
        owner: Some(owner),
        loaded: true,
        error: false,
        receipt: Some(receipt),
    };
    (shell, store)
}

#[test]
fn restored_review_requires_fresh_owner_inventory_and_no_in_memory_operation() {
    let (mut shell, _store) = restored();
    let request = Request::capture(&shell).unwrap();
    assert!(request.current(&shell, true));
    shell.canvas_checklist.view.as_mut().unwrap().snapshot =
        Some(Arc::new((*request.snapshot).clone()));
    assert!(
        !request.current(&shell, true),
        "equal bytes do not replace exact read ownership"
    );
    shell.canvas_checklist.view.as_mut().unwrap().snapshot = Some(request.snapshot.clone());
    shell.core.account_epoch += 1;
    assert!(Request::capture(&shell).is_none());
    shell.core.account_epoch -= 1;
    let slot = std::mem::take(&mut shell.canvas_checklist.recovery);
    let view = presentation(&shell).unwrap();
    let _ = structure_ops::begin_structure(
        &mut shell,
        view.visit,
        view.read,
        "paragraph",
        structure_ops::Action::After,
        &TextDraft::from_rtml("Other").unwrap(),
        100,
    )
    .unwrap();
    shell.canvas_checklist.recovery = slot;
    assert!(
        Request::capture(&shell).is_none(),
        "no local decision may settle an in-memory request"
    );
}

#[test]
fn local_removal_reserves_generation_and_failed_storage_keeps_record() {
    let (mut shell, _store) = restored();
    let request = Request::capture(&shell).unwrap();
    let pending = begin_local_removal(&mut shell, &request).unwrap();
    assert!(begin_local_removal(&mut shell, &request).is_none());
    assert!(!shell.canvas_checklist.recovery.loaded);
    assert!(finish_local_removal(&mut shell, &pending, false));
    assert!(shell.canvas_checklist.recovery.error);
    assert!(shell.canvas_checklist.recovery.receipt.as_ref() == Some(&request.receipt));
    assert!(Request::capture(&shell).is_none());
}

#[test]
fn old_local_completion_cannot_erase_a_new_read_or_owner() {
    let (mut shell, _store) = restored();
    let request = Request::capture(&shell).unwrap();
    let pending = begin_local_removal(&mut shell, &request).unwrap();
    let newer = begin_load(&mut shell, &request.owner);
    assert_ne!(newer, pending.generation);
    assert!(!finish_local_removal(&mut shell, &pending, true));
    assert_eq!(shell.canvas_checklist.recovery.generation, newer);
    assert!(!shell.canvas_checklist.recovery.loaded);
    shell.core.active_account = Some("00000000-0000-4000-8000-000000000996".into());
    assert!(!finish_local_removal(&mut shell, &pending, true));
}

#[test]
fn explicit_local_archive_commits_before_unlock_and_requires_new_canonical_read() {
    let (mut shell, store) = restored();
    let request = Request::capture(&shell).unwrap();
    let mut journal = Journal::open(&store.0, &request.owner.account).unwrap();
    let review = journal
        .local_review(&request.receipt, &request.snapshot)
        .unwrap();
    let pending = begin_local_removal(&mut shell, &request).unwrap();
    assert!(shell.canvas_checklist.recovery.receipt.is_some());
    assert!(
        journal
            .remove_local_lock(
                &review,
                &request.snapshot,
                LocalDecision::RemoveEditLockOutcomeUnknown,
                123
            )
            .unwrap()
    );
    assert!(journal.get(&request.owner.scope).unwrap().is_none());
    assert!(finish_local_removal(&mut shell, &pending, true));
    assert!(shell.canvas_checklist.recovery.receipt.is_none());
    assert!(shell.canvas_checklist.view.as_ref().unwrap().stale);
    assert!(shell.canvas_checklist.operations.is_empty());
    let view = presentation(&shell).unwrap();
    assert!(begin(&mut shell, view.visit, view.read, "paragraph", true).is_err());
}

#[test]
fn held_review_open_cannot_adopt_another_file_or_replacement_visit() {
    let (mut shell, _store) = restored();
    let original = Request::capture(&shell).unwrap();
    let held = begin_read(&mut shell).unwrap();
    assert!(finish_read(
        &mut shell,
        &held,
        Ok(original.snapshot.clone())
    ));
    assert!(original.after_read(&shell, &held).is_some());
    // A newer independently completed read is never our held read.
    let newer = begin_read(&mut shell).unwrap();
    assert!(finish_read(
        &mut shell,
        &newer,
        Ok(original.snapshot.clone())
    ));
    assert!(original.after_read(&shell, &held).is_none());
    // Closing/reopening the same file changes visit authority even if the exact
    // receipt and canonical bytes are restored into the replacement view.
    shell.shortcut_generation = shell.shortcut_generation.wrapping_add(1);
    open_visit(&mut shell);
    let replacement = begin_read(&mut shell).unwrap();
    assert!(finish_read(
        &mut shell,
        &replacement,
        Ok(original.snapshot.clone())
    ));
    assert!(Request::capture(&shell).is_some());
    assert!(original.after_read(&shell, &replacement).is_none());
    assert!(original.after_read(&shell, &held).is_none());
    shell.canvas.as_mut().unwrap().id = "other-file".into();
    shell.shortcut_generation = shell.shortcut_generation.wrapping_add(1);
    open_visit(&mut shell);
    let other = begin_read(&mut shell).unwrap();
    assert!(finish_read(
        &mut shell,
        &other,
        Ok(original.snapshot.clone())
    ));
    assert!(original.after_read(&shell, &other).is_none());
    shell.core.active_account = Some("00000000-0000-4000-8000-000000000996".into());
    assert!(original.after_read(&shell, &held).is_none());
}

fn restored_split() -> (ShellState, Store) {
    use super_platinum_core::canvas::edit::structure::{SplitIntent, SplitWitness};
    let (mut shell, _previous_store) = restored();
    let snapshot = crate::canvas::text::split_recovery_fixture::snapshot();
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(snapshot.clone())));
    let intent = SplitIntent::capture(&snapshot, "paragraph", 100).unwrap();
    let witness = SplitWitness::new(
        intent.seed(),
        9,
        &TextDraft::from_rtml("Keep this").unwrap(),
        &TextDraft::from_rtml("Second paragraph").unwrap(),
    )
    .unwrap();
    let checkpoint = intent
        .prepare(&snapshot, &witness, 100)
        .unwrap()
        .checkpoint()
        .unwrap();
    let store = store();
    let owner = shell.canvas_checklist.recovery.owner.clone().unwrap();
    let mut journal = Journal::open(&store.0, &owner.account).unwrap();
    let Begin::Started(receipt) = journal.begin(checkpoint).unwrap() else {
        panic!("fresh split")
    };
    shell.canvas_checklist.recovery.receipt = Some(receipt);
    (shell, store)
}

#[test]
fn split_review_keeps_entire_pair_through_stale_read_account_change_and_failed_storage() {
    let (mut shell, store) = restored_split();
    let request = Request::capture(&shell).expect("restored split review available");
    let journal = Journal::open(&store.0, &request.owner.account).unwrap();
    let review = journal
        .local_review(&request.receipt, &request.snapshot)
        .unwrap();
    let pair = review.split().unwrap();
    assert_eq!(pair.original_text(), "Keep thisSecond paragraph");
    assert_eq!(
        (pair.parent_text(), pair.child_text()),
        ("Keep this", "Second paragraph")
    );
    assert_eq!(review.observed_sections().count(), 3);
    shell.canvas_checklist.view.as_mut().unwrap().snapshot =
        Some(Arc::new((*request.snapshot).clone()));
    assert!(begin_local_removal(&mut shell, &request).is_none());
    shell.canvas_checklist.view.as_mut().unwrap().snapshot = Some(request.snapshot.clone());
    let original_account = shell.core.active_account.clone();
    shell.core.active_account = Some("00000000-0000-4000-8000-000000000996".into());
    assert!(begin_local_removal(&mut shell, &request).is_none());
    shell.core.active_account = original_account;
    let pending = begin_local_removal(&mut shell, &request).unwrap();
    assert!(finish_local_removal(&mut shell, &pending, false));
    assert!(shell.canvas_checklist.recovery.error);
    assert!(shell.canvas_checklist.recovery.receipt.as_ref() == Some(&request.receipt));
    assert!(journal.get(&request.owner.scope).unwrap().as_ref() == Some(&request.receipt));
    assert!(!journal.has_local_review(&request.receipt).unwrap());
    assert!(shell.canvas_checklist.operations.is_empty());
}

#[test]
fn split_local_archive_unlocks_only_after_exact_pair_commit_and_requires_refresh() {
    let (mut shell, store) = restored_split();
    let request = Request::capture(&shell).unwrap();
    let mut journal = Journal::open(&store.0, &request.owner.account).unwrap();
    let review = journal
        .local_review(&request.receipt, &request.snapshot)
        .unwrap();
    let pending = begin_local_removal(&mut shell, &request).unwrap();
    assert!(shell.canvas_checklist.recovery.receipt.is_some());
    assert!(
        journal
            .remove_local_lock(
                &review,
                &request.snapshot,
                LocalDecision::RemoveEditLockOutcomeUnknown,
                123
            )
            .unwrap()
    );
    assert!(journal.has_local_review(&request.receipt).unwrap());
    assert!(journal.get(&request.owner.scope).unwrap().is_none());
    assert!(finish_local_removal(&mut shell, &pending, true));
    assert!(shell.canvas_checklist.recovery.receipt.is_none());
    assert!(shell.canvas_checklist.view.as_ref().unwrap().stale);
    assert!(shell.canvas_checklist.operations.is_empty());
}
