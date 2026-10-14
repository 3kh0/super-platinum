use super::*;
mod ownership;
use serde_json::json;
use super_platinum_core::slack_lists::ListRecordReadback;

#[test]
fn deletion_retires_only_same_owner_known_created_identity() {
    for mismatch in ["record", "list", "owner", "present", "none"] {
        let mut shell = shell();
        let attempt = begin_current(&mut shell);
        let data = loaded(&shell);
        validate(&mut shell, &attempt, &data).unwrap();
        assert!(finish(
            &mut shell,
            &attempt,
            Ok(json!({"ok":true,"record":created()}))
        ));
        let list = if mismatch == "list" {
            "F_OTHER"
        } else {
            "F_LIST_FIXTURE"
        };
        let id = if mismatch == "record" {
            "R_OTHER"
        } else {
            "R_CREATED"
        };
        let response = if mismatch == "present" {
            json!({"ok":true,"record":created()})
        } else {
            json!({"ok":false,"error":"record_deleted"})
        };
        let tombstone = ListRecordReadback::from_response(list, id, &response).unwrap();
        if mismatch == "owner" {
            shell.core.active_account = Some("different-account".into());
        }
        let scope = Scope::capture(&shell).unwrap();
        reconcile_deleted(&mut shell, &scope, &tombstone);
        assert_eq!(
            shell.slack_lists.create.receipts.is_empty(),
            mismatch == "none",
            "{mismatch}"
        );
    }
}

#[test]
fn deletion_clears_matching_created_notice_but_preserves_unrelated_notice() {
    let mut shell = shell();
    let attempt = begin_current(&mut shell);
    let data = loaded(&shell);
    validate(&mut shell, &attempt, &data).unwrap();
    assert!(finish(
        &mut shell,
        &attempt,
        Ok(json!({"ok":true,"record":created()}))
    ));
    read(&mut shell, with_record(data, created()));
    assert!(presentation(&shell).notice.is_some());
    let scope = Scope::capture(&shell).unwrap();
    for id in ["R_OTHER", "R_CREATED"] {
        let deleted = ListRecordReadback::from_response(
            "F_LIST_FIXTURE",
            id,
            &json!({"ok":false,"error":"record_deleted"}),
        )
        .unwrap();
        reconcile_deleted(&mut shell, &scope, &deleted);
        assert_eq!(presentation(&shell).notice.is_none(), id == "R_CREATED");
    }
    assert!(!presentation(&shell).can_open);
}
fn shell() -> ShellState {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    let loaded = shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    let mut raw = loaded.document.raw.clone();
    raw["list_limits"] = json!({"over_row_maximum":false});
    loaded.document = Document::parse(raw).unwrap();
    shell
}
fn begin_current(shell: &mut ShellState) -> Attempt {
    let view = shell.slack_lists.view.as_ref().unwrap();
    let visit = view.generation;
    let layout = view.layout.generation;
    begin(shell, visit, layout).unwrap()
}
fn loaded(shell: &ShellState) -> Loaded {
    shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .clone()
        .unwrap()
}
fn created() -> Value {
    json!({"id":"R_CREATED","list_id":"F_LIST_FIXTURE","date_created":12345,"created_by":"U1","fields":[],"position":999})
}
fn with_record(mut loaded: Loaded, record: Value) -> Loaded {
    let mut raw = loaded.records.raw.clone();
    raw["records"].as_array_mut().unwrap().push(record);
    loaded.records = RecordsPage::parse(raw).unwrap();
    loaded
}
fn read(shell: &mut ShellState, loaded: Loaded) {
    let attempt = begin_read(shell).unwrap();
    assert!(!finish_read(shell, &attempt, Ok(loaded)));
}
#[test]
fn explicit_create_is_duplicate_locked_and_cancelled_preflight_never_sends() {
    let mut shell = shell();
    assert!(presentation(&shell).enabled);
    let attempt = begin_current(&mut shell);
    let data = loaded(&shell);
    assert!(blocks_mutation(&shell));
    assert!(presentation(&shell).operation.unwrap().can_cancel);
    let view = shell.slack_lists.view.as_ref().unwrap();
    let visit = view.generation;
    let layout = view.layout.generation;
    assert!(begin(&mut shell, visit, layout).is_err());
    assert!(!cancel(&mut shell, attempt.serial + 1));
    assert!(cancel(&mut shell, attempt.serial));
    assert!(validate(&mut shell, &attempt, &data).is_err());
    assert!(!finish(
        &mut shell,
        &attempt,
        Ok(json!({"ok":true,"record":created()}))
    ));
    assert!(presentation(&shell).enabled);
}
#[test]
fn readback_uses_only_confirmed_identity_and_opens_canonical_detail() {
    let mut shell = shell();
    let attempt = begin_current(&mut shell);
    let data = loaded(&shell);
    validate(&mut shell, &attempt, &data).unwrap();
    assert!(!cancel(&mut shell, attempt.serial));
    assert!(finish(
        &mut shell,
        &attempt,
        Ok(json!({"ok":true,"record":created()}))
    ));
    assert!(blocks_mutation(&shell));
    read(&mut shell, data.clone());
    assert!(blocks_mutation(&shell));
    read(&mut shell, with_record(data, created()));
    assert!(!blocks_mutation(&shell));
    let detail = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap()).unwrap();
    assert_eq!(detail.record_id, "R_CREATED");
    assert_eq!(presentation(&shell).notice.as_deref(), Some("Item added."));
}
#[test]
fn post_dispatch_transport_failure_stays_uncertain_without_replay_across_reopen() {
    let mut shell = shell();
    let attempt = begin_current(&mut shell);
    let data = loaded(&shell);
    validate(&mut shell, &attempt, &data).unwrap();
    // Successful preflight consumes Preparing and enters the dispatch boundary.
    // A transport failure from that boundary cannot prove Slack did not create.
    assert!(receipt(&shell).unwrap().stage == Stage::Sending);
    assert!(finish(
        &mut shell,
        &attempt,
        Err(Error::Transport("lost response".into()))
    ));
    assert!(presentation(&shell).operation.unwrap().can_retire);
    assert!(receipt(&shell).unwrap().stage == Stage::Uncertain);
    assert!(!dismiss(&mut shell, attempt.serial));
    assert!(!cancel(&mut shell, attempt.serial));
    assert!(validate(&mut shell, &attempt, &data).is_err());
    close_shell(&mut shell);
    let file = serde_json::from_value(data.document.raw.clone()).unwrap();
    assert!(open_shell(&mut shell, file));
    read(&mut shell, with_record(data, created()));
    assert!(blocks_mutation(&shell));
    assert!(shell.slack_lists.view.as_ref().unwrap().detail.is_none());
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (visit, layout) = (view.generation, view.layout.generation);
    assert!(begin(&mut shell, visit, layout).is_err());
    assert_eq!(receipt(&shell).unwrap().attempt.serial, attempt.serial);
    assert_eq!(shell.slack_lists.create.receipts.len(), 1);
    assert!(!finish(
        &mut shell,
        &attempt,
        Ok(json!({"ok":true,"record":created()}))
    ));
    assert!(!retire(&mut shell, attempt.serial + 1));
    assert!(retire(&mut shell, attempt.serial));
    assert!(!blocks_mutation(&shell));
    assert!(presentation(&shell).enabled);
    assert!(shell.slack_lists.create.receipts.is_empty());
    // Acknowledgment only retires local evidence; no dispatch is scheduled.
    assert_eq!(fixture::creation_attempts(&shell), 0);
}
#[test]
fn stale_visit_selection_scope_and_fresh_access_reject_preflight() {
    for change in ["visit", "selection", "account", "limit"] {
        let mut shell = shell();
        let attempt = begin_current(&mut shell);
        let mut data = loaded(&shell);
        match change {
            "visit" => {
                close_shell(&mut shell);
                let file = serde_json::from_value(data.document.raw.clone()).unwrap();
                open_shell(&mut shell, file);
            }
            "selection" => {
                shell
                    .slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .layout
                    .selection_generation = next_id()
            }
            "account" => shell.core.account_epoch += 1,
            "limit" => {
                let mut raw = data.document.raw.clone();
                raw["list_limits"]["over_row_maximum"] = json!(true);
                data.document = Document::parse(raw).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(validate(&mut shell, &attempt, &data).is_err(), "{change}");
    }
}
#[test]
fn malformed_success_locks_but_definitive_offline_can_be_explicitly_retried() {
    let mut shell = shell();
    let attempt = begin_current(&mut shell);
    let data = loaded(&shell);
    validate(&mut shell, &attempt, &data).unwrap();
    assert!(finish(&mut shell, &attempt, Ok(json!({"ok":true}))));
    assert!(blocks_mutation(&shell));
    shell.core.account_epoch += 1;
    assert!(!retire(&mut shell, attempt.serial));
    assert!(!poll(&mut shell));
    assert_eq!(shell.slack_lists.create.receipts.len(), 1);
    let mut shell = self::shell();
    let attempt = begin_current(&mut shell);
    let data = loaded(&shell);
    validate(&mut shell, &attempt, &data).unwrap();
    assert!(!finish(
        &mut shell,
        &attempt,
        Err(Error::Offline("fixture offline".into()))
    ));
    assert!(!blocks_mutation(&shell));
    assert!(presentation(&shell).enabled);
}
#[test]
fn confirmed_result_does_not_replace_a_reopened_visit_but_remains_openable() {
    let mut shell = shell();
    let attempt = begin_current(&mut shell);
    let data = loaded(&shell);
    validate(&mut shell, &attempt, &data).unwrap();
    close_shell(&mut shell);
    assert!(!finish(
        &mut shell,
        &attempt,
        Ok(json!({"ok":true,"record":created()}))
    ));
    let file = serde_json::from_value(data.document.raw.clone()).unwrap();
    open_shell(&mut shell, file);
    read(&mut shell, with_record(data, created()));
    assert!(!blocks_mutation(&shell));
    assert!(shell.slack_lists.view.as_ref().unwrap().detail.is_none());
    assert!(presentation(&shell).can_open);
}

#[test]
fn result_open_keeps_original_completed_receipt_after_concurrent_cell_and_schema_changes() {
    let mut shell = shell();
    let data = shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    let mut document = data.document.raw.clone();
    document["list_metadata"]["schema"].as_array_mut().unwrap().push(json!({"id":"C_DONE","key":"todo_completed","name":"Completed","type":"todo_completed"}));
    document["list_metadata"]["views"] = json!([{"id":"V_COMPLETED","name":"Completed","type":"table","default_view_key":"completed_items","columns":[],"filters":[],"sorts":[]}]);
    data.document = Document::parse(document).unwrap();
    shell.slack_lists.view.as_mut().unwrap().layout.selected = Some("V_COMPLETED".into());
    let attempt = begin_current(&mut shell);
    let mut data = loaded(&shell);
    validate(&mut shell, &attempt, &data).unwrap();
    let mut returned = created();
    returned["fields"] = json!([{"key":"todo_completed","column_id":"C_DONE","checkbox":true}]);
    close_shell(&mut shell);
    finish(
        &mut shell,
        &attempt,
        Ok(json!({"ok":true,"record":returned.clone()})),
    );
    let file = serde_json::from_value(data.document.raw.clone()).unwrap();
    open_shell(&mut shell, file);
    let mut document = data.document.raw.clone();
    document["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"C_NEW","key":"new","name":"New field","type":"text"}));
    data.document = Document::parse(document).unwrap();
    let mut current = returned.clone();
    current["fields"][0]["checkbox"] = json!(false);
    current["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":"new","column_id":"C_NEW","text":"Concurrent value"}));
    read(&mut shell, with_record(data, current));
    assert_eq!(
        shell.slack_lists.create.notice.as_ref().unwrap().record.raw,
        returned
    );
    assert!(presentation(&shell).can_open);
    let visit = shell.slack_lists.view.as_ref().unwrap().generation;
    assert!(open_result_shell(&mut shell, visit));
    let detail = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap()).unwrap();
    assert_eq!(detail.record_id, "R_CREATED");
    assert!(
        detail
            .row
            .cells
            .iter()
            .any(|cell| cell.column_id == "C_NEW" && cell.text == "Concurrent value")
    );
    assert!(
        detail
            .row
            .cells
            .iter()
            .any(|cell| cell.column_id == "C_DONE" && cell.checkbox == Some(false))
    );
}

#[test]
fn confirmed_but_missing_item_can_be_dismissed_without_replaying_creation() {
    let mut shell = shell();
    let attempt = begin_current(&mut shell);
    let data = loaded(&shell);
    validate(&mut shell, &attempt, &data).unwrap();
    assert!(!dismiss(&mut shell, attempt.serial));
    assert!(!retire(&mut shell, attempt.serial));
    finish(
        &mut shell,
        &attempt,
        Ok(json!({"ok":true,"record":created()})),
    );
    read(&mut shell, data);
    assert!(blocks_mutation(&shell));
    assert!(presentation(&shell).operation.unwrap().can_dismiss);
    assert!(!presentation(&shell).operation.unwrap().can_retire);
    assert!(!retire(&mut shell, attempt.serial));
    assert!(!dismiss(&mut shell, attempt.serial + 1));
    assert!(dismiss(&mut shell, attempt.serial));
    assert!(!blocks_mutation(&shell));
    assert!(presentation(&shell).enabled);
    assert_eq!(fixture::creation_attempts(&shell), 0);
    assert!(!finish(
        &mut shell,
        &attempt,
        Ok(json!({"ok":true,"record":created()}))
    ));
}
