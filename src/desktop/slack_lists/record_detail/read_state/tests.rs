use super::*;
use serde_json::json;

fn shell() -> ShellState {
    ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list")
}
fn open(shell: &mut ShellState) -> u64 {
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (instance, visit, layout) = (
        view.rows[0].instance_id.clone(),
        view.generation,
        view.layout.generation,
    );
    assert!(super::super::open_shell(shell, &instance, visit, layout));
    shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .detail
        .as_ref()
        .unwrap()
        .open_generation
}
fn detail(shell: &ShellState) -> &Detail {
    shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .detail
        .as_ref()
        .unwrap()
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
fn read(shell: &mut ShellState, loaded: Loaded) {
    let read = begin_read(shell).unwrap();
    assert!(!finish_read(shell, &read, Ok(loaded)));
}
#[test]
fn one_claim_survives_canonical_refresh_and_reconcile_without_resending() {
    let mut shell = shell();
    let open = open(&mut shell);
    let attempt = claim(&mut shell, open).unwrap();
    assert!(attempt.current(&shell));
    assert!(claim(&mut shell, open).is_none());
    let action = detail(&shell).generation;
    let data = loaded(&shell);
    read(&mut shell, data);
    assert_ne!(detail(&shell).generation, action);
    assert_eq!(detail(&shell).open_generation, open);
    assert!(attempt.current(&shell));
    assert!(claim(&mut shell, open).is_none());
    // Paging has no serial-state mutation; an ordinary projection also cannot
    // turn a claimed opening back into work.
    reconcile(shell.slack_lists.view.as_mut().unwrap());
    assert!(claim(&mut shell, open).is_none());
    fixture::record_open(&mut shell, &attempt.list, &attempt.record, open);
    assert_eq!(fixture::record_open_count(&shell), 1);
    assert!(fixture::record_open_valid(&shell));
}
#[test]
fn offline_and_failed_requests_consume_the_open_claim_without_touching_records() {
    for error in [
        Error::Offline("fixture offline".into()),
        Error::Transport("lost response".into()),
        Error::Api("invalid_auth".into()),
        Error::RateLimited {
            retry_after_secs: Some(30),
        },
    ] {
        let mut shell = shell();
        let open = open(&mut shell);
        let attempt = claim(&mut shell, open).unwrap();
        let before = loaded(&shell);
        let action = detail(&shell).generation;
        let result = Err(error);
        assert_ne!(outcome(&result), "success");
        // The completion path only classifies diagnostics; it has no shell
        // mutation or retry callback, regardless of whether HTTP was sent.
        assert!(attempt.current(&shell));
        assert!(claim(&mut shell, open).is_none());
        assert_eq!(detail(&shell).generation, action);
        assert_eq!(loaded(&shell).records, before.records);
        assert!(shell.slack_lists.view.as_ref().unwrap().error.is_none());
    }
}
#[test]
fn close_reopen_rejects_old_claim_and_old_mount_but_new_open_can_claim_once() {
    let mut shell = shell();
    let first = open(&mut shell);
    let attempt = claim(&mut shell, first).unwrap();
    let action = detail(&shell).generation;
    assert!(super::super::close_shell(&mut shell, action));
    assert!(!attempt.current(&shell));
    assert!(claim(&mut shell, first).is_none());
    let second = open(&mut shell);
    assert_ne!(first, second);
    assert!(claim(&mut shell, first).is_none());
    assert!(!detail(&shell).read_started);
    assert!(!attempt.current(&shell));
    assert!(claim(&mut shell, second).is_some());
    assert!(claim(&mut shell, second).is_none());
}
#[test]
fn account_workspace_viewer_and_list_visit_changes_revoke_delayed_dispatch() {
    for change in [
        "account",
        "workspace",
        "viewer",
        "list_visit",
        "selection",
        "deleted_file",
    ] {
        let mut shell = shell();
        let open = open(&mut shell);
        let attempt = claim(&mut shell, open).unwrap();
        match change {
            "account" => shell.core.account_epoch += 1,
            "workspace" => shell.core.active_team = Some("OTHER".into()),
            "viewer" => shell.self_account.user_id = "OTHER".into(),
            "list_visit" => shell.slack_lists.view.as_mut().unwrap().generation = next_id(),
            "selection" => {
                shell
                    .slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .layout
                    .selection_generation = next_id()
            }
            "deleted_file" => {
                shell
                    .core
                    .workspaces
                    .get_mut(&attempt.scope.team)
                    .unwrap()
                    .deleted_files
                    .insert(attempt.list.clone());
            }
            _ => unreachable!(),
        }
        assert!(!attempt.current(&shell), "{change}");
    }
}
#[test]
fn canonical_replacement_removal_and_archive_revoke_queued_open() {
    for change in ["creator", "created", "removed", "archived"] {
        let mut shell = shell();
        let open = open(&mut shell);
        let attempt = claim(&mut shell, open).unwrap();
        let mut data = loaded(&shell);
        let mut raw = data.records.raw.clone();
        let records = raw["records"].as_array_mut().unwrap();
        let index = records
            .iter()
            .position(|record| record["id"] == attempt.record)
            .unwrap();
        match change {
            "creator" => records[index]["created_by"] = json!("OTHER"),
            "created" => records[index]["date_created"] = json!(999),
            "removed" => {
                records.remove(index);
            }
            "archived" => records[index]["archived"] = json!(true),
            _ => unreachable!(),
        }
        data.records = RecordsPage::parse(raw).unwrap();
        read(&mut shell, data);
        assert!(!attempt.current(&shell), "{change}");
    }
}
#[test]
fn accepted_created_detail_uses_the_same_explicit_open_lifetime() {
    let mut shell = shell();
    let id = shell.slack_lists.view.as_ref().unwrap().all_rows[0]
        .id
        .clone();
    assert!(super::super::open_created(&mut shell, &id));
    let first = detail(&shell).open_generation;
    let attempt = claim(&mut shell, first).unwrap();
    assert_eq!(attempt.record, id);
    assert!(claim(&mut shell, first).is_none());
    let action = detail(&shell).generation;
    super::super::close_shell(&mut shell, action);
    assert!(super::super::open_created(&mut shell, &id));
    let second = detail(&shell).open_generation;
    assert_ne!(first, second);
    assert!(claim(&mut shell, second).is_some());
}
