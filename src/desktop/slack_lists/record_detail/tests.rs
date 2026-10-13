use super::*;
use serde_json::json;

fn shell() -> ShellState {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    let mut loaded = data(&shell);
    loaded.document = Document::parse(json!({"id":"F_LIST_FIXTURE","mode":"list","title":"Detail guard fixture","access":"write","is_locked":false,"list_metadata":{
        "schema":[
            {"id":"name","key":"name","name":"Name","type":"rich_text","is_primary_column":true},
            {"id":"number","key":"number","name":"Estimate","type":"number"},
            {"id":"done","key":"done","name":"Done","type":"checkbox"},
            {"id":"teams","key":"teams","name":"Teams","type":"select","options":{"format":"multi_select","choices":[{"value":"a","label":"Alpha"},{"value":"b","label":"Beta"}]}},
            {"id":"future","key":"future","name":"Future","type":"future_type"}
        ],
        "views":[
            {"id":"table","name":"Hidden fields","type":"table","columns":[{"id":"name","key":"name","visible":true}],"filters":[{"key":"number","operator":"equal","values":[1]}]},
            {"id":"board","name":"Copies","type":"board","columns":[{"id":"name","key":"name","visible":true},{"id":"teams","key":"teams","visible":true}],"grouping":{"group_by":"teams","group_by_column_id":"teams"},"filters":[{"key":"number","operator":"equal","values":[1]}]}
        ]
    }})).unwrap();
    loaded.records = RecordsPage::parse(json!({"ok":true,"records":[
        {"id":"R1","date_created":1,"created_by":"U1","fields":[
            {"key":"name","rich_text":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Visible record","style":{"bold":true}}]}]}]},
            {"key":"number","number":[1]},{"key":"done","column_id":"done","checkbox":false},{"key":"teams","select":["a","b"]},{"key":"future","text":"Unknown but readable"}]},
        {"id":"R2","date_created":2,"created_by":"U1","fields":[{"key":"name","text":"Filtered record"},{"key":"number","number":[0]}]}
    ]})).unwrap();
    loaded.personal_task_lists = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .ok();
    read(&mut shell, loaded);
    select(&mut shell, Some("table"));
    shell
}
fn data(shell: &ShellState) -> Loaded {
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
    let attempt = begin_read(shell).unwrap();
    assert!(!finish_read(shell, &attempt, Ok(loaded)));
}
fn select(shell: &mut ShellState, selected: Option<&str>) {
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (visit, layout) = (view.generation, view.layout.generation);
    assert!(layout::select_shell(
        shell,
        selected.map(str::to_owned),
        visit,
        layout
    ));
}
fn open(shell: &mut ShellState) -> u64 {
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (instance, visit, layout) = (
        view.rows[0].instance_id.clone(),
        view.generation,
        view.layout.generation,
    );
    assert!(open_shell(shell, &instance, visit, layout));
    detail(shell).generation
}
fn detail(shell: &ShellState) -> RecordDetailView {
    super::super::presentation(shell).unwrap().detail.unwrap()
}
fn number(loaded: &Loaded, value: i64) -> Loaded {
    let mut loaded = loaded.clone();
    let mut raw = loaded.records.raw.clone();
    raw["records"][0]["fields"][1]["number"] = json!([value]);
    loaded.records = RecordsPage::parse(raw).unwrap();
    loaded
}

#[test]
fn accepted_creation_can_open_filtered_canonical_record_without_changing_view() {
    let mut shell = shell();
    assert!(
        !shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .any(|row| row.id == "R2")
    );
    assert!(open_created(&mut shell, "R2"));
    assert_eq!(detail(&shell).record_id, "R2");
    assert_eq!(
        shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .layout
            .selected
            .as_deref(),
        Some("table")
    );
    assert!(
        !open_created(&mut shell, "R1"),
        "must not replace a detail the user opened"
    );
    let generation = detail(&shell).generation;
    assert!(close_shell(&mut shell, generation));
    assert!(!open_created(&mut shell, "missing"));
    shell.slack_lists.view.as_mut().unwrap().stale = true;
    assert!(!open_created(&mut shell, "R2"));
}

#[test]
fn exact_visible_instance_grants_full_schema_only_to_detail_authority() {
    let mut shell = shell();
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (visit, layout, instance) = (
        view.generation,
        view.layout.generation,
        view.rows[0].instance_id.clone(),
    );
    assert!(!layout::cell_visible(view, "R1", "number"));
    assert!(!open_shell(&mut shell, "R2", visit, layout));
    assert!(!open_shell(&mut shell, &instance, visit + 1, layout));
    assert!(!open_shell(&mut shell, &instance, visit, layout + 1));
    assert!(editor::begin_shell(&mut shell, "R1", "number", visit).is_err());
    assert!(begin_edit(&mut shell, "R1", "done").is_err());
    let generation = open(&mut shell);
    let shown = detail(&shell);
    assert_eq!(shown.columns.len(), 5);
    assert_eq!(shown.row.cells.len(), 5);
    assert_eq!(shown.record_id, "R1");
    assert!(Arc::ptr_eq(
        &shown.row,
        &shell.slack_lists.view.as_ref().unwrap().all_rows[0]
    ));
    assert_eq!(shown.row.cells[4].text, "Unknown but readable");
    assert!(!shown.row.cells[4].can_edit);
    assert!(layout::instance_record(&shell, &instance, "name", visit, layout).is_none());
    assert!(editor::begin_shell(&mut shell, "R1", "name", visit).is_err());
    assert!(begin_editor_shell(&mut shell, generation, "future").is_err());
    assert!(begin_editor_shell(&mut shell, generation, "number").is_ok());
    let editor = editor::presentation(&shell).unwrap();
    assert_eq!(
        editor.original.as_ref(),
        &CellEditValue::Number(Some(1.into()))
    );
    let attempt = editor::begin_save(
        &mut shell,
        editor.generation,
        CellEditValue::Number(Some(3.into())),
    )
    .unwrap();
    assert_eq!(attempt.edit.record_id(), "R1");
    assert_eq!(
        attempt.edit.cells(),
        json!([{"row_id":"R1","column_id":"number","number":[3]}])
    );
    let current = data(&shell);
    assert!(validate_edit(&mut shell, &attempt, current).is_ok());
    assert_eq!(fixture::attempt_count(&shell), 0);
}

#[test]
fn closing_detail_revokes_unsent_checkbox_and_typed_draft_without_reopening_authority() {
    let mut shell = shell();
    let generation = open(&mut shell);
    let attempt = begin_checkbox_shell(&mut shell, generation, "done").unwrap();
    assert!(close_shell(&mut shell, generation));
    assert!(shell.slack_lists.operations.is_empty());
    let current = data(&shell);
    assert!(validate_edit(&mut shell, &attempt, current).is_err());
    let newer = open(&mut shell);
    assert_ne!(generation, newer);
    assert!(!close_shell(&mut shell, generation));
    assert!(begin_checkbox_shell(&mut shell, generation, "done").is_err());
    begin_editor_shell(&mut shell, newer, "teams").unwrap();
    let editor = editor::presentation(&shell).unwrap();
    let attempt = editor::begin_save(
        &mut shell,
        editor.generation,
        CellEditValue::Select(vec!["a".into()]),
    )
    .unwrap();
    select(&mut shell, Some("board"));
    assert!(shell.slack_lists.view.as_ref().unwrap().detail.is_none());
    assert!(shell.slack_lists.view.as_ref().unwrap().editor.is_none());
    assert!(shell.slack_lists.operations.is_empty());
    let current = data(&shell);
    assert!(validate_edit(&mut shell, &attempt, current).is_err());
    assert!(
        editor::begin_save(&mut shell, editor.generation, CellEditValue::Select(vec![])).is_err()
    );
}

#[test]
fn sent_detail_receipt_survives_back_and_saved_view_change_until_readback() {
    let mut shell = shell();
    let generation = open(&mut shell);
    begin_editor_shell(&mut shell, generation, "number").unwrap();
    let editor = editor::presentation(&shell).unwrap();
    let attempt = editor::begin_save(
        &mut shell,
        editor.generation,
        CellEditValue::Number(Some(2.into())),
    )
    .unwrap();
    let current = data(&shell);
    validate_edit(&mut shell, &attempt, current.clone()).unwrap();
    assert!(close_shell(&mut shell, generation));
    assert!(shell.slack_lists.view.as_ref().unwrap().editor.is_none());
    assert_eq!(shell.slack_lists.operations.len(), 1);
    select(&mut shell, None);
    finish_edit(
        &mut shell,
        &attempt,
        Err(Error::Transport("fixture lost response".into())),
    );
    assert!(
        shell
            .slack_lists
            .operations
            .values()
            .all(|mutation| mutation.phase == Phase::Uncertain)
    );
    read(&mut shell, number(&current, 2));
    assert!(shell.slack_lists.operations.is_empty());
    assert!(shell.slack_lists.view.as_ref().unwrap().detail.is_none());
    assert_eq!(
        shell.slack_lists.view.as_ref().unwrap().all_rows[0].cells[1].text,
        "2"
    );
    assert_eq!(fixture::attempt_count(&shell), 0);
}

#[test]
fn refreshed_detail_callbacks_and_open_editor_authority_are_not_reused() {
    let mut shell = shell();
    let generation = open(&mut shell);
    let stable_open = detail(&shell).open_generation;
    begin_editor_shell(&mut shell, generation, "number").unwrap();
    let draft = editor::presentation(&shell).unwrap();
    let team = shell.core.active_team.clone().unwrap();
    assert!(invalidate(&mut shell, &team, "F_LIST_FIXTURE"));
    assert!(begin_checkbox_shell(&mut shell, generation, "done").is_err());
    assert!(
        editor::begin_save(
            &mut shell,
            draft.generation,
            CellEditValue::Number(Some(2.into()))
        )
        .is_err()
    );
    let current = data(&shell);
    read(&mut shell, current);
    let refreshed = detail(&shell).generation;
    assert_ne!(refreshed, generation);
    assert_eq!(detail(&shell).open_generation, stable_open);
    assert!(!close_shell(&mut shell, generation));
    assert!(begin_checkbox_shell(&mut shell, generation, "done").is_err());
    assert!(editor::presentation(&shell).unwrap().disabled);
    assert!(editor::cancel_shell(&mut shell, draft.generation));
    assert!(begin_editor_shell(&mut shell, generation, "number").is_err());
    begin_editor_shell(&mut shell, refreshed, "number").unwrap();
    assert!(!editor::presentation(&shell).unwrap().disabled);
}

#[test]
fn missing_archived_or_recreated_record_stays_unavailable_for_this_detail_visit() {
    for mode in ["missing", "archived", "deleted", "recreated"] {
        let mut shell = shell();
        let generation = open(&mut shell);
        let original = data(&shell);
        let mut changed = original.clone();
        let mut raw = changed.records.raw.clone();
        match mode {
            "missing" => {
                raw["records"].as_array_mut().unwrap().remove(0);
            }
            "archived" => raw["records"][0]["archived"] = json!(true),
            "deleted" => raw["records"][0]["is_deleted"] = json!(true),
            "recreated" => raw["records"][0]["date_created"] = json!(999),
            _ => unreachable!(),
        }
        changed.records = RecordsPage::parse(raw).unwrap();
        read(&mut shell, changed);
        let unavailable = detail(&shell);
        assert!(unavailable.unavailable, "{mode}");
        assert_ne!(generation, unavailable.generation);
        assert!(begin_checkbox_shell(&mut shell, unavailable.generation, "done").is_err());
        read(&mut shell, original);
        assert!(
            detail(&shell).unavailable,
            "{mode} must not resurrect old authority"
        );
        assert!(begin_editor_shell(&mut shell, unavailable.generation, "number").is_err());
        let unavailable_generation = detail(&shell).generation;
        assert!(close_shell(&mut shell, unavailable_generation));
        let reopened = open(&mut shell);
        assert!(!detail(&shell).unavailable);
        assert!(begin_editor_shell(&mut shell, reopened, "number").is_ok());
    }
}

#[test]
fn duplicate_board_instances_open_one_canonical_record_without_authorizing_hidden_callbacks() {
    let mut shell = shell();
    select(&mut shell, Some("board"));
    let view = shell.slack_lists.view.as_ref().unwrap();
    assert_eq!(view.rows.len(), 2);
    let copies = [
        view.rows[0].instance_id.clone(),
        view.rows[1].instance_id.clone(),
    ];
    assert_ne!(copies[0], copies[1]);
    let (visit, layout) = (view.generation, view.layout.generation);
    assert!(!open_shell(&mut shell, "R1", visit, layout));
    assert!(open_shell(&mut shell, &copies[1], visit, layout));
    let generation = detail(&shell).generation;
    assert_eq!(detail(&shell).record_id, "R1");
    assert!(!open_shell(&mut shell, &copies[0], visit, layout));
    let attempt = begin_checkbox_shell(&mut shell, generation, "done").unwrap();
    assert_eq!(attempt.edit.record_id(), "R1");
    assert!(begin_checkbox_shell(&mut shell, generation, "done").is_err());
    assert!(close_shell(&mut shell, generation));
    assert!(open_shell(&mut shell, &copies[0], visit, layout));
    assert_ne!(detail(&shell).generation, generation);
    assert!(begin_checkbox_shell(&mut shell, generation, "done").is_err());
}

#[test]
fn permission_account_and_record_schema_changes_are_rechecked_before_dispatch() {
    let mut shell = shell();
    let generation = open(&mut shell);
    let attempt = begin_checkbox_shell(&mut shell, generation, "done").unwrap();
    let mut denied = data(&shell);
    denied.allow_lists = None;
    assert!(validate_edit(&mut shell, &attempt, denied).is_err());
    assert!(close_shell(&mut shell, generation));
    let generation = open(&mut shell);
    let attempt = begin_checkbox_shell(&mut shell, generation, "done").unwrap();
    let mut replaced = data(&shell);
    let mut raw = replaced.document.raw.clone();
    raw["list_metadata"]["schema"][2]["key"] = json!("replacement");
    replaced.document = Document::parse(raw).unwrap();
    assert!(validate_edit(&mut shell, &attempt, replaced).is_err());
    let snapshot = data(&shell);
    shell.core.account_epoch += 1;
    assert!(!close_shell(&mut shell, generation));
    assert!(validate_edit(&mut shell, &attempt, snapshot).is_err());
    assert!(begin_checkbox_shell(&mut shell, generation, "done").is_err());
    poll(&mut shell);
    assert!(shell.slack_lists.view.is_none());
    assert!(shell.slack_lists.operations.is_empty());
}
