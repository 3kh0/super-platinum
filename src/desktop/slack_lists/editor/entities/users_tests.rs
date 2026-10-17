use super::*;
use serde_json::json;

fn shell() -> ShellState {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    super::super::super::fixture::add_user_edits(&mut shell).unwrap();
    shell
}
fn open(shell: &mut ShellState, column: &str) -> u64 {
    let visit = shell.slack_lists.view.as_ref().unwrap().generation;
    super::super::begin_shell(shell, "R_LIST_000", column, visit).unwrap();
    shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .editor
        .as_ref()
        .unwrap()
        .generation
}
#[test]
fn ordinary_picker_filters_candidates_without_discarding_existing_uncached_ids() {
    let mut shell = shell();
    let generation = open(&mut shell, "C_EDIT_USERS");
    let editor = super::super::presentation(&shell).unwrap();
    assert_eq!(editor.kind, EditorKind::MultiUser);
    assert_eq!(
        editor.original.as_ref(),
        &CellEditValue::Users(vec!["U00000001".into(), "U99999999".into()])
    );
    assert!(
        editor
            .choices
            .iter()
            .any(|choice| choice.id == "U99999999" && choice.label == "U99999999")
    );
    let ids = editor
        .directory
        .candidates
        .iter()
        .map(|choice| choice.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["U00000001", "U00000002", "W00000003"]);
    let attempt = super::super::begin_save(
        &mut shell,
        generation,
        CellEditValue::Users(vec!["U00000001".into()]),
    )
    .unwrap();
    assert_eq!(attempt.edit.cells()[0]["user"], json!(["U00000001"]));
    assert!(attempt.edit.cells()[0].get("select").is_none());
}
#[test]
fn search_results_are_bounded_and_self_remains_selectable() {
    let mut shell = shell();
    let team = shell.core.active_team.clone().unwrap();
    let workspace = shell.core.workspaces.get_mut(&team).unwrap();
    workspace.self_user_id = "U00000002".into();
    for i in 10..100 {
        let id = format!("U{i:08}");
        workspace.users.insert(
            id.clone(),
            User {
                id,
                name: Some(format!("Person {i}")),
                ..User::default()
            },
        );
    }
    let result = cached(&shell, Kind::Users, "");
    assert_eq!(result.len(), LIMIT);
    assert_eq!(result[0].id, "U00000002");
    assert_eq!(cached(&shell, Kind::Users, "Whitney")[0].id, "W00000003");
}
#[test]
fn directory_completion_cannot_replace_new_query_cancelled_draft_or_account() {
    for change in ["query", "cancel", "account", "read"] {
        let mut shell = shell();
        let generation = open(&mut shell, "C_EDIT_USER");
        let request = begin(&mut shell, generation, "Alice".into()).unwrap();
        match change {
            "query" => {
                begin(&mut shell, generation, "Bob".into()).unwrap();
            }
            "cancel" => {
                super::super::cancel_shell(&mut shell, generation);
            }
            "account" => shell.core.account_epoch += 1,
            "read" => shell.slack_lists.view.as_mut().unwrap().revision = next_id(),
            _ => unreachable!(),
        }
        assert!(!request.current(&shell));
        finish(
            &mut shell,
            &request,
            Ok(Results::Users(vec![User {
                id: "U88888888".into(),
                name: Some("Late directory row".into()),
                ..User::default()
            }])),
        );
        assert!(
            shell
                .slack_lists
                .view
                .as_ref()
                .and_then(|view| view.editor.as_ref())
                .is_none_or(|draft| draft
                    .directory
                    .presentation()
                    .candidates
                    .iter()
                    .all(|choice| choice.id != "U88888888"))
        );
    }
}
#[test]
fn single_clear_and_cardinality_use_existing_cell_guard_while_tasks_remain_readonly() {
    let mut shell = shell();
    let generation = open(&mut shell, "C_EDIT_USER");
    assert!(
        super::super::begin_save(
            &mut shell,
            generation,
            CellEditValue::Users(vec!["U00000001".into(), "U00000002".into()])
        )
        .is_err()
    );
    let attempt =
        super::super::begin_save(&mut shell, generation, CellEditValue::Users(vec![])).unwrap();
    assert_eq!(attempt.edit.cells()[0]["user"], json!([]));
    super::super::cancel_shell(&mut shell, generation);
    let visit = shell.slack_lists.view.as_ref().unwrap().generation;
    assert!(
        super::super::begin_shell(&mut shell, "R_LIST_000", "C_EDIT_TASK_USER", visit).is_err()
    );
}
#[test]
fn rate_limited_search_retains_cached_candidates_and_blocks_requests_until_deadline() {
    let mut shell = shell();
    let generation = open(&mut shell, "C_EDIT_USER");
    let request = begin(&mut shell, generation, "Alice".into()).unwrap();
    finish(
        &mut shell,
        &request,
        Err(Error::RateLimited {
            retry_after_secs: Some(60),
        }),
    );
    assert!(begin(&mut shell, generation, "Bob".into()).is_none());
    let editor = super::super::presentation(&shell).unwrap();
    assert_eq!(editor.directory.query, "Bob");
    assert!(!editor.directory.loading);
    assert_eq!(editor.directory.candidates[0].id, "U00000002");
    assert!(editor.directory.error.unwrap().contains("wait"));
}

#[test]
fn eligible_remote_fuzzy_matches_survive_local_substring_filter() {
    let mut shell = shell();
    let generation = open(&mut shell, "C_EDIT_USER");
    let request = begin(&mut shell, generation, "alce".into()).unwrap();
    assert!(cached(&shell, Kind::Users, "alce").is_empty());
    let alice = workspace(&shell).unwrap().users["U00000001"].clone();
    let bot = workspace(&shell).unwrap().users["U00000004"].clone();
    finish(&mut shell, &request, Ok(Results::Users(vec![alice, bot])));
    let editor = super::super::presentation(&shell).unwrap();
    assert_eq!(
        editor
            .directory
            .candidates
            .iter()
            .map(|choice| choice.id.as_str())
            .collect::<Vec<_>>(),
        vec!["U00000001"]
    );
}

#[test]
fn hidden_cell_rejects_queued_query_without_changing_directory_state() {
    let mut shell = shell();
    let generation = open(&mut shell, "C_EDIT_USER");
    begin(&mut shell, generation, "Alice".into()).unwrap();
    let before = super::super::presentation(&shell).unwrap().directory;
    shell.slack_lists.view.as_mut().unwrap().rows = Arc::new(Vec::new());
    assert!(begin(&mut shell, generation, "Bob".into()).is_none());
    let after = super::super::presentation(&shell).unwrap().directory;
    assert!(before == after);
}
