use super::super::tests::{accepted, deleted, loaded, read, reopen, sending, shell};
use super::*;
use serde_json::json;

fn deleted_item() -> (ShellState, Attempt, Loaded, Loaded) {
    let mut shell = shell();
    let original = loaded(&shell);
    let delete = sending(&mut shell);
    assert!(finish(&mut shell, &delete, Ok(accepted(&delete))));
    let check = begin_check(&mut shell, delete.serial).unwrap();
    assert!(finish_check(&mut shell, &check, Ok(deleted(&delete))));
    let mut missing = original.clone();
    let mut raw = missing.records.raw.clone();
    raw["records"]
        .as_array_mut()
        .unwrap()
        .retain(|record| record["id"] != delete.intent.record_id());
    missing.records = RecordsPage::parse(raw).unwrap();
    read(&mut shell, missing.clone());
    (shell, delete, original, missing)
}
fn present(attempt: &Attempt, original: &Loaded) -> ListRecordReadback {
    ListRecordReadback::from_response(attempt.intent.list_id(),attempt.intent.record_id(),&json!({"ok":true,"record":original.records.record(attempt.intent.record_id()).unwrap().raw})).unwrap()
}

#[test]
fn same_identity_undo_then_redo_require_distinct_explicit_transitions_and_readback() {
    let (mut shell, delete, original, missing) = deleted_item();
    let serial = available(&shell).unwrap();
    let undo = begin(&mut shell, serial).unwrap();
    assert!(!undo.redo);
    assert!(begin(&mut shell, serial).is_none());
    let intent = validate_restore(&mut shell, &undo, &missing, &deleted(&delete)).unwrap();
    assert!(!intent.deleted());
    assert!(lifecycle::finish_restore(
        &mut shell,
        &undo,
        Ok(intent.validate_response(&json!({"ok":true})).unwrap())
    ));
    assert!(blocks_mutation(&shell));
    let check = begin_check(&mut shell, undo.serial).unwrap();
    assert!(!finish_check(&mut shell, &check, Ok(deleted(&delete))));
    assert!(blocks_mutation(&shell));
    let check = begin_check(&mut shell, undo.serial).unwrap();
    assert!(finish_check(
        &mut shell,
        &check,
        Ok(present(&undo, &original))
    ));
    read(&mut shell, original.clone());
    assert!(!blocks_mutation(&shell));
    assert!(presentation(&shell).redo);
    assert!(
        begin(&mut shell, serial).is_none(),
        "stale Undo cannot consume newer Redo capability"
    );
    let redo_serial = available(&shell).unwrap();
    let redo = begin(&mut shell, redo_serial).unwrap();
    assert!(redo.redo);
    let intent =
        validate_restore(&mut shell, &redo, &original, &present(&redo, &original)).unwrap();
    assert!(intent.deleted());
    assert!(lifecycle::finish_restore(
        &mut shell,
        &redo,
        Ok(intent.validate_response(&json!({"ok":true})).unwrap())
    ));
    let check = begin_check(&mut shell, redo.serial).unwrap();
    assert!(finish_check(&mut shell, &check, Ok(deleted(&delete))));
    read(&mut shell, missing);
    assert!(!presentation(&shell).redo);
    assert!(available(&shell).is_some());
}

#[test]
fn uncertain_restore_keeps_same_owner_lock_across_reopen_and_rejects_other_identity() {
    let (mut shell, delete, original, missing) = deleted_item();
    let serial = available(&shell).unwrap();
    let undo = begin(&mut shell, serial).unwrap();
    validate_restore(&mut shell, &undo, &missing, &deleted(&delete)).unwrap();
    assert!(!lifecycle::finish_restore(
        &mut shell,
        &undo,
        Err(ListDeleteFailure::Uncertain(Error::Offline(
            "lost after dispatch".into()
        )))
    ));
    reopen(&mut shell, missing);
    assert!(blocks_mutation(&shell));
    assert!(available(&shell).is_none());
    let mut raw = original
        .records
        .record(undo.intent.record_id())
        .unwrap()
        .raw
        .clone();
    raw["date_created"] = json!(999);
    let wrong = ListRecordReadback::from_response(
        undo.intent.list_id(),
        undo.intent.record_id(),
        &json!({"ok":true,"record":raw}),
    )
    .unwrap();
    let check = begin_check(&mut shell, undo.serial).unwrap();
    assert!(!finish_check(&mut shell, &check, Ok(wrong)));
    assert!(blocks_mutation(&shell));
    let check = begin_check(&mut shell, undo.serial).unwrap();
    assert!(finish_check(
        &mut shell,
        &check,
        Ok(present(&undo, &original))
    ));
}

#[test]
fn restore_preflight_rejects_rotated_account_and_late_acceptance_needs_new_scope_read() {
    let (mut shell, delete, original, missing) = deleted_item();
    let serial = available(&shell).unwrap();
    let stale = begin(&mut shell, serial).unwrap();
    shell.core.account_epoch += 1;
    assert!(validate_restore(&mut shell, &stale, &missing, &deleted(&delete)).is_err());
    reopen(&mut shell, missing.clone());
    let serial = available(&shell).unwrap();
    let undo = begin(&mut shell, serial).unwrap();
    let intent = validate_restore(&mut shell, &undo, &missing, &deleted(&delete)).unwrap();
    shell.core.account_epoch += 1;
    reopen(&mut shell, missing);
    assert!(blocks_mutation(&shell));
    assert!(!lifecycle::finish_restore(
        &mut shell,
        &undo,
        Ok(intent.validate_response(&json!({"ok":true})).unwrap())
    ));
    let check = begin_check(&mut shell, undo.serial).unwrap();
    assert!(finish_check(
        &mut shell,
        &check,
        Ok(present(&undo, &original))
    ));
}

#[test]
fn known_not_sent_undo_can_be_requested_again_but_preparing_close_never_sends() {
    let (mut shell, delete, _, missing) = deleted_item();
    let serial = available(&shell).unwrap();
    let undo = begin(&mut shell, serial).unwrap();
    validate_restore(&mut shell, &undo, &missing, &deleted(&delete)).unwrap();
    assert!(!lifecycle::finish_restore(
        &mut shell,
        &undo,
        Err(ListDeleteFailure::NotSent(Error::Offline(
            "known offline".into()
        )))
    ));
    assert_eq!(available(&shell), Some(serial));
    let retry = begin(&mut shell, serial).unwrap();
    close_visit(&mut shell);
    assert!(validate_restore(&mut shell, &retry, &missing, &deleted(&delete)).is_err());
}

#[test]
fn undo_callback_from_closed_visit_cannot_target_reopened_list() {
    let (mut shell, _, _, missing) = deleted_item();
    let serial = available(&shell).unwrap();
    let visit = shell.slack_lists.view.as_ref().unwrap().generation;
    assert!(callback_current(&shell, serial, visit));
    reopen(&mut shell, missing);
    assert!(!callback_current(&shell, serial, visit));
    assert!(callback_current(
        &shell,
        serial,
        shell.slack_lists.view.as_ref().unwrap().generation
    ));
}

#[test]
fn followup_read_requires_current_uncancelled_preparing_owner() {
    for change in ["cancel", "reopen", "account", "sending"] {
        let (mut shell, delete, _, missing) = deleted_item();
        let serial = available(&shell).unwrap();
        let attempt = begin(&mut shell, serial).unwrap();
        assert!(preflight_current(&shell, &attempt));
        match change {
            "cancel" => {
                assert!(cancel_preparing_shell(&mut shell, attempt.serial));
            }
            "reopen" => reopen(&mut shell, missing.clone()),
            "account" => shell.core.account_epoch += 1,
            "sending" => {
                validate_restore(&mut shell, &attempt, &missing, &deleted(&delete)).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(!preflight_current(&shell, &attempt), "{change}");
    }
}
