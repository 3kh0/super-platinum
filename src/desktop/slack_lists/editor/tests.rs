use super::*;
use serde_json::json;
fn shell() -> ShellState {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    let mut loaded = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .clone()
        .unwrap();
    loaded.document=Document::parse(json!({"id":"F_LIST_FIXTURE","mode":"list","title":"Editors","access":"write","is_locked":false,"list_metadata":{"schema":[
        {"id":"name","key":"name","name":"Item","type":"rich_text","is_primary_column":true},
        {"id":"number","key":"number","name":"Estimate","type":"number"},
        {"id":"select","key":"select","name":"Stage","type":"select","options":{"choices":[{"value":"a","label":"First"},{"value":"b","label":"Second"}]}}
    ]}})).unwrap();
    loaded.records=RecordsPage::parse(json!({"ok":true,"records":[{"id":"row","date_created":1,"created_by":"U1","fields":[
        {"key":"name","rich_text":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Original","style":{"bold":true}}]}]}]},
        {"key":"number","number":[1]}, {"key":"select","select":["a"]}
    ]}]})).unwrap();
    loaded.personal_task_lists = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .ok();
    shell.slack_lists.view.as_mut().unwrap().loaded = Some(loaded);
    super::super::rebuild_rows(&mut shell);
    shell
}
fn open(shell: &mut ShellState, column: &str) -> u64 {
    let visit = super::super::generation(shell).unwrap();
    begin_shell(shell, "row", column, visit).unwrap();
    presentation(shell).unwrap().generation
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
fn replace_number(mut loaded: Loaded, value: i64) -> Loaded {
    let mut record = loaded.records.records[0].raw.clone();
    record["fields"][1]["number"] = json!([value]);
    loaded.records = RecordsPage::parse(json!({"ok":true,"records":[record]})).unwrap();
    loaded
}
fn date_shell(kind: &str, field: Value) -> ShellState {
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
    document["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"date","key":"date","name":"Due date","type":kind}));
    data.document = Document::parse(document).unwrap();
    let mut record = data.records.records[0].raw.clone();
    record["fields"].as_array_mut().unwrap().push(field);
    data.records = RecordsPage::parse(json!({"ok":true,"records":[record]})).unwrap();
    super::super::rebuild_rows(&mut shell);
    shell
}
#[test]
fn calendar_editor_preserves_iso_seed_and_requires_explicit_set_or_clear() {
    for kind in ["date", "due_date", "todo_due_date"] {
        let mut shell = date_shell(
            kind,
            json!({"key":"date","date":["2026-10-13"],"timestamp":[-1]}),
        );
        let generation = open(&mut shell, "date");
        let view = presentation(&shell).unwrap();
        assert_eq!(view.kind, EditorKind::Date);
        assert_eq!(
            view.original.as_ref(),
            &CellEditValue::Date(Some("2026-10-13".into()))
        );
        assert!(shell.slack_lists.operations.is_empty());
        assert!(
            begin_save(
                &mut shell,
                generation,
                CellEditValue::Date(Some("2026-02-30".into()))
            )
            .is_err()
        );
        assert!(shell.slack_lists.operations.is_empty());
        let attempt = begin_save(
            &mut shell,
            generation,
            CellEditValue::Date(Some("2026-11-01".into())),
        )
        .unwrap();
        assert_eq!(attempt.edit.cells()[0]["date"], json!(["2026-11-01"]));
        assert!(cancel_shell(&mut shell, generation));
        let data = loaded(&shell);
        assert!(validate_edit(&mut shell, &attempt, data).is_err());
        let generation = open(&mut shell, "date");
        let clear = begin_save(&mut shell, generation, CellEditValue::Date(None)).unwrap();
        assert_eq!(clear.edit.cells()[0]["date"], json!([]));
        let data = loaded(&shell);
        assert!(validate_edit(&mut shell, &clear, data).is_ok());
        assert_eq!(fixture::attempt_count(&shell), 0);
    }
}
#[test]
fn date_editor_rejects_stale_or_cancelled_draft_and_timestamp_only_cells() {
    let mut shell = date_shell("date", json!({"key":"date","date":[]}));
    let generation = open(&mut shell, "date");
    assert_eq!(
        presentation(&shell).unwrap().original.as_ref(),
        &CellEditValue::Date(None)
    );
    let data = loaded(&shell);
    let read = begin_read(&mut shell).unwrap();
    finish_read(&mut shell, &read, Ok(data));
    assert!(
        begin_save(
            &mut shell,
            generation,
            CellEditValue::Date(Some("2026-10-13".into()))
        )
        .is_err()
    );
    assert!(cancel_shell(&mut shell, generation));
    let newer = open(&mut shell, "date");
    assert!(begin_save(&mut shell, generation, CellEditValue::Date(None)).is_err());
    close_shell(&mut shell);
    assert!(begin_save(&mut shell, newer, CellEditValue::Date(None)).is_err());

    let mut shell = date_shell("todo_due_date", json!({"key":"date","timestamp":[0]}));
    let visit = super::super::generation(&shell).unwrap();
    assert!(begin_shell(&mut shell, "row", "date", visit).is_err());
    assert!(presentation(&shell).is_none());
    assert!(shell.slack_lists.operations.is_empty());
}
#[test]
fn rich_draft_preserves_original_blocks_and_only_explicit_save_creates_attempt() {
    let mut shell = shell();
    let generation = open(&mut shell, "name");
    let view = presentation(&shell).unwrap();
    let CellEditValue::RichText(original) = view.original.as_ref() else {
        panic!()
    };
    assert_eq!(
        original[0]["elements"][0]["elements"][0]["style"]["bold"],
        true
    );
    assert!(shell.slack_lists.operations.is_empty());
    assert!(cancel_shell(&mut shell, generation));
    assert!(shell.slack_lists.operations.is_empty());
    assert!(
        begin_save(
            &mut shell,
            generation,
            CellEditValue::RichText(original.clone())
        )
        .is_err()
    );
    let generation = open(&mut shell, "name");
    let mut desired = original.clone();
    desired[0]["elements"][0]["elements"][0]["text"] = json!("Edited");
    let attempt = begin_save(
        &mut shell,
        generation,
        CellEditValue::RichText(desired.clone()),
    )
    .unwrap();
    assert_eq!(attempt.edit.cells()[0]["rich_text"], json!(desired));
    assert!(begin_save(&mut shell, generation, CellEditValue::RichText(desired)).is_err());
    assert_eq!(fixture::attempt_count(&shell), 0);
}
#[test]
fn stale_refresh_reopen_and_cancelled_preflight_cannot_save_draft() {
    let mut shell = shell();
    let generation = open(&mut shell, "number");
    let data = loaded(&shell);
    let read = begin_read(&mut shell).unwrap();
    finish_read(&mut shell, &read, Ok(data.clone()));
    assert!(presentation(&shell).unwrap().stale);
    assert!(
        begin_save(
            &mut shell,
            generation,
            CellEditValue::Number(Some(2.into()))
        )
        .is_err()
    );
    assert!(cancel_shell(&mut shell, generation));
    let generation = open(&mut shell, "number");
    let attempt = begin_save(
        &mut shell,
        generation,
        CellEditValue::Number(Some(2.into())),
    )
    .unwrap();
    assert!(cancel_shell(&mut shell, generation));
    assert!(validate_edit(&mut shell, &attempt, data.clone()).is_err());
    assert!(shell.slack_lists.operations.is_empty());
    let newer = open(&mut shell, "number");
    assert!(!cancel_shell(&mut shell, generation));
    assert!(
        begin_save(
            &mut shell,
            generation,
            CellEditValue::Number(Some(3.into()))
        )
        .is_err()
    );
    assert_eq!(presentation(&shell).unwrap().generation, newer);
    close_shell(&mut shell);
    assert!(begin_save(&mut shell, newer, CellEditValue::Number(Some(3.into()))).is_err());
}
#[test]
fn numeric_uncertain_receipt_survives_close_reopen_and_concrete_readback() {
    let mut shell = shell();
    let generation = open(&mut shell, "number");
    let attempt = begin_save(
        &mut shell,
        generation,
        CellEditValue::Number(Some(2.into())),
    )
    .unwrap();
    let data = loaded(&shell);
    validate_edit(&mut shell, &attempt, data.clone()).unwrap();
    assert!(cancel_shell(&mut shell, generation)); // Drops draft, not dispatched receipt.
    finish_edit(
        &mut shell,
        &attempt,
        Err(Error::Transport("lost reply".into())),
    );
    assert_eq!(shell.slack_lists.operations.len(), 1);
    close_shell(&mut shell);
    let file: File = serde_json::from_value(data.document.raw.clone()).unwrap();
    open_shell(&mut shell, file);
    let read = begin_read(&mut shell).unwrap();
    finish_read(&mut shell, &read, Ok(replace_number(data, 2)));
    assert!(shell.slack_lists.operations.is_empty());
    assert_eq!(fixture::attempt_count(&shell), 0);
    assert_eq!(
        shell.slack_lists.view.as_ref().unwrap().rows[0].cells[1].text,
        "2"
    );
}
#[test]
fn select_changes_revalidate_permission_and_schema_before_dispatch() {
    let mut shell = shell();
    let generation = open(&mut shell, "select");
    assert_eq!(presentation(&shell).unwrap().choices[1].label, "Second");
    assert!(
        begin_save(
            &mut shell,
            generation,
            CellEditValue::Select(vec!["new-option".into()])
        )
        .is_err()
    );
    assert!(shell.slack_lists.operations.is_empty());
    let attempt = begin_save(
        &mut shell,
        generation,
        CellEditValue::Select(vec!["b".into()]),
    )
    .unwrap();
    let mut data = loaded(&shell);
    data.allow_lists = None;
    assert!(validate_edit(&mut shell, &attempt, data).is_err());
    cancel_preflight(&mut shell, &attempt, "Permission changed");
    assert!(shell.slack_lists.operations.is_empty());
    assert!(presentation(&shell).unwrap().stale);
}
#[test]
fn acknowledged_readback_closes_only_own_editor_and_keeps_cached_rows_until_read() {
    let mut shell = shell();
    let generation = open(&mut shell, "number");
    let before = shell.slack_lists.view.as_ref().unwrap().rows.clone();
    let data = loaded(&shell);
    let attempt = begin_save(
        &mut shell,
        generation,
        CellEditValue::Number(Some(2.into())),
    )
    .unwrap();
    validate_edit(&mut shell, &attempt, data.clone()).unwrap();
    finish_edit(
        &mut shell,
        &attempt,
        Ok(json!({"ok":true,"cells":attempt.edit.cells()})),
    );
    assert!(Arc::ptr_eq(
        &before,
        &shell.slack_lists.view.as_ref().unwrap().rows
    ));
    assert!(presentation(&shell).is_some());
    let read = begin_read(&mut shell).unwrap();
    finish_read(&mut shell, &read, Ok(replace_number(data, 2)));
    assert!(presentation(&shell).is_none());
    assert!(shell.slack_lists.operations.is_empty());
}

#[test]
fn select_schema_multi_format_opens_multi_editor_and_keeps_multiple_choices() {
    let mut shell = shell();
    let data = shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    let mut raw = data.document.raw.clone();
    raw["list_metadata"]["schema"][2]["options"]["format"] = json!("multi_select");
    data.document = Document::parse(raw).unwrap();
    let mut record = data.records.records[0].raw.clone();
    record["fields"][2]["select"] = json!(["a", "b"]);
    data.records = RecordsPage::parse(json!({"ok":true,"records":[record]})).unwrap();
    super::super::rebuild_rows(&mut shell);
    let generation = open(&mut shell, "select");
    let view = presentation(&shell).unwrap();
    assert_eq!(view.kind, EditorKind::MultiSelect);
    assert_eq!(
        view.original.as_ref(),
        &CellEditValue::Select(vec!["a".into(), "b".into()])
    );
    let attempt = begin_save(
        &mut shell,
        generation,
        CellEditValue::Select(vec!["b".into()]),
    )
    .unwrap();
    assert_eq!(attempt.edit.cells()[0]["select"], json!(["b"]));
    let data = loaded(&shell);
    assert!(validate_edit(&mut shell, &attempt, data).is_ok());
}

#[test]
fn save_outcome_copy_takes_precedence_over_generic_external_stale_notice() {
    let mut shell = shell();
    let generation = open(&mut shell, "number");
    let data = loaded(&shell);
    let attempt = begin_save(
        &mut shell,
        generation,
        CellEditValue::Number(Some(2.into())),
    )
    .unwrap();
    validate_edit(&mut shell, &attempt, data.clone()).unwrap();
    finish_edit(
        &mut shell,
        &attempt,
        Err(Error::Transport("lost reply".into())),
    );
    assert!(
        presentation(&shell)
            .unwrap()
            .error
            .unwrap()
            .contains("save is unconfirmed")
    );
    let read = begin_read(&mut shell).unwrap();
    assert!(
        presentation(&shell)
            .unwrap()
            .error
            .unwrap()
            .contains("Checking this save")
    );
    finish_read(&mut shell, &read, Ok(replace_number(data, 3)));
    assert!(
        presentation(&shell)
            .unwrap()
            .error
            .unwrap()
            .contains("different value")
    );
    assert!(cancel_shell(&mut shell, generation));
    let generation = open(&mut shell, "number");
    let team = shell.core.active_team.clone().unwrap();
    invalidate(&mut shell, &team, "F_LIST_FIXTURE");
    assert!(
        presentation(&shell)
            .unwrap()
            .error
            .unwrap()
            .contains("changed while the editor was open")
    );
    assert!(cancel_shell(&mut shell, generation));
}

#[test]
fn new_rich_cell_server_block_id_confirms_readback_and_closes_editor() {
    let mut shell = shell();
    let mut data = loaded(&shell);
    let mut raw = data.records.records[0].raw.clone();
    raw["fields"]
        .as_array_mut()
        .unwrap()
        .retain(|field| field["key"] != "name");
    data.records = RecordsPage::parse(json!({"ok":true,"records":[raw]})).unwrap();
    shell.slack_lists.view.as_mut().unwrap().loaded = Some(data.clone());
    super::super::rebuild_rows(&mut shell);
    let generation = open(&mut shell, "name");
    let blocks = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Native editor verification","style":{"bold":true}}]}]}),
    ];
    let attempt = begin_save(
        &mut shell,
        generation,
        CellEditValue::RichText(blocks.clone()),
    )
    .unwrap();
    validate_edit(&mut shell, &attempt, data.clone()).unwrap();
    let mut received = blocks.clone();
    received[0]["block_id"] = json!("bfz4L");
    finish_edit(
        &mut shell,
        &attempt,
        Ok(json!({"ok":true,"cells":[{"row_id":"row","column_id":"name","rich_text":received}]})),
    );
    assert!(presentation(&shell).is_some());
    let mut raw = data.records.records[0].raw.clone();
    raw["fields"].as_array_mut().unwrap().push(json!({"key":"name","column_id":"name","rich_text":received,"text":"Native editor verification"}));
    data.records = RecordsPage::parse(json!({"ok":true,"records":[raw]})).unwrap();
    let read = begin_read(&mut shell).unwrap();
    finish_read(&mut shell, &read, Ok(data));
    assert!(presentation(&shell).is_none());
    assert!(shell.slack_lists.operations.is_empty());
    assert_eq!(attempt.edit.cells()[0]["rich_text"], json!(blocks));
}
