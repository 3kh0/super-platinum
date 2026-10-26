mod end_readback;
mod split_readback;
use super::*;
use crate::canvas::text::structure_fixture;
use super_platinum_core::canvas::edit::text::TextDraft;

pub(super) struct Store(pub(super) std::path::PathBuf);
impl Store {
    fn reserve(path: std::path::PathBuf) -> std::io::Result<Self> {
        // Own the name atomically before Journal::open can create/reopen it.
        // Failed reservation must never return a guard that removes that path.
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        drop(file);
        Ok(Self(path))
    }
    pub(super) fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..256 {
            let serial = NEXT
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value.checked_add(1)
                })
                .expect("test journal counter exhausted");
            let path = std::env::temp_dir().join(format!(
                "canvas-desktop-test-{}-{serial}.sqlite",
                std::process::id()
            ));
            match Self::reserve(path) {
                Ok(store) => return store,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("cannot reserve isolated test journal: {error}"),
            }
        }
        panic!("too many occupied test journal paths");
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
/// Manual checklist reducer fixtures must model the journal read completed by
/// the async loader before editing. Query a fresh isolated database for the
/// captured real owner; never access default config paths or mutate fixture env.
/// These checkbox-only fixtures never create structural journal receipts.
pub(in crate::canvas::checklist) fn load_empty_isolated(shell: &mut ShellState, read: &Read) {
    if bypass(shell) {
        return;
    }
    assert!(read.current(shell));
    let owner = Owner::capture(shell, &read.file).expect("synthetic account owner");
    let store = Store::new();
    let generation = begin_load(shell, &owner);
    let journal = Journal::open(&store.0, &owner.account).unwrap();
    let result = journal.get(&owner.scope).unwrap();
    assert!(result.is_none());
    publish_load(shell, read, owner, generation, Ok(result));
    assert_eq!(reason(shell), None);
    drop(journal);
}
fn setup() -> (ShellState, Attempt, PreparedStructureEdit, Owner, Store) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Test".into(),
        permalink: None,
    });
    shell.overlay = Some(Overlay::Canvas);
    open_visit(&mut shell);
    let snapshot = structure_fixture::snapshot();
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(snapshot.clone())));
    let view = presentation(&shell).unwrap();
    let (attempt, intent) = structure_ops::begin_structure(
        &mut shell,
        view.visit,
        view.read,
        "paragraph",
        structure_ops::Action::Delete,
        &TextDraft::default(),
        100,
    )
    .unwrap();
    let prepared = intent.prepare(&snapshot, 100).unwrap();
    let store = Store::new();
    let owner = Owner {
        account: "d1b586dc-c6d3-4615-8316-a60de869a5e8".into(),
        scope: prepared.checkpoint().unwrap().scope().clone(),
        path: Some(store.0.clone()),
    };
    (shell, attempt, prepared, owner, store)
}
fn reserve_sync(owner: &Owner, prepared: &PreparedStructureEdit) -> Ticket {
    let mut journal = Journal::open(owner.path.as_ref().unwrap(), &owner.account).unwrap();
    let Begin::Started(receipt) = journal.begin(prepared.checkpoint().unwrap()).unwrap() else {
        panic!("duplicate")
    };
    Ticket {
        owner: owner.clone(),
        receipt: Arc::new(receipt),
    }
}
#[test]
fn disk_read_loading_error_owner_and_unresolved_each_block_shared_edit_gate() {
    let (_, _, prepared, owner, _store) = setup();
    let ticket = reserve_sync(&owner, &prepared);
    let mut state = State::default();
    assert_eq!(state.reason_for(Some(owner.clone())), Some(WAIT));
    state.owner = Some(owner.clone());
    state.loaded = true;
    assert_eq!(state.reason_for(Some(owner.clone())), None);
    state.error = true;
    assert_eq!(state.reason_for(Some(owner.clone())), Some(FAILED));
    state.error = false;
    state.receipt = Some((*ticket.receipt).clone());
    assert_eq!(state.reason_for(Some(owner.clone())), Some(UNRESOLVED));
    let mut other = owner.clone();
    other.account = "other".into();
    assert_eq!(state.reason_for(Some(other)), Some(WAIT));
}
#[test]
fn sealed_readback_cannot_unlock_until_exact_receipt_removal_commits() {
    let (mut shell, attempt, prepared, owner, _store) = setup();
    let ticket = reserve_sync(&owner, &prepared);
    shell.canvas_checklist.operations[0].durable = Some(ticket.clone());
    assert!(structure_ops::mark_dispatched(
        &mut shell, &attempt, &prepared
    ));
    let (accepted, current) = structure_fixture::accepted(prepared, &attempt.snapshot, 100);
    assert!(structure_ops::finish_structure(
        &mut shell, &attempt, accepted
    ));
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(current)));
    assert!(!confirm_readback(&mut shell, &attempt, &read));
    assert_eq!(
        shell.canvas_checklist.operations[0].durable_verified,
        Some(read.serial)
    );
    let mut journal = Journal::open(owner.path.as_ref().unwrap(), &owner.account).unwrap();
    assert!(journal.complete(&ticket.receipt).unwrap());
    shell.canvas_checklist.operations[0].durable = None;
    assert!(confirm_readback(&mut shell, &attempt, &read));
    assert!(shell.canvas_checklist.operations.is_empty());
}
#[test]
fn uncertain_absence_cannot_request_durable_settlement() {
    let (mut shell, attempt, prepared, owner, _store) = setup();
    let ticket = reserve_sync(&owner, &prepared);
    shell.canvas_checklist.operations[0].durable = Some(ticket.clone());
    assert!(structure_ops::mark_dispatched(
        &mut shell, &attempt, &prepared
    ));
    assert!(finish(&mut shell, &attempt, Outcome::Uncertain));
    let mut current = (*attempt.snapshot).clone();
    current.sections.retain(|s| s.id() != attempt.section);
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(Arc::new(current))));
    assert!(!confirm_readback(&mut shell, &attempt, &read));
    assert_eq!(shell.canvas_checklist.operations[0].durable_verified, None);
    let journal = Journal::open(owner.path.as_ref().unwrap(), &owner.account).unwrap();
    assert!(journal.get(&owner.scope).unwrap().is_some());
}
#[test]
fn invalidation_after_durable_reservation_still_refuses_dispatch() {
    let (mut shell, attempt, prepared, owner, _store) = setup();
    let ticket = reserve_sync(&owner, &prepared);
    shell.canvas_checklist.operations[0].durable = Some(ticket.clone());
    assert!(invalidate(
        &mut shell,
        &owner.scope.workspace,
        &owner.scope.file
    ));
    assert!(!structure_ops::mark_dispatched(
        &mut shell, &attempt, &prepared
    ));
    assert!(cancel_preflight(&mut shell, &attempt));
    // Cancelling renderer state alone cannot delete the durable row.
    let mut journal = Journal::open(owner.path.as_ref().unwrap(), &owner.account).unwrap();
    assert!(journal.get(&owner.scope).unwrap().is_some());
    assert!(journal.complete(&ticket.receipt).unwrap());
}

fn opened_b_after_reserving_a() -> (ShellState, Owner, Ticket, Owner, Store) {
    let (mut shell, _, prepared, owner_a, store) = setup();
    let ticket = reserve_sync(&owner_a, &prepared);
    shell.canvas.as_mut().unwrap().id = "F_B".into();
    open_visit(&mut shell);
    let mut owner_b = owner_a.clone();
    owner_b.scope.file = "F_B".into();
    shell.canvas_checklist.recovery = State {
        generation: 0,
        owner: Some(owner_b.clone()),
        loaded: true,
        error: false,
        receipt: None,
    };
    (shell, owner_a, ticket, owner_b, store)
}
#[test]
fn late_a_reservation_success_preserves_loaded_b_slot_and_original_a_ticket() {
    let (mut shell, owner_a, ticket, owner_b, _store) = opened_b_after_reserving_a();
    publish_reservation(&mut shell, &owner_a, Ok((*ticket.receipt).clone()));
    assert_eq!(
        shell.canvas_checklist.recovery.reason_for(Some(owner_b)),
        None
    );
    let mut journal = Journal::open(owner_a.path.as_ref().unwrap(), &owner_a.account).unwrap();
    assert!(journal.get(&owner_a.scope).unwrap().is_some());
    assert!(journal.complete(&ticket.receipt).unwrap());
}
#[test]
fn late_a_reservation_error_cannot_mark_loaded_b_slot_failed() {
    let (mut shell, owner_a, _ticket, owner_b, _store) = opened_b_after_reserving_a();
    publish_reservation(&mut shell, &owner_a, Err(()));
    assert_eq!(
        shell.canvas_checklist.recovery.reason_for(Some(owner_b)),
        None
    );
    assert!(!shell.canvas_checklist.recovery.error);
}

fn retirement_in_flight() -> (
    ShellState,
    Attempt,
    Arc<CanvasCollabSnapshot>,
    Ticket,
    Store,
) {
    let (mut shell, attempt, prepared, owner, store) = setup();
    let ticket = reserve_sync(&owner, &prepared);
    shell.canvas_checklist.operations[0].durable = Some(ticket.clone());
    assert!(structure_ops::mark_dispatched(
        &mut shell, &attempt, &prepared
    ));
    let (accepted, current) = structure_fixture::accepted(prepared, &attempt.snapshot, 100);
    assert!(structure_ops::finish_structure(
        &mut shell, &attempt, accepted
    ));
    let read = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(current.clone())));
    assert!(!confirm_readback(&mut shell, &attempt, &read));
    shell.canvas_checklist.operations[0].durable_finishing = true;
    (shell, attempt, current, ticket, store)
}
#[test]
fn latest_verified_read_settles_when_older_read_retirement_finishes() {
    let (mut shell, attempt, current, ticket, _store) = retirement_in_flight();
    let newer = begin_read(&mut shell).unwrap();
    assert!(finish_read(&mut shell, &newer, Ok(current)));
    assert!(!confirm_readback(&mut shell, &attempt, &newer));
    assert_eq!(
        shell.canvas_checklist.operations[0].durable_verified,
        Some(newer.serial)
    );
    let mut journal =
        Journal::open(ticket.owner.path.as_ref().unwrap(), &ticket.owner.account).unwrap();
    assert!(journal.complete(&ticket.receipt).unwrap());
    finish_retirement(&mut shell, &attempt, true);
    assert!(shell.canvas_checklist.operations.is_empty());
    assert_eq!(
        shell.canvas_checklist.view.as_ref().unwrap().settled_text,
        Some(attempt.serial)
    );
}
#[test]
fn retirement_completion_cannot_adopt_an_unverified_newer_read() {
    let (mut shell, attempt, _current, ticket, _store) = retirement_in_flight();
    let newer = begin_read(&mut shell).unwrap();
    // Original inventory is not an accepted deletion readback.
    assert!(finish_read(
        &mut shell,
        &newer,
        Ok(attempt.snapshot.clone())
    ));
    assert!(!confirm_readback(&mut shell, &attempt, &newer));
    let mut journal =
        Journal::open(ticket.owner.path.as_ref().unwrap(), &ticket.owner.account).unwrap();
    assert!(journal.complete(&ticket.receipt).unwrap());
    finish_retirement(&mut shell, &attempt, true);
    assert_eq!(shell.canvas_checklist.operations.len(), 1);
    assert_eq!(
        shell.canvas_checklist.view.as_ref().unwrap().settled_text,
        None
    );
}

#[test]
fn held_empty_load_cannot_clear_later_reservation_after_cleanup_failure() {
    let (mut shell, old_attempt, prepared, owner, _store) = setup();
    open_visit(&mut shell); // Cancels only the old unsent renderer attempt.
    let read = begin_read(&mut shell).unwrap();
    let held = begin_load(&mut shell, &owner); // Its DB query observed None.
    let ticket = reserve_sync(&owner, &prepared); // Begin commits after that query.
    publish_reservation(&mut shell, &owner, Ok((*ticket.receipt).clone()));
    assert!(!cancel_preflight(&mut shell, &old_attempt));
    // Cleanup did not commit. The earlier empty result must not erase the lock.
    publish_load(&mut shell, &read, owner.clone(), held, Ok(None));
    assert_eq!(
        shell
            .canvas_checklist
            .recovery
            .reason_for(Some(owner.clone())),
        Some(UNRESOLVED)
    );
    assert!(
        Journal::open(owner.path.as_ref().unwrap(), &owner.account)
            .unwrap()
            .get(&owner.scope)
            .unwrap()
            .is_some()
    );
}
#[test]
fn held_empty_load_cannot_clear_reservation_error() {
    let (mut shell, _, _, owner, _store) = setup();
    open_visit(&mut shell);
    let read = begin_read(&mut shell).unwrap();
    let held = begin_load(&mut shell, &owner);
    publish_reservation(&mut shell, &owner, Err(()));
    publish_load(&mut shell, &read, owner.clone(), held, Ok(None));
    assert_eq!(
        shell.canvas_checklist.recovery.reason_for(Some(owner)),
        Some(FAILED)
    );
}
#[test]
fn successful_retirement_fences_old_receipt_load_and_keeps_slot_loaded() {
    let (mut shell, _, prepared, owner, _store) = setup();
    open_visit(&mut shell);
    let read = begin_read(&mut shell).unwrap();
    let held = begin_load(&mut shell, &owner);
    let ticket = reserve_sync(&owner, &prepared);
    let fence = retirement_fence(&shell, &ticket);
    let mut journal = Journal::open(owner.path.as_ref().unwrap(), &owner.account).unwrap();
    assert!(journal.complete(&ticket.receipt).unwrap());
    publish_retirement(&mut shell, &ticket, fence);
    publish_load(
        &mut shell,
        &read,
        owner.clone(),
        held,
        Ok(Some((*ticket.receipt).clone())),
    );
    assert_eq!(
        shell.canvas_checklist.recovery.reason_for(Some(owner)),
        None
    );
}
#[test]
fn load_generation_is_not_reused_by_a_b_a_slot_replacement() {
    let (mut shell, _, _, owner, _store) = setup();
    open_visit(&mut shell);
    let read = begin_read(&mut shell).unwrap();
    let old_a = begin_load(&mut shell, &owner);
    let mut other = owner.clone();
    other.scope.file = "F_OTHER".into();
    begin_load(&mut shell, &other);
    let new_a = begin_load(&mut shell, &owner);
    assert_ne!(old_a, new_a);
    publish_load(&mut shell, &read, owner.clone(), old_a, Ok(None));
    assert_eq!(
        shell
            .canvas_checklist
            .recovery
            .reason_for(Some(owner.clone())),
        Some(WAIT)
    );
    publish_load(&mut shell, &read, owner.clone(), new_a, Ok(None));
    assert_eq!(
        shell.canvas_checklist.recovery.reason_for(Some(owner)),
        None
    );
}

#[test]
fn old_ticket_retirement_cannot_clear_a_newer_reservation_error() {
    for retain_old_receipt in [false, true] {
        let (mut shell, _, prepared, owner, _store) = setup();
        open_visit(&mut shell);
        begin_load(&mut shell, &owner);
        let ticket = reserve_sync(&owner, &prepared);
        if retain_old_receipt {
            publish_reservation(&mut shell, &owner, Ok((*ticket.receipt).clone()));
        }
        let fence = retirement_fence(&shell, &ticket);
        let mut journal = Journal::open(owner.path.as_ref().unwrap(), &owner.account).unwrap();
        assert!(journal.complete(&ticket.receipt).unwrap());
        // Disk removal completed, but its UI continuation has not run. A newer
        // begin may have an uncertain commit outcome; its error must survive.
        publish_reservation(&mut shell, &owner, Err(()));
        let generation = shell.canvas_checklist.recovery.generation;
        publish_retirement(&mut shell, &ticket, fence);
        assert_eq!(shell.canvas_checklist.recovery.generation, generation);
        assert_eq!(
            shell.canvas_checklist.recovery.reason_for(Some(owner)),
            Some(FAILED)
        );
    }
}

#[test]
fn old_retirement_cannot_hide_a_new_reservation_behind_a_newer_loading_slot() {
    let (mut shell, _, prepared, owner, _store) = setup();
    open_visit(&mut shell);
    let read = begin_read(&mut shell).unwrap();
    begin_load(&mut shell, &owner);
    let old = reserve_sync(&owner, &prepared);
    publish_reservation(&mut shell, &owner, Ok((*old.receipt).clone()));
    let fence = retirement_fence(&shell, &old);
    let mut journal = Journal::open(owner.path.as_ref().unwrap(), &owner.account).unwrap();
    assert!(journal.complete(&old.receipt).unwrap());
    // Old delete committed; a later independently reserved receipt and a later
    // load now own the slot before the old worker's UI continuation resumes.
    let replacement = reserve_sync(&owner, &prepared);
    publish_reservation(&mut shell, &owner, Ok((*replacement.receipt).clone()));
    let loading = begin_load(&mut shell, &owner);
    publish_retirement(&mut shell, &old, fence);
    assert_eq!(shell.canvas_checklist.recovery.generation, loading);
    assert_eq!(
        shell
            .canvas_checklist
            .recovery
            .reason_for(Some(owner.clone())),
        Some(WAIT)
    );
    publish_load(
        &mut shell,
        &read,
        owner.clone(),
        loading,
        Ok(Some((*replacement.receipt).clone())),
    );
    assert_eq!(
        shell.canvas_checklist.recovery.reason_for(Some(owner)),
        Some(UNRESOLVED)
    );
}

#[test]
fn occupied_store_path_is_never_adopted_or_removed() {
    let store = Store::new();
    std::fs::write(&store.0, b"retained existing owner").unwrap();
    let error = match Store::reserve(store.0.clone()) {
        Ok(_) => panic!("occupied path was adopted"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(&store.0).unwrap(), b"retained existing owner");
}

#[test]
fn parallel_store_reservations_keep_identical_receipt_scopes_isolated() {
    let (_, _, prepared, owner, _store) = setup();
    let checkpoint = prepared.checkpoint().unwrap();
    let stores = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let checkpoint = checkpoint.clone();
                let account = owner.account.clone();
                scope.spawn(move || {
                    let store = Store::new();
                    let mut journal = Journal::open(&store.0, &account).unwrap();
                    assert!(journal.get(checkpoint.scope()).unwrap().is_none());
                    assert!(matches!(
                        journal.begin(checkpoint).unwrap(),
                        Begin::Started(_)
                    ));
                    drop(journal);
                    store
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    let paths: std::collections::BTreeSet<_> = stores.iter().map(|store| &store.0).collect();
    assert_eq!(paths.len(), stores.len());
    for store in &stores {
        assert!(
            Journal::open(&store.0, &owner.account)
                .unwrap()
                .get(&owner.scope)
                .unwrap()
                .is_some()
        );
    }
}

mod replacement;
