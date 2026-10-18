use super::*;
use serde_json::json;
fn shell(kind: &str, personal: bool) -> ShellState {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    fixture::seed(&mut shell, kind, personal);
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
fn draft_generation(shell: &ShellState) -> u64 {
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
fn ordinary_and_special_calendar_dates_use_same_input_and_explicit_task_capture() {
    for kind in ["date", "due_date", "todo_due_date"] {
        for personal in [false, true] {
            let mut shell = shell(kind, personal);
            let before = loaded(&shell);
            let generation = draft_generation(&shell);
            let shown = crate::slack_lists::presentation(&shell).unwrap();
            assert!(shown.rows[0].cells[1].can_edit);
            assert_eq!(shown.editor.unwrap().kind, EditorKind::Date);
            assert!(shell.slack_lists.operations.is_empty());
            if personal {
                assert!(
                    CellEdit::capture(
                        &before.document,
                        &before.records,
                        "R_DATE",
                        "C_DATE",
                        Some("full"),
                        before.personal_task_lists.as_ref(),
                        CellEditValue::Date(Some("2026-10-18".into()))
                    )
                    .is_err()
                );
            }
            for value in [
                CellEditValue::Date(Some("2026-02-30".into())),
                CellEditValue::Date(Some("2026-10-17".into())),
                CellEditValue::Users(vec![]),
            ] {
                assert!(begin_save(&mut shell, generation, value).is_err());
            }
            let attempt = begin_save(
                &mut shell,
                generation,
                CellEditValue::Date(Some("2026-10-18".into())),
            )
            .unwrap();
            assert_eq!(
                attempt.edit.cells(),
                json!([{"row_id":"R_DATE","column_id":"C_DATE","date":["2026-10-18"]}])
            );
            assert_eq!(loaded(&shell).records, before.records);
            assert_eq!(loaded(&shell).document.raw, before.document.raw);
            assert_eq!(crate::slack_lists::fixture::attempt_count(&shell), 0);
            assert!(cancel_shell(&mut shell, generation));
            assert!(validate_edit(&mut shell, &attempt, before.clone()).is_err());
            let visit = shell.slack_lists.view.as_ref().unwrap().generation;
            begin_shell(&mut shell, "R_DATE", "C_DATE", visit).unwrap();
            let generation = draft_generation(&shell);
            let clear = begin_save(&mut shell, generation, CellEditValue::Date(None)).unwrap();
            assert_eq!(clear.edit.cells()[0]["date"], json!([]));
            assert!(validate_edit(&mut shell, &clear, before).is_ok());
        }
    }
}
#[test]
fn task_date_preflight_refuses_policy_permission_and_page_change() {
    for change in 0..7 {
        let mut shell = shell("todo_due_date", true);
        let generation = draft_generation(&shell);
        let attempt = begin_save(&mut shell, generation, CellEditValue::Date(None)).unwrap();
        let mut fresh = loaded(&shell);
        match change {
            0 => fresh.personal_task_lists = None,
            1 => {
                fresh.personal_task_lists = PersonalTaskLists::from_response(
                    &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
                )
                .ok()
            }
            2 => fresh.allow_lists = Some("disabled".into()),
            3 => fresh.records.records[0].raw["date_created"] = json!(2),
            4 => fresh.list_experiments = None,
            5 => fresh.list_experiments.as_mut().unwrap().todos = true,
            _ => fresh.list_experiments.as_mut().unwrap().todos_rollback = true,
        }
        assert!(validate_edit(&mut shell, &attempt, fresh).is_err());
        assert_eq!(crate::slack_lists::fixture::attempt_count(&shell), 0);
    }
}
#[test]
fn primary_personal_timestamp_mode_never_opens_or_saves_calendar_cells() {
    let mut shell = shell("todo_due_date", true);
    let generation = draft_generation(&shell);
    shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap()
        .list_experiments
        .as_mut()
        .unwrap()
        .todos = true;
    assert!(begin_save(&mut shell, generation, CellEditValue::Date(None)).is_err());
    cancel_shell(&mut shell, generation);
    let visit = shell.slack_lists.view.as_ref().unwrap().generation;
    assert!(begin_shell(&mut shell, "R_DATE", "C_DATE", visit).is_err());
    let loaded = loaded(&shell);
    assert_eq!(
        mode(
            &loaded,
            &loaded.records.records[0],
            &loaded.document.columns[1]
        ),
        Ok(Mode::PersonalReminder)
    );
    assert!(shell.slack_lists.operations.is_empty());
}
#[test]
fn subtask_and_ordinary_calendar_modes_do_not_coerce_timestamps() {
    for kind in ["date", "due_date", "todo_due_date"] {
        let mut shell = shell(kind, true);
        let generation = draft_generation(&shell);
        cancel_shell(&mut shell, generation);
        let loaded = shell
            .slack_lists
            .view
            .as_mut()
            .unwrap()
            .loaded
            .as_mut()
            .unwrap();
        loaded.list_experiments.as_mut().unwrap().todos = true;
        if kind == "todo_due_date" {
            let mut raw = loaded.records.raw.clone();
            raw["records"][0]["parent_record_id"] = json!("R_PARENT");
            loaded.records = RecordsPage::parse(raw).unwrap();
        }
        let row = &loaded.records.records[0];
        let column = &loaded.document.columns[1];
        assert_eq!(mode(loaded, row, column), Ok(Mode::Calendar));
        assert_eq!(
            input(loaded, row, column).unwrap(),
            CellEditValue::Date(Some("2026-10-17".into()))
        );
        let edit = capture(loaded, "R_DATE", "C_DATE", CellEditValue::Date(None)).unwrap();
        assert_eq!(
            edit.cells(),
            json!([{"row_id":"R_DATE","column_id":"C_DATE","date":[]}])
        );
        assert!(edit.cells()[0].get("timestamp").is_none());
    }
}
#[test]
fn missing_preferences_timestamp_and_range_shapes_cannot_open_calendar_editor() {
    for field in [
        json!({"key":"due","timestamp":[0]}),
        json!({"key":"due","date":["2026-10-17"],"timestamp":[1792195200]}),
        json!({"key":"due","date":["2026-10-17","2026-10-18"]}),
        json!({"key":"due","date":["2026-10-17|2026-10-18"]}),
    ] {
        let mut shell = shell("todo_due_date", true);
        let generation = draft_generation(&shell);
        cancel_shell(&mut shell, generation);
        let loaded = shell
            .slack_lists
            .view
            .as_mut()
            .unwrap()
            .loaded
            .as_mut()
            .unwrap();
        let mut raw = loaded.records.raw.clone();
        raw["records"][0]["fields"][1] = field;
        loaded.records = RecordsPage::parse(raw).unwrap();
        rebuild_rows(&mut shell);
        let visit = shell.slack_lists.view.as_ref().unwrap().generation;
        assert!(begin_shell(&mut shell, "R_DATE", "C_DATE", visit).is_err());
        assert!(!crate::slack_lists::presentation(&shell).unwrap().rows[0].cells[1].can_edit);
        assert!(shell.slack_lists.operations.is_empty());
    }
    let mut shell = shell("due_date", true);
    let generation = draft_generation(&shell);
    cancel_shell(&mut shell, generation);
    shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap()
        .personal_task_lists = None;
    let visit = shell.slack_lists.view.as_ref().unwrap().generation;
    assert!(begin_shell(&mut shell, "R_DATE", "C_DATE", visit).is_err());
}
#[test]
fn lost_task_date_write_is_retained_until_exact_readback_without_replay() {
    let mut shell = shell("todo_due_date", true);
    let generation = draft_generation(&shell);
    let attempt = begin_save(
        &mut shell,
        generation,
        CellEditValue::Date(Some("2026-10-18".into())),
    )
    .unwrap();
    let original = loaded(&shell);
    validate_edit(&mut shell, &attempt, original.clone()).unwrap();
    cancel_shell(&mut shell, generation);
    finish_edit(
        &mut shell,
        &attempt,
        Err(Error::Transport("lost date reply".into())),
    );
    assert_eq!(shell.slack_lists.operations.len(), 1);
    close_shell(&mut shell);
    let file: File = serde_json::from_value(original.document.raw.clone()).unwrap();
    assert!(open_shell(&mut shell, file));
    // Missing row evidence cannot settle an uncertain save. A complete
    // canonical different value has a separate, deliberate settlement path.
    let mut incomplete = original.clone();
    let mut raw = incomplete.records.raw.clone();
    raw["records"] = json!([]);
    incomplete.records = RecordsPage::parse(raw).unwrap();
    assert!(
        attempt
            .edit
            .readback_value(&incomplete.document, &incomplete.records)
            .is_none()
    );
    let read = begin_read(&mut shell).unwrap();
    assert!(read.current(&shell) && read.owns_read(&shell));
    assert_eq!(read.id, original.document.id);
    finish_read(&mut shell, &read, Ok(incomplete));
    assert_eq!(shell.slack_lists.operations.len(), 1);
    let mut observed = original.clone();
    let mut raw = observed.records.raw.clone();
    raw["records"][0]["fields"][1]["date"] = json!(["2026-10-18"]);
    observed.records = RecordsPage::parse(raw).unwrap();
    assert!(
        attempt
            .edit
            .readback_matches(&observed.document, &observed.records)
    );
    let read = begin_read(&mut shell).unwrap();
    assert!(read.current(&shell) && read.owns_read(&shell));
    assert_eq!(read.id, original.document.id);
    finish_read(&mut shell, &read, Ok(observed.clone()));
    assert!(shell.slack_lists.operations.is_empty());
    assert_eq!(
        loaded(&shell).records.next_cursor,
        original.records.next_cursor
    );
    assert_eq!(loaded(&shell).document.raw, original.document.raw);
    assert_eq!(loaded(&shell).records.raw, observed.records.raw);
    assert_eq!(crate::slack_lists::fixture::attempt_count(&shell), 0);
}

#[test]
fn complete_different_task_date_read_resolves_with_notice_without_replay() {
    let mut shell = shell("todo_due_date", true);
    let generation = draft_generation(&shell);
    let attempt = begin_save(
        &mut shell,
        generation,
        CellEditValue::Date(Some("2026-10-18".into())),
    )
    .unwrap();
    let original = loaded(&shell);
    validate_edit(&mut shell, &attempt, original.clone()).unwrap();
    cancel_shell(&mut shell, generation);
    finish_edit(
        &mut shell,
        &attempt,
        Err(Error::Transport("lost date reply".into())),
    );
    assert_eq!(shell.slack_lists.operations.len(), 1);
    assert!(
        !attempt
            .edit
            .readback_matches(&original.document, &original.records)
    );
    assert!(
        attempt
            .edit
            .readback_value(&original.document, &original.records)
            .is_some()
    );
    let read = begin_read(&mut shell).unwrap();
    assert!(read.current(&shell) && read.owns_read(&shell));
    assert_eq!(read.id, original.document.id);
    finish_read(&mut shell, &read, Ok(original.clone()));
    assert!(shell.slack_lists.operations.is_empty());
    assert_eq!(
        shell.slack_lists.view.as_ref().unwrap().error.as_deref(),
        Some("Slack currently shows a different value. Review it before changing it again.")
    );
    assert_eq!(loaded(&shell).records, original.records);
    assert_eq!(loaded(&shell).document.raw, original.document.raw);
    assert_eq!(crate::slack_lists::fixture::attempt_count(&shell), 0);
}
