use super::*;
use serde_json::json;
fn shell() -> ShellState {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    assert!(fixture::seed(&mut shell));
    shell
}
#[test]
fn special_picker_reaches_closed_intent_and_not_now_reuses_cell_worker_without_canonical_change() {
    let mut shell = shell();
    let e = entry(&shell).unwrap();
    let guard = Guard::capture(&shell, &e).unwrap();
    let before = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .clone()
        .unwrap();
    let shown = super::super::presentation(&shell).unwrap();
    assert_eq!(shown.kind, EditorKind::MultiUser);
    assert!(shown.disabled);
    assert!(shown.assignment.unwrap().can_not_now);
    assert!(
        CellEdit::capture(
            &before.document,
            &before.records,
            &e.row,
            &e.column,
            before.allow_lists.as_deref(),
            before.personal_task_lists.as_ref(),
            CellEditValue::Users(e.selected.clone())
        )
        .is_err()
    );
    assert!(admit(&mut shell, &guard, TaskAssignmentDecision::ExistingAccess).is_err());
    let attempt = admit(
        &mut shell,
        &guard,
        TaskAssignmentDecision::AssignWithoutSharing,
    )
    .unwrap();
    assert_eq!(
        attempt.edit.cells(),
        json!([{"row_id":"R_TASK","column_id":"C_TASK","user":["U00000001","U00000002","W00000003"]}])
    );
    assert_eq!(
        shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records,
        before.records
    );
    assert_eq!(
        shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .document
            .raw,
        before.document.raw
    );
    assert!(entry(&shell).is_none());
    assert!(validate_edit(&mut shell, &attempt, before).is_ok());
}
#[test]
fn cancel_before_share_is_zero_write_and_frees_only_unattempted_receipt() {
    let mut shell = shell();
    let generation = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .editor
        .as_ref()
        .unwrap()
        .generation;
    assert!(super::super::cancel_shell(&mut shell, generation));
    assert!(entry(&shell).is_none());
    assert!(shell.slack_lists.operations.is_empty());
    assert!(fixture::seed(&mut shell));
    let e = entry(&shell).unwrap();
    let guard = Guard::capture(&shell, &e).unwrap();
    guard.update(|e| {
        e.recipients[0].result = ShareResult::Uncertain;
        e.recipients[0].dm_attempted = true;
    });
    assert!(super::super::cancel_shell(&mut shell, guard.draft));
    assert_eq!(
        entry(&shell).unwrap().recipients[0].result,
        ShareResult::Uncertain
    );
    assert!(shell.slack_lists.operations.is_empty());
}
#[test]
fn share_ack_is_not_access_and_page_or_document_change_never_rebases_intent() {
    let mut shell = shell();
    let e = entry(&shell).unwrap();
    let guard = Guard::capture(&shell, &e).unwrap();
    guard.update(|e| {
        e.recipients[0].result = ShareResult::Acknowledged;
        e.stage = Stage::Uncertain;
    });
    assert!(admit(&mut shell, &guard, TaskAssignmentDecision::ExistingAccess).is_err());
    let original = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .clone()
        .unwrap();
    shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap()
        .records
        .next_cursor = None;
    assert!(!guard.current(&shell));
    assert!(Guard::capture(&shell, &entry(&shell).unwrap()).is_err());
    shell.slack_lists.view.as_mut().unwrap().loaded = Some(original);
    shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap()
        .document
        .raw["shares"] = json!({"private":{"D00000002":[]}});
    assert!(
        admit(
            &mut shell,
            &guard,
            TaskAssignmentDecision::AssignWithoutSharing
        )
        .is_err()
    );
    assert!(shell.slack_lists.operations.is_empty());
    assert_eq!(
        entry(&shell).unwrap().recipients[0].result,
        ShareResult::Acknowledged
    );
}
#[test]
fn owner_read_selection_and_editor_aba_retire_original_job() {
    for change in 0..5 {
        registry(|r| r.entries.clear());
        let mut shell = shell();
        let e = entry(&shell).unwrap();
        let guard = Guard::capture(&shell, &e).unwrap();
        match change {
            0 => shell.core.account_epoch += 1,
            1 => shell.slack_lists.view.as_mut().unwrap().read += 1,
            2 => {
                shell
                    .slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .layout
                    .selection_generation += 1
            }
            3 => {
                shell
                    .slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .editor
                    .as_mut()
                    .unwrap()
                    .generation = next_id()
            }
            _ => shell.slack_lists.scope = Scope::capture(&shell),
        }
        assert!(!guard.current(&shell));
        assert!(
            admit(
                &mut shell,
                &guard,
                TaskAssignmentDecision::AssignWithoutSharing
            )
            .is_err()
        );
        assert!(shell.slack_lists.operations.is_empty());
    }
}
#[test]
fn positive_independent_access_unblocks_exact_assignment_without_share_replay() {
    let mut shell = shell();
    let e = entry(&shell).unwrap();
    let guard = Guard::capture(&shell, &e).unwrap();
    guard.update(|e| {
        e.stage = Stage::Uncertain;
        for person in &mut e.recipients {
            person.result = ShareResult::Uncertain;
            e.intent
                .observe_shared_access(&person.id, &json!({"ok":true,"can_see":true}))
                .unwrap();
        }
    });
    let attempt = admit(&mut shell, &guard, TaskAssignmentDecision::ExistingAccess).unwrap();
    assert_eq!(
        attempt.edit.cells()[0]["user"],
        json!(["U00000001", "U00000002", "W00000003"])
    );
    assert!(entry(&shell).is_none());
}
#[test]
fn retired_share_failure_retains_cooldown_and_cannot_enable_new_action() {
    let mut shell = shell();
    let e = entry(&shell).unwrap();
    let guard = Guard::capture(&shell, &e).unwrap();
    guard.update(|e| {
        e.job = Some(guard.job);
        e.recipients[0].result = ShareResult::Sending;
    });
    shell.slack_lists.view.as_mut().unwrap().read += 1;
    guard.update(|e| {
        e.job = None;
        e.recipients[0].result = ShareResult::Uncertain;
        cool_down(
            e,
            &Error::RateLimited {
                retry_after_secs: Some(3600),
            },
        );
    });
    let p = presentation(&shell).unwrap();
    assert!(!p.can_share && !p.can_check && !p.can_not_now);
    assert_eq!(
        entry(&shell).unwrap().recipients[0].result,
        ShareResult::Uncertain
    );
}

#[test]
fn not_now_keeps_unknown_share_in_separate_ledger_until_independent_access() {
    let mut shell = shell();
    let e = entry(&shell).unwrap();
    let guard = Guard::capture(&shell, &e).unwrap();
    guard.update(|e| {
        e.recipients[0].result = ShareResult::Uncertain;
        e.recipients[0].dm_attempted = true;
    });
    admit(
        &mut shell,
        &guard,
        TaskAssignmentDecision::AssignWithoutSharing,
    )
    .unwrap();
    assert!(entry(&shell).is_none());
    let retained = previous_share(&guard.key, "U00000002").unwrap();
    assert_eq!(retained.result, ShareResult::Uncertain);
    assert!(registry(|r| r
        .shares
        .iter()
        .any(|r| r.serial == e.serial && !r.observed)));
    observe_previous_share(&guard.key, "U00000002");
    assert!(previous_share(&guard.key, "U00000002").is_none());
    assert!(registry(|r| r
        .shares
        .iter()
        .any(|r| r.serial == e.serial && r.observed)));
}

#[test]
fn sharing_cooldown_belongs_to_stable_account_owner_across_list_replacement() {
    let shell = shell();
    let key = Key::current(&shell).unwrap();
    record_cooldown(
        &key,
        &Error::RateLimited {
            retry_after_secs: Some(3600),
        },
    );
    let mut other = key.clone();
    other.list = "F_OTHER".into();
    assert!(owner_blocked(&other));
    let mut different = other.clone();
    different.owner.2 = "U_DIFFERENT".into();
    assert!(!owner_blocked(&different));
    let captured = registry(|r| r.deadlines[&key.owner]);
    record_cooldown(
        &key,
        &Error::RateLimited {
            retry_after_secs: Some(1),
        },
    );
    assert_eq!(registry(|r| r.deadlines[&key.owner]), captured);
    assert!(!presentation(&shell).unwrap().can_not_now);
}
