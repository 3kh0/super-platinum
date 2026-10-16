use super::super::super::{LocalDraft, operation::tests::Fixture};
use super::*;
use super_platinum_core::ComposerState;

fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}

#[tokio::test]
async fn all_slots_include_never_started_reservations_and_membership_aba() {
    let fixture = Fixture::new().await;
    let empty = fixture.account.ownership_inventory().unwrap();
    let slot = fixture.account.reserve_editor().unwrap();
    assert_eq!(
        empty.validate(&fixture.account),
        Err(ownership::Error::Changed)
    );
    let reserved = fixture.account.ownership_inventory().unwrap();
    assert_eq!(reserved.editor_slot_count(), 1);
    assert!(reserved.slots[0].editor.is_none());
    assert!(reserved.slots[0].opening.is_none());
    drop(slot);
    assert_eq!(
        reserved.validate(&fixture.account),
        Err(ownership::Error::Changed)
    );
    assert_eq!(
        empty.validate(&fixture.account),
        Err(ownership::Error::Changed)
    );
    assert_eq!(
        fixture
            .account
            .ownership_inventory()
            .unwrap()
            .editor_slot_count(),
        0
    );
}

#[tokio::test]
async fn actual_input_serial_changes_even_for_equal_bytes_and_keeps_same_editor() {
    let fixture = Fixture::new().await;
    let editor = editor::Editor::open(fixture.owner.clone()).await.unwrap();
    let gate = editor.gate.lock().await;
    let before = fixture.account.ownership_inventory().unwrap();
    let captured = before.slots[0].editor.as_ref().unwrap();
    assert!(Arc::ptr_eq(&captured.original, &editor));
    let serial = editor.submit(&local("Original")).unwrap();
    assert!(serial > captured.serial);
    assert_eq!(
        before.validate(&fixture.account),
        Err(ownership::Error::Changed)
    );
    let equal = fixture.account.ownership_inventory().unwrap();
    assert_eq!(equal.slots[0].editor.as_ref().unwrap().serial, serial);
    assert!(editor.submit(&local("Original")).unwrap() > serial);
    assert_eq!(
        equal.validate(&fixture.account),
        Err(ownership::Error::Changed)
    );
    assert_eq!(editor.snapshot().pending.unwrap().composer.text, "Original");
    let undo = fixture.account.ownership_inventory().unwrap();
    editor.submit(&local("Typed and then undone")).unwrap();
    editor.submit(&local("Original")).unwrap();
    assert_eq!(
        undo.validate(&fixture.account),
        Err(ownership::Error::Changed)
    );
    assert_eq!(editor.snapshot().pending.unwrap().composer.text, "Original");
    drop(gate);
    editor.flush().await.unwrap();
}

#[tokio::test]
async fn lost_open_waiter_before_first_poll_keeps_same_retained_job_through_install() {
    let fixture = Fixture::new().await;
    let opened = editor_open::Request {
        owner: fixture.owner.clone(),
        expected: None,
    }
    .begin()
    .unwrap_or_else(|_| panic!("exact open admission"));
    let id = opened.id().to_owned();
    drop(opened);
    let before = fixture.account.ownership_inventory().unwrap();
    assert_eq!(before.editor_slot_count(), 1);
    assert!(before.slots[0].editor.is_none());
    let job = before.slots[0].opening.as_ref().unwrap().original.clone();
    assert_eq!(
        job.evidence.lock().unwrap().terminal,
        editor_open::Terminal::Running
    );
    let retained = fixture
        .account
        .retained_openings()
        .unwrap()
        .into_iter()
        .find(|opened| opened.id() == id)
        .unwrap();
    retained.ready().await;
    let editor = retained.result().unwrap();
    assert_eq!(
        before.validate(&fixture.account),
        Err(ownership::Error::Changed)
    );
    let after = fixture.account.ownership_inventory().unwrap();
    assert!(Arc::ptr_eq(
        &job,
        &after.slots[0].opening.as_ref().unwrap().original
    ));
    assert!(Arc::ptr_eq(
        &editor,
        &after.slots[0].editor.as_ref().unwrap().original
    ));
    assert_eq!(
        job.evidence.lock().unwrap().terminal,
        editor_open::Terminal::Installed
    );
}

#[tokio::test]
async fn exact_operation_evidence_and_monotonic_unknown_remain_retained() {
    let fixture = Fixture::new().await;
    let operation = fixture.operation().await;
    let before = fixture.account.ownership_inventory().unwrap();
    assert_eq!(before.operation_count(), 1);
    assert!(before.operations[0].original.same(&operation));
    operation.phase(operation::Phase::Review).unwrap();
    assert_eq!(
        before.validate(&fixture.account),
        Err(ownership::Error::Changed)
    );
    let phase = fixture.account.ownership_inventory().unwrap();
    let review = fixture.owner.review().await.unwrap();
    operation.review(&review).unwrap();
    assert_eq!(
        phase.validate(&fixture.account),
        Err(ownership::Error::Changed)
    );
    let evidenced = fixture.account.ownership_inventory().unwrap();
    operation.unknown();
    assert_eq!(
        evidenced.validate(&fixture.account),
        Err(ownership::Error::Changed)
    );
    let unknown = fixture.account.ownership_inventory().unwrap();
    unknown.validate(&fixture.account).unwrap();
    assert_eq!(unknown.operation_count(), 1);
    assert!(!unknown.operations[0].live);
    assert_eq!(
        operation.record().evidence.lock().unwrap().status,
        operation::Status::Unknown
    );
    assert!(operation.acknowledge().is_err());
    assert_eq!(operation.record().local.composer.text, "Original");
}

#[tokio::test]
async fn exact_finished_removal_invalidates_membership_without_erasing_inventory_objects() {
    let fixture = Fixture::new().await;
    let operation = fixture.operation().await;
    operation.finish(Ok(operation::Status::Finished));
    let before = fixture.account.ownership_inventory().unwrap();
    operation.acknowledge().unwrap();
    assert_eq!(
        before.validate(&fixture.account),
        Err(ownership::Error::Changed)
    );
    assert!(before.operations[0].original.same(&operation));
    assert_eq!(
        before.operations[0].original.record().local.composer.text,
        "Original"
    );
    assert_eq!(
        fixture
            .account
            .ownership_inventory()
            .unwrap()
            .operation_count(),
        0
    );
}

#[tokio::test]
async fn replacement_account_with_same_id_never_validates_old_component() {
    let first = Fixture::new().await;
    let second = Fixture::new().await;
    assert_eq!(first.account.id(), second.account.id());
    let inventory = first.account.ownership_inventory().unwrap();
    assert_eq!(
        inventory.validate(&second.account),
        Err(ownership::Error::Replaced)
    );
    inventory.validate(&first.account).unwrap();
}

#[tokio::test]
async fn registry_poison_is_unknown_instead_of_empty() {
    let fixture = Fixture::new().await;
    let before = fixture.account.ownership_inventory().unwrap();
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = fixture.account.operations.lock().unwrap();
        panic!("deliberate retained-registry poison");
    }));
    assert_eq!(
        before.validate(&fixture.account),
        Err(ownership::Error::Unknown)
    );
    assert!(matches!(
        fixture.account.ownership_inventory(),
        Err(ownership::Error::Unknown)
    ));
}
