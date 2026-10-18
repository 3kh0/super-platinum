use super::*;
use serde_json::json;

pub(super) fn shell() -> ShellState {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    open_detail(&mut shell, 0);
    shell
}
fn open_detail(shell: &mut ShellState, index: usize) -> u64 {
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (instance, visit, layout) = (
        view.rows[index].instance_id.clone(),
        view.generation,
        view.layout.generation,
    );
    assert!(record_detail::open_shell(shell, &instance, visit, layout));
    record_detail::presentation(shell.slack_lists.view.as_ref().unwrap())
        .unwrap()
        .generation
}
pub(super) fn loaded(shell: &ShellState) -> Loaded {
    shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .clone()
        .unwrap()
}
fn begin(shell: &mut ShellState) -> Attempt {
    let detail = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap()).unwrap();
    let serial = begin_shell(shell, detail.generation).unwrap();
    confirm_shell(shell, serial).unwrap()
}
pub(super) fn sending(shell: &mut ShellState) -> Attempt {
    let attempt = begin(shell);
    let data = loaded(shell);
    validate(shell, &attempt, &data).unwrap();
    attempt
}
pub(super) fn accepted(attempt: &Attempt) -> AcceptedDeletion {
    attempt
        .intent
        .validate_response(&json!({"ok":true}))
        .unwrap()
}
pub(super) fn deleted(attempt: &Attempt) -> ListRecordReadback {
    ListRecordReadback::from_response(
        attempt.intent.list_id(),
        attempt.intent.record_id(),
        &json!({"ok":false,"error":"record_deleted"}),
    )
    .unwrap()
}
pub(super) fn read(shell: &mut ShellState, data: Loaded) {
    let read = begin_read(shell).unwrap();
    assert!(!finish_read(shell, &read, Ok(data)));
}
pub(super) fn reopen(shell: &mut ShellState, data: Loaded) {
    close_shell(shell);
    assert!(open_shell(
        shell,
        serde_json::from_value(data.document.raw.clone()).unwrap()
    ));
    read(shell, data);
}

#[test]
fn confirmation_cancel_and_preparing_cancel_never_dispatch_or_unlock_another_attempt() {
    let mut shell = shell();
    let generation = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap())
        .unwrap()
        .generation;
    let serial = begin_shell(&mut shell, generation).unwrap();
    assert!(blocks_mutation(&shell));
    assert!(begin_shell(&mut shell, generation).is_err());
    assert!(!cancel_confirmation_shell(&mut shell, serial + 1));
    assert!(cancel_confirmation_shell(&mut shell, serial));
    assert!(confirm_shell(&mut shell, serial).is_err());
    let attempt = begin(&mut shell);
    assert!(confirm_shell(&mut shell, attempt.serial).is_err());
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (visit, layout) = (view.generation, view.layout.generation);
    assert!(create::begin(&mut shell, visit, layout).is_err());
    assert!(mutations::begin_edit(&mut shell, attempt.intent.record_id(), "C_CHECK").is_err());
    assert!(editor::begin_shell(&mut shell, attempt.intent.record_id(), "C_NAME", visit).is_err());
    assert!(cancel_preparing_shell(&mut shell, attempt.serial));
    let data = loaded(&shell);
    assert!(validate(&mut shell, &attempt, &data).is_err());
    assert!(!finish(&mut shell, &attempt, Ok(accepted(&attempt))));
    assert_eq!(fixture::attempts(&shell), 0);
}

#[test]
fn changed_permission_target_and_closed_visit_cannot_pass_preflight() {
    for change in ["permission", "record", "close", "epoch"] {
        let mut shell = shell();
        let attempt = begin(&mut shell);
        let mut data = loaded(&shell);
        match change {
            "permission" => {
                let mut raw = data.document.raw.clone();
                raw["access"] = json!("read");
                data.document = Document::parse(raw).unwrap();
            }
            "record" => {
                let mut raw = data.records.raw.clone();
                let record = raw["records"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|record| record["id"] == attempt.intent.record_id())
                    .unwrap();
                record["position"] = json!("changed");
                data.records = RecordsPage::parse(raw).unwrap();
            }
            "close" => close_shell(&mut shell),
            "epoch" => shell.core.account_epoch += 1,
            _ => unreachable!(),
        }
        assert!(validate(&mut shell, &attempt, &data).is_err(), "{change}");
    }
}

#[test]
fn only_definite_not_sent_releases_a_dispatched_receipt() {
    for uncertain in [false, true] {
        let mut shell = shell();
        let attempt = sending(&mut shell);
        let failure = if uncertain {
            ListDeleteFailure::Uncertain(Error::Offline("link lost after dispatch".into()))
        } else {
            ListDeleteFailure::NotSent(Error::Offline("known offline before dispatch".into()))
        };
        assert!(!finish(&mut shell, &attempt, Err(failure)));
        assert_eq!(blocks_mutation(&shell), uncertain);
        assert!(!finish(&mut shell, &attempt, Ok(accepted(&attempt))));
        if uncertain {
            assert_eq!(receipt(&shell).unwrap().stage, Stage::Uncertain);
            assert!(!cancel_preparing_shell(&mut shell, attempt.serial));
        }
    }
}

#[test]
fn uncertain_survives_reopen_and_page_absence_until_exact_tombstone() {
    let mut shell = shell();
    let attempt = sending(&mut shell);
    let mut data = loaded(&shell);
    finish(
        &mut shell,
        &attempt,
        Err(ListDeleteFailure::Uncertain(Error::Transport(
            "response lost".into(),
        ))),
    );
    let mut raw = data.records.raw.clone();
    raw["records"]
        .as_array_mut()
        .unwrap()
        .retain(|record| record["id"] != attempt.intent.record_id());
    data.records = RecordsPage::parse(raw).unwrap();
    reopen(&mut shell, data);
    assert!(blocks_mutation(&shell));
    let check = begin_check(&mut shell, attempt.serial).unwrap();
    let wrong = ListRecordReadback::from_response(
        attempt.intent.list_id(),
        "R_OTHER",
        &json!({"ok":false,"error":"record_deleted"}),
    )
    .unwrap();
    assert!(!finish_check(&mut shell, &check, Ok(wrong)));
    assert!(blocks_mutation(&shell));
    let check = begin_check(&mut shell, attempt.serial).unwrap();
    assert!(finish_check(&mut shell, &check, Ok(deleted(&attempt))));
    assert!(!blocks_mutation(&shell));
    assert_eq!(
        presentation(&shell).notice.as_deref(),
        Some("Item deleted.")
    );
}

#[test]
fn late_acceptance_after_rotation_only_settles_original_owner_then_fresh_read() {
    let mut shell = shell();
    let attempt = sending(&mut shell);
    let data = loaded(&shell);
    shell.core.account_epoch += 1;
    poll(&mut shell);
    assert_eq!(
        shell.slack_lists.delete.receipts[&attempt.key()].stage,
        Stage::Uncertain
    );
    reopen(&mut shell, data.clone());
    let revision = shell.slack_lists.view.as_ref().unwrap().revision;
    assert!(blocks_mutation(&shell));
    assert!(!finish(&mut shell, &attempt, Ok(accepted(&attempt))));
    assert_eq!(shell.slack_lists.view.as_ref().unwrap().revision, revision);
    assert_eq!(receipt(&shell).unwrap().stage, Stage::AwaitingRead);
    let check = begin_check(&mut shell, attempt.serial).unwrap();
    assert!(finish_check(&mut shell, &check, Ok(deleted(&attempt))));
    assert!(!blocks_mutation(&shell));
}

#[test]
fn another_account_never_sees_or_reconciles_original_receipt() {
    let mut shell = shell();
    let attempt = sending(&mut shell);
    let data = loaded(&shell);
    let original_account = shell.core.active_account.clone();
    shell.core.active_account = Some("other-account".into());
    shell.core.account_epoch += 1;
    reopen(&mut shell, data.clone());
    assert!(presentation(&shell).operation.is_none());
    assert!(begin_check(&mut shell, attempt.serial).is_none());
    assert!(!finish(&mut shell, &attempt, Ok(accepted(&attempt))));
    assert_eq!(shell.slack_lists.delete.receipts.len(), 1);
    shell.core.active_account = original_account;
    shell.core.account_epoch += 1;
    reopen(&mut shell, data);
    assert!(blocks_mutation(&shell));
    assert!(begin_check(&mut shell, attempt.serial).is_some());
}

#[test]
fn old_read_lease_cannot_clear_new_read_or_new_detail() {
    let mut shell = shell();
    let attempt = sending(&mut shell);
    let data = loaded(&shell);
    assert!(finish(&mut shell, &attempt, Ok(accepted(&attempt))));
    let old = begin_check(&mut shell, attempt.serial).unwrap();
    reopen(&mut shell, data);
    open_detail(&mut shell, 1);
    let open = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap())
        .unwrap()
        .open_generation;
    let fresh = begin_check(&mut shell, attempt.serial).unwrap();
    assert!(!finish_check(&mut shell, &old, Ok(deleted(&attempt))));
    assert_eq!(
        receipt(&shell).unwrap().read.as_ref().unwrap().read,
        fresh.read
    );
    assert!(finish_check(&mut shell, &fresh, Ok(deleted(&attempt))));
    assert_eq!(
        record_detail::presentation(shell.slack_lists.view.as_ref().unwrap())
            .unwrap()
            .open_generation,
        open
    );
}

#[test]
fn present_and_rate_limited_readbacks_keep_lock_and_honor_retry_after() {
    let mut shell = shell();
    let attempt = sending(&mut shell);
    let raw = loaded(&shell)
        .records
        .records
        .iter()
        .find(|record| record.id == attempt.intent.record_id())
        .unwrap()
        .raw
        .clone();
    assert!(finish(&mut shell, &attempt, Ok(accepted(&attempt))));
    let check = begin_check(&mut shell, attempt.serial).unwrap();
    let present = ListRecordReadback::from_response(
        attempt.intent.list_id(),
        attempt.intent.record_id(),
        &json!({"ok":true,"record":raw}),
    )
    .unwrap();
    assert!(!finish_check(&mut shell, &check, Ok(present)));
    assert!(blocks_mutation(&shell));
    let check = begin_check(&mut shell, attempt.serial).unwrap();
    assert!(!finish_check(
        &mut shell,
        &check,
        Err(Error::RateLimited {
            retry_after_secs: Some(60)
        })
    ));
    assert!(receipt(&shell).unwrap().retry_at > now());
    assert!(begin_check(&mut shell, attempt.serial).is_none());
    assert!(!presentation(&shell).operation.unwrap().can_check);
    assert!(
        presentation(&shell)
            .operation
            .unwrap()
            .message
            .contains("asked us to wait")
    );
    shell
        .slack_lists
        .delete
        .receipts
        .get_mut(&attempt.key())
        .unwrap()
        .retry_at = now() - 1;
    assert!(poll(&mut shell));
    assert_eq!(receipt(&shell).unwrap().retry_at, 0);
    assert!(presentation(&shell).operation.unwrap().can_check);
    assert!(
        !presentation(&shell)
            .operation
            .unwrap()
            .message
            .contains("asked us to wait")
    );
    assert!(!poll(&mut shell));
    assert_eq!(fixture::attempts(&shell), 0);
    assert!(
        receipt(&shell).unwrap().read.is_none(),
        "expiry must not issue a read automatically"
    );
}

#[test]
fn authoritative_tombstone_closes_original_detail_only_after_readback() {
    let mut shell = shell();
    let attempt = sending(&mut shell);
    assert!(finish(&mut shell, &attempt, Ok(accepted(&attempt))));
    assert!(shell.slack_lists.view.as_ref().unwrap().detail.is_some());
    let check = begin_check(&mut shell, attempt.serial).unwrap();
    assert!(finish_check(&mut shell, &check, Ok(deleted(&attempt))));
    assert!(shell.slack_lists.view.as_ref().unwrap().detail.is_none());
    assert!(shell.slack_lists.view.as_ref().unwrap().stale);
}

#[test]
fn capacity_blocks_new_confirmations_without_evicting_unknown_operations() {
    let mut shell = shell();
    let attempt = sending(&mut shell);
    let template = receipt(&shell).unwrap().clone();
    shell.slack_lists.delete.receipts.clear();
    for index in 0..MAX_PER_OWNER {
        shell.slack_lists.delete.receipts.insert(
            (attempt.scope.stable_owner(), format!("F_OTHER_{index}")),
            template.clone(),
        );
    }
    assert!(!presentation(&shell).enabled);
    assert_eq!(shell.slack_lists.delete.receipts.len(), MAX_PER_OWNER);
    assert!(
        presentation(&shell)
            .reason
            .unwrap()
            .contains("earlier item deletions")
    );
}

#[test]
fn stale_ui_callbacks_cannot_report_into_replaced_detail_or_account() {
    let mut shell = shell();
    let generation = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap())
        .unwrap()
        .generation;
    assert!(begin_callback_current(&shell, generation));
    let confirmation = begin_shell(&mut shell, generation).unwrap();
    assert!(confirm_callback_current(&shell, confirmation));
    let data = loaded(&shell);
    reopen(&mut shell, data);
    let next = open_detail(&mut shell, 1);
    shell.slack_lists.view.as_mut().unwrap().error = Some("Current detail notice".into());
    assert!(!begin_callback_current(&shell, generation));
    assert!(!confirm_callback_current(&shell, confirmation));
    assert!(begin_callback_current(&shell, next));
    let next_confirmation = begin_shell(&mut shell, next).unwrap();
    assert!(!confirm_callback_current(&shell, confirmation));
    assert!(confirm_callback_current(&shell, next_confirmation));
    shell.core.account_epoch += 1;
    assert!(!begin_callback_current(&shell, next));
    assert!(!confirm_callback_current(&shell, next_confirmation));
    assert_eq!(
        shell.slack_lists.view.as_ref().unwrap().error.as_deref(),
        Some("Current detail notice")
    );
}

#[test]
fn completion_target_lookup_retains_delete_confirmation_and_receipt_off_view() {
    let mut s = shell();
    let scope = s.slack_lists.scope.clone().unwrap();
    let detail = record_detail::presentation(s.slack_lists.view.as_ref().unwrap()).unwrap();
    let serial = begin_shell(&mut s, detail.generation).unwrap();
    assert!(blocks_target(&s, &scope, "F_LIST_FIXTURE"));
    confirm_shell(&mut s, serial).unwrap();
    s.slack_lists.view.as_mut().unwrap().id = "F_OTHER".into();
    assert!(blocks_target(&s, &scope, "F_LIST_FIXTURE"));
    assert!(!blocks_target(&s, &scope, "F_OTHER"));
    s.core.active_account = Some("foreign".into());
    let foreign = Scope::capture(&s).unwrap();
    assert!(!blocks_target(&s, &foreign, "F_LIST_FIXTURE"));
}
