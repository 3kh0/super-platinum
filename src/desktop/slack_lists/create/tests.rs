use super::*;
use serde_json::json;
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
fn lost_response_survives_reopen_and_never_guesses_new_blank_identity() {
    let mut shell = shell();
    let attempt = begin_current(&mut shell);
    let data = loaded(&shell);
    validate(&mut shell, &attempt, &data).unwrap();
    assert!(finish(
        &mut shell,
        &attempt,
        Err(Error::Transport("lost response".into()))
    ));
    assert!(presentation(&shell).operation.unwrap().can_retire);
    close_shell(&mut shell);
    let file = serde_json::from_value(data.document.raw.clone()).unwrap();
    assert!(open_shell(&mut shell, file));
    read(&mut shell, with_record(data, created()));
    assert!(blocks_mutation(&shell));
    assert!(shell.slack_lists.view.as_ref().unwrap().detail.is_none());
    assert!(!retire(&mut shell, attempt.serial + 1));
    assert!(retire(&mut shell, attempt.serial));
    assert!(!blocks_mutation(&shell));
    assert!(presentation(&shell).enabled);
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
    assert!(poll(&mut shell));
    assert!(shell.slack_lists.create.receipts.is_empty());
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
    finish(
        &mut shell,
        &attempt,
        Ok(json!({"ok":true,"record":created()})),
    );
    read(&mut shell, data);
    assert!(blocks_mutation(&shell));
    assert!(presentation(&shell).operation.unwrap().can_dismiss);
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
