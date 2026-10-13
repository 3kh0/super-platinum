use super::*;
use crate::slack_lists::tests::{file, loaded, make_shell, read, saving};
use serde_json::json;

fn accepted(attempt: &EditAttempt) -> Value {
    json!({"ok":true,"cells":attempt.edit.cells()})
}
fn rotate(shell: &mut ShellState) {
    let team = shell.core.active_team.clone().unwrap();
    shell.core.account_epoch += 1;
    shell
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .token = "rotated synthetic".into();
    shell.core.transport = Some(std::sync::Arc::new(
        super_platinum_core::slack::Transport::new("rotated synthetic").unwrap(),
    ));
}
#[test]
fn workspace_switch_preserves_saving_until_response_and_fresh_read() {
    let mut shell = make_shell();
    let attempt = saving(&mut shell);
    let team = shell.core.active_team.clone();
    shell.core.active_team = Some("OTHER".into());
    assert!(poll(&mut shell));
    assert!(shell.slack_lists.operations.is_empty());
    assert!(get(&shell, &attempt).is_some_and(|m| m.phase == Phase::Saving));
    shell.core.active_team = team;
    assert!(open_shell(&mut shell, file()));
    assert!(begin_read(&mut shell).is_none());
    assert!(begin_edit(&mut shell, "row1", "done").is_err());
    let before = shell.slack_lists.view.as_ref().unwrap().error.clone();
    assert!(
        presentation(&shell)
            .unwrap()
            .error
            .unwrap()
            .contains("still being sent")
    );
    finish_edit(&mut shell, &attempt, Ok(accepted(&attempt)));
    assert_eq!(shell.slack_lists.view.as_ref().unwrap().error, before);
    assert!(get(&shell, &attempt).is_some_and(|m| m.phase == Phase::Uncertain));
    assert!(
        presentation(&shell)
            .unwrap()
            .error
            .unwrap()
            .contains("Refresh to check")
    );
    read(&mut shell, loaded(true));
    assert!(get(&shell, &attempt).is_none());
    assert!(begin_edit(&mut shell, "row1", "done").is_ok());
}
#[test]
fn credentials_rotation_keeps_lock_and_old_read_cannot_reconcile_new_scope() {
    let mut shell = make_shell();
    let attempt = saving(&mut shell);
    finish_edit(
        &mut shell,
        &attempt,
        Err(Error::Transport("unknown".into())),
    );
    let old_read = begin_read(&mut shell).unwrap();
    rotate(&mut shell);
    assert!(poll(&mut shell));
    let receipt = get(&shell, &attempt).unwrap();
    assert!(!receipt.attempt.scope.request_ready(&shell));
    assert!(receipt.attempt.scope.current(&shell) == false);
    assert!(open_shell(&mut shell, file()));
    let fresh = begin_read(&mut shell).unwrap();
    finish_read(&mut shell, &old_read, Ok(loaded(true)));
    assert!(get(&shell, &attempt).is_some());
    assert!(shell.slack_lists.view.as_ref().unwrap().loading);
    finish_read(&mut shell, &fresh, Ok(loaded(true)));
    assert!(get(&shell, &attempt).is_none());
}
#[test]
fn another_owner_same_list_is_independent_and_late_response_cannot_touch_its_view() {
    let mut shell = make_shell();
    let old = saving(&mut shell);
    let account = shell.core.active_account.clone();
    shell.core.active_account = Some("OTHER_ACCOUNT".into());
    assert!(poll(&mut shell));
    assert!(open_shell(&mut shell, file()));
    read(&mut shell, loaded(false));
    let new = saving(&mut shell);
    let before = shell.slack_lists.view.as_ref().unwrap().revision;
    finish_edit(&mut shell, &old, Ok(accepted(&old)));
    assert_eq!(shell.slack_lists.view.as_ref().unwrap().revision, before);
    assert!(get(&shell, &old).is_some_and(|m| m.phase == Phase::Uncertain));
    assert!(get(&shell, &new).is_some_and(|m| m.phase == Phase::Saving));
    shell.core.active_account = account;
    assert!(poll(&mut shell));
    assert!(open_shell(&mut shell, file()));
    read(&mut shell, loaded(true));
    assert!(get(&shell, &old).is_none());
    assert!(get(&shell, &new).is_some_and(|m| m.phase == Phase::Saving));
    finish_edit(&mut shell, &new, Err(Error::Api("not_allowed".into())));
    assert!(get(&shell, &new).is_none());
}
#[test]
fn rotated_saving_completion_is_retained_and_preparing_is_retired() {
    let mut shell = make_shell();
    let sent = saving(&mut shell);
    rotate(&mut shell);
    assert!(poll(&mut shell));
    finish_edit(&mut shell, &sent, Err(Error::Transport("unknown".into())));
    assert!(get(&shell, &sent).is_some_and(|m| m.phase == Phase::Uncertain));
    assert!(open_shell(&mut shell, file()));
    // A concrete differing canonical value unlocks only a fresh explicit edit.
    read(&mut shell, loaded(false));
    assert!(get(&shell, &sent).is_none());
    let unsent = begin_edit(&mut shell, "row1", "done").unwrap();
    rotate(&mut shell);
    assert!(poll(&mut shell));
    assert!(get(&shell, &unsent).is_none());
    assert!(validate_edit(&mut shell, &unsent, loaded(false)).is_err());
    assert!(shell.slack_lists.retained_cells.receipts.is_empty());
}
#[test]
fn late_duplicate_completion_cannot_replace_new_attempt() {
    let mut shell = make_shell();
    let old = saving(&mut shell);
    finish_edit(&mut shell, &old, Ok(accepted(&old)));
    read(&mut shell, loaded(true));
    let new = begin_edit(&mut shell, "row1", "done").unwrap();
    validate_edit(&mut shell, &new, loaded(true)).unwrap();
    finish_edit(&mut shell, &old, Err(Error::Api("not_allowed".into())));
    assert!(get(&shell, &new).is_some_and(|m| m.phase == Phase::Saving));
}
#[test]
fn capacity_never_evicts_and_collision_restore_fails_closed() {
    let mut shell = make_shell();
    let attempt = saving(&mut shell);
    let mut duplicate = shell.slack_lists.operations["FL1"].clone();
    duplicate.attempt.serial = next_id();
    let serial = duplicate.attempt.serial;
    shell
        .slack_lists
        .retained_cells
        .receipts
        .insert(serial, duplicate);
    let scope = shell.slack_lists.scope.clone().unwrap();
    assert!(!restore(&mut shell, &scope));
    assert_eq!(
        shell.slack_lists.operations["FL1"].attempt.serial,
        attempt.serial
    );
    assert!(
        shell
            .slack_lists
            .retained_cells
            .receipts
            .contains_key(&serial)
    );
    for _ in 2..MAX_PER_OWNER {
        let mut receipt = shell.slack_lists.operations["FL1"].clone();
        receipt.attempt.serial = next_id();
        shell
            .slack_lists
            .retained_cells
            .receipts
            .insert(receipt.attempt.serial, receipt);
    }
    assert!(capacity(&shell).is_err());
    assert_eq!(
        shell.slack_lists.operations.len() + shell.slack_lists.retained_cells.receipts.len(),
        MAX_PER_OWNER
    );
    for _ in MAX_PER_OWNER..MAX_TOTAL {
        let mut receipt = shell.slack_lists.operations["FL1"].clone();
        receipt.attempt.serial = next_id();
        shell
            .slack_lists
            .retained_cells
            .receipts
            .insert(receipt.attempt.serial, receipt);
    }
    assert!(capacity(&shell).unwrap_err().contains("Unconfirmed"));
    assert_eq!(
        shell.slack_lists.operations.len() + shell.slack_lists.retained_cells.receipts.len(),
        MAX_TOTAL
    );
}
