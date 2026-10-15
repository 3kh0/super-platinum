use super::*;
use serde_json::json;

fn shell(selected: &str) -> ShellState {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    shell.core.active_account = Some(format!("creation-test-{}", next_id()));
    shell.slack_lists.scope = Scope::capture(&shell);
    shell.self_account.user_id = "U00000001".into();
    let team = shell.core.active_team.clone().unwrap();
    shell.core.workspaces.get_mut(&team).unwrap().self_user_id = "U00000001".into();
    shell.slack_lists.scope = Scope::capture(&shell);
    fixture::add_board_views(&mut shell).unwrap();
    // Header creation fixtures add a second todo_completed key. Board owns its
    // own completion column; only the creation permission is needed here.
    fixture::set_creation_limit(&mut shell, Some(false));
    fixture::enable_board_tasks(&mut shell);
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (visit, layout) = (view.generation, view.layout.generation);
    assert!(layout::select_shell(
        &mut shell,
        Some(selected.into()),
        visit,
        layout
    ));
    assert!(reason(&shell).is_none());
    shell
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
fn pending(shell: &mut ShellState, lane: &str, lease: Lease) -> Pending {
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (visit, layout) = (view.generation, view.layout.generation);
    begin(shell, visit, layout, lane, lease).unwrap()
}
fn ready(shell: &mut ShellState, lane: &str, lease: Lease) -> Attempt {
    let pending = pending(shell, lane, lease);
    let intent = prepare(pending.seed, pending.attempt.board.as_ref().unwrap()).unwrap();
    install(shell, &pending.attempt, intent).unwrap()
}
fn edit_document(shell: &mut ShellState, edit: impl FnOnce(&mut Value)) {
    let loaded = shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    let mut raw = loaded.document.raw.clone();
    edit(&mut raw);
    loaded.document = Document::parse(raw).unwrap();
}

#[test]
fn worker_input_contains_only_metadata_and_needs_no_records_page() {
    let mut shell = shell("V_BOARD_CHECK");
    let Pending { attempt, seed } = pending(&mut shell, "true", Lease::new());
    // Exhaustive destructuring intentionally fails compilation if a future
    // change adds Loaded/RecordsPage (or other hidden payload) to worker input.
    let SeedMetadata {
        document,
        allow_lists,
        personal_task_lists,
    } = seed;
    let seed = SeedMetadata::capture(
        &document,
        allow_lists.as_deref(),
        personal_task_lists.as_ref(),
    );
    // Seed conversion needs neither the live page nor any cloned row payloads.
    shell.slack_lists.view.as_mut().unwrap().loaded = None;
    let intent = prepare(seed, attempt.board.as_ref().unwrap()).unwrap();
    assert_eq!(
        intent.initial_fields(),
        Some(&json!([
            {"column_id":"C_CHECK","checkbox":true}
        ]))
    );
    // Losing live authority still prevents the prepared result being installed.
    assert!(install(&mut shell, &attempt, intent).is_err());
}

#[test]
fn supported_lanes_use_same_one_shot_create_and_exact_typed_seeds() {
    for (view, lane, seed) in [
        (
            "V_BOARD_MANY",
            "LANE_1",
            Some(json!({"column_id":"C_BOARD_LANES","select":["LANE_1"]})),
        ),
        (
            "V_BOARD_MANY",
            "",
            Some(json!({"column_id":"C_BOARD_LANES","select":[]})),
        ),
        (
            "V_BOARD_CHECK",
            "true",
            Some(json!({"column_id":"C_CHECK","checkbox":true})),
        ),
        (
            "V_BOARD_CHECK",
            "",
            Some(json!({"column_id":"C_CHECK","checkbox":false})),
        ),
        (
            "V_BOARD_TASKS",
            "true",
            Some(json!({"column_id":"C_TODO","checkbox":true})),
        ),
        (
            "V_BOARD_TASKS",
            "",
            Some(json!({"column_id":"C_TODO","checkbox":false})),
        ),
        ("V_BOARD_UNGROUPED", "", None),
    ] {
        let mut shell = shell(view);
        let attempt = ready(&mut shell, lane, Lease::new());
        assert_eq!(
            attempt.intent.initial_fields().cloned(),
            seed.map(|value| json!([value])),
            "{view}/{lane}"
        );
        assert!(blocks_mutation(&shell));
        fixture::dispatch_creation(&mut shell, attempt.clone());
        assert_eq!(fixture::creation_attempts(&shell), 1);
        assert!(fixture::creation_prepared_valid(&shell));
        let id = fixture::complete_creation(&mut shell).unwrap();
        assert_eq!(receipt(&shell).unwrap().record.as_ref().unwrap().id, id);
        assert!(receipt(&shell).unwrap().stage == Stage::AwaitingRead);
        let data = loaded(&shell);
        assert!(validate(&mut shell, &attempt, &data).is_err());
        assert_eq!(fixture::creation_attempts(&shell), 1);
    }
}

#[test]
fn reservation_blocks_header_duplicate_and_second_lane_before_seed_work() {
    let mut shell = shell("V_BOARD_CHECK");
    let pending = pending(&mut shell, "true", Lease::new());
    assert!(blocks_mutation(&shell));
    assert!(unavailable_for(&shell, true).is_some());
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (visit, layout) = (view.generation, view.layout.generation);
    assert!(super::super::begin(&mut shell, visit, layout).is_err());
    assert!(begin(&mut shell, visit, layout, "", Lease::new()).is_err());
    assert!(receipt(&shell).unwrap().stage == Stage::Preparing);
    assert_eq!(
        receipt(&shell).unwrap().attempt.serial,
        pending.attempt.serial
    );
    assert_eq!(fixture::creation_attempts(&shell), 0);
}

#[test]
fn stale_or_unknown_lane_cannot_reserve() {
    for mismatch in ["lane", "layout", "visit", "mount"] {
        let mut shell = shell("V_BOARD_CHECK");
        let view = shell.slack_lists.view.as_ref().unwrap();
        let (mut visit, mut layout) = (view.generation, view.layout.generation);
        let lease = Lease::new();
        match mismatch {
            "layout" => layout += 1,
            "visit" => visit += 1,
            "mount" => lease.retire(),
            _ => {}
        }
        assert!(
            begin(
                &mut shell,
                visit,
                layout,
                if mismatch == "lane" { "false" } else { "true" },
                lease
            )
            .is_err(),
            "{mismatch}"
        );
        assert!(!blocks_mutation(&shell));
    }
}

#[test]
fn owner_lane_and_seed_context_rechecked_after_preparation_and_before_dispatch() {
    for after_install in [false, true] {
        for mismatch in [
            "account", "epoch", "viewer", "selected", "layout", "lane", "mount", "filters",
            "schema", "limits",
        ] {
            let mut shell = shell("V_BOARD_CHECK");
            let lease = Lease::new();
            let pending = pending(&mut shell, "true", lease.clone());
            let intent = prepare(
                pending.seed.clone(),
                pending.attempt.board.as_ref().unwrap(),
            )
            .unwrap();
            let attempt = if after_install {
                install(&mut shell, &pending.attempt, intent.clone()).unwrap()
            } else {
                pending.attempt.clone()
            };
            match mismatch {
                "account" => shell.core.active_account = Some("another-isolated-account".into()),
                "epoch" => shell.core.account_epoch += 1,
                "viewer" => shell.self_account.user_id = "OTHER_VIEWER".into(),
                "selected" => {
                    shell.slack_lists.view.as_mut().unwrap().layout.selected =
                        Some("V_BOARD_UNGROUPED".into())
                }
                "layout" => shell.slack_lists.view.as_mut().unwrap().layout.generation += 1,
                "lane" => {
                    shell.slack_lists.view.as_mut().unwrap().layout.lanes = Arc::new(Vec::new())
                }
                "mount" => lease.retire(),
                "filters" => edit_document(&mut shell, |raw| {
                    let view = raw["list_metadata"]["views"]
                        .as_array_mut()
                        .unwrap()
                        .iter_mut()
                        .find(|view| view["id"] == "V_BOARD_CHECK")
                        .unwrap();
                    view["filters"] = json!([{ "key":"NAME", "column_id":"C_NAME", "operator":"contains", "values":["different"] }]);
                }),
                "schema" => edit_document(&mut shell, |raw| {
                    raw["list_metadata"]["schema"][0]["name"] = json!("Changed")
                }),
                "limits" => edit_document(&mut shell, |raw| {
                    raw["list_limits"]["over_row_maximum"] = json!(true)
                }),
                _ => unreachable!(),
            }
            if after_install {
                let fresh = loaded(&shell);
                assert!(
                    validate(&mut shell, &attempt, &fresh).is_err(),
                    "{mismatch}"
                );
            } else {
                assert!(install(&mut shell, &attempt, intent).is_err(), "{mismatch}");
            }
            assert_eq!(fixture::creation_attempts(&shell), 0);
        }
    }
}

#[test]
fn fresh_preflight_seed_change_refuses_but_last_read_only_is_harmless() {
    for changed in [false, true] {
        let mut shell = shell("V_BOARD_CHECK");
        let attempt = ready(&mut shell, "true", Lease::new());
        let mut fresh = loaded(&shell);
        let mut raw = fresh.document.raw.clone();
        raw["last_read"] = json!("1800000000.000001");
        if changed {
            let view = raw["list_metadata"]["views"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|view| view["id"] == "V_BOARD_CHECK")
                .unwrap();
            view["info_column_filters"] = json!([]);
        }
        fresh.document = Document::parse(raw).unwrap();
        assert_eq!(validate(&mut shell, &attempt, &fresh).is_ok(), !changed);
    }
}

#[test]
fn cancelled_worker_cannot_overwrite_replacement_receipt_or_release_its_lock() {
    let mut shell = shell("V_BOARD_CHECK");
    let old = pending(&mut shell, "true", Lease::new());
    assert!(cancel(&mut shell, old.attempt.serial));
    let replacement = pending(&mut shell, "", Lease::new());
    let intent = prepare(old.seed, old.attempt.board.as_ref().unwrap()).unwrap();
    assert!(install(&mut shell, &old.attempt, intent).is_err());
    rejected(&mut shell, &old.attempt, CHANGED, None);
    assert_eq!(
        receipt(&shell).unwrap().attempt.serial,
        replacement.attempt.serial
    );
    assert!(blocks_mutation(&shell));
}

#[tokio::test]
async fn unmount_while_worker_runs_retires_authority_without_dispatch() {
    let mut shell = shell("V_BOARD_CHECK");
    let lease = Lease::new();
    let pending = pending(&mut shell, "true", lease.clone());
    let origin = pending.attempt.board.clone().unwrap();
    let (started, start) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let worker = tokio::task::spawn_blocking(move || {
        started.send(()).unwrap();
        wait.recv().unwrap();
        prepare(pending.seed, &origin)
    });
    start.await.unwrap();
    assert!(blocks_mutation(&shell));
    lease.retire();
    assert!(poll(&mut shell));
    release.send(()).unwrap();
    let intent = worker.await.unwrap().unwrap();
    assert!(install(&mut shell, &pending.attempt, intent).is_err());
    assert!(!blocks_mutation(&shell));
    assert_eq!(fixture::creation_attempts(&shell), 0);
}

#[test]
fn sent_receipt_survives_lane_unmount_and_lost_response_never_replays() {
    let mut shell = shell("V_BOARD_CHECK");
    let lease = Lease::new();
    let attempt = ready(&mut shell, "true", lease.clone());
    let data = loaded(&shell);
    validate(&mut shell, &attempt, &data).unwrap();
    lease.retire();
    finish(
        &mut shell,
        &attempt,
        Err(Error::Transport("synthetic uncertain delivery".into())),
    );
    assert!(receipt(&shell).unwrap().stage == Stage::Uncertain);
    assert!(blocks_mutation(&shell));
    assert!(validate(&mut shell, &attempt, &data).is_err());
    assert!(!finish(&mut shell, &attempt, Ok(json!({"ok":true}))));
    assert!(receipt(&shell).unwrap().stage == Stage::Uncertain);
}

#[test]
fn malformed_or_grouped_filter_defaults_fail_before_any_dispatch() {
    let mut shell = shell("V_BOARD_CHECK");
    edit_document(&mut shell, |raw| {
        let view = raw["list_metadata"]["views"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|view| view["id"] == "V_BOARD_CHECK")
            .unwrap();
        view["filter_set"] = json!({"operator":"and","filters":[]});
    });
    let pending = pending(&mut shell, "true", Lease::new());
    assert!(prepare(pending.seed, pending.attempt.board.as_ref().unwrap()).is_err());
    rejected(&mut shell, &pending.attempt, CHANGED, None);
    assert!(!blocks_mutation(&shell));
    assert_eq!(fixture::creation_attempts(&shell), 0);
}

#[test]
fn viewer_seed_is_captured_from_scope_and_cannot_follow_a_new_authenticated_user() {
    let mut shell = shell("V_BOARD_CHECK");
    let team = shell.core.active_team.clone().unwrap();
    shell.core.workspaces.get_mut(&team).unwrap().self_user_id = "U00000001".into();
    shell.self_account.user_id = "U00000001".into();
    shell.slack_lists.scope = Scope::capture(&shell);
    edit_document(&mut shell, |raw| {
        let view = raw["list_metadata"]["views"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|view| view["id"] == "V_BOARD_CHECK")
            .unwrap();
        view["filters"] = json!([
            {"key":"assignee","column_id":"C_USER","operator":"is_me","values":["viewer"]},
            {"key":"NAME","column_id":"C_NAME","operator":"contains","values":["https://example.test/task"]}
        ]);
    });
    let pending = pending(&mut shell, "true", Lease::new());
    let intent = prepare(pending.seed, pending.attempt.board.as_ref().unwrap()).unwrap();
    let seeds = intent.initial_fields().unwrap().as_array().unwrap();
    assert_eq!(seeds[0], json!({"column_id":"C_USER","user":["U00000001"]}));
    assert_eq!(seeds[1]["column_id"], "C_NAME");
    assert!(seeds[1]["rich_text"].is_array());
    shell.core.workspaces.get_mut(&team).unwrap().self_user_id = "U00000002".into();
    shell.self_account.user_id = "U00000002".into();
    shell.slack_lists.scope = Scope::capture(&shell);
    assert!(install(&mut shell, &pending.attempt, intent).is_err());
    assert_eq!(fixture::creation_attempts(&shell), 0);
}
