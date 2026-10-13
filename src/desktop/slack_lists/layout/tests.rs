use super::*;
fn shell() -> ShellState {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    fixture::add_saved_views(&mut shell);
    fixture::refresh(&mut shell);
    shell
}
fn select(shell: &mut ShellState, id: Option<&str>) {
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (visit, layout) = (view.generation, view.layout.generation);
    assert!(select_shell(shell, id.map(str::to_owned), visit, layout));
}
#[test]
fn column_geometry_preserves_actual_primary_and_typed_widths_across_views() {
    let mut shell = shell();
    let view = presentation(&shell).unwrap();
    assert_eq!(
        view.columns
            .iter()
            .filter(|c| c.pinned)
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        vec!["C_NAME"]
    );
    assert_eq!(
        view.columns
            .iter()
            .find(|c| c.id == "C_CHECK")
            .unwrap()
            .size,
        "checkbox"
    );
    for id in ["C_NUMBER", "C_CURRENCY"] {
        assert_eq!(
            view.columns.iter().find(|c| c.id == id).unwrap().size,
            "number"
        );
    }
    select(&mut shell, Some("V_LIST_REORDERED_WIDE"));
    let view = presentation(&shell).unwrap();
    assert!(!view.columns[0].pinned);
    assert_eq!(view.columns[1].id, "C_NAME");
    assert!(view.columns[1].pinned);
    select(&mut shell, Some("V_LIST_NONAME"));
    assert!(
        presentation(&shell)
            .unwrap()
            .columns
            .iter()
            .all(|c| !c.pinned)
    );
}
#[test]
fn selected_columns_rows_and_unsupported_state_are_cached_and_explicit() {
    let mut shell = shell();
    let visit = generation(&shell).unwrap();
    select(&mut shell, Some("V_LIST_REORDERED"));
    let projected = presentation(&shell).unwrap();
    assert_eq!(
        projected
            .columns
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        vec!["C_SELECT", "C_NAME", "C_CHECK"]
    );
    assert_eq!(
        projected.rows[0]
            .cells
            .iter()
            .map(|c| c.column_id.as_str())
            .collect::<Vec<_>>(),
        vec!["C_SELECT", "C_NAME", "C_CHECK"]
    );
    assert!(Arc::ptr_eq(
        &projected.rows,
        &presentation(&shell).unwrap().rows
    ));
    assert!(Arc::ptr_eq(
        &projected.entries,
        &presentation(&shell).unwrap().entries
    ));
    assert_eq!(generation(&shell), Some(visit));
    assert!(super::super::begin_edit(&mut shell, "R_LIST_000", "C_NUMBER").is_err());
    select(&mut shell, Some("V_LIST_FILTERED"));
    assert_eq!(
        presentation(&shell)
            .unwrap()
            .rows
            .iter()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        vec!["R_LIST_072", "R_LIST_070", "R_LIST_068"]
    );
    select(&mut shell, Some("V_LIST_BOARD"));
    let projected = presentation(&shell).unwrap();
    assert!(projected.view_notice.is_some());
    assert!(projected.rows.is_empty());
    assert_eq!(projected.selected_view.as_deref(), Some("V_LIST_BOARD"));
    select(&mut shell, None);
    assert_eq!(presentation(&shell).unwrap().total_rows, 73);
    assert!(
        presentation(&shell)
            .unwrap()
            .rows
            .iter()
            .any(|r| r.id == "R_LIST_010" && r.has_children)
    );
}
#[test]
fn grouped_instances_collapse_and_stale_layout_actions_keep_canonical_identity() {
    let mut shell = shell();
    select(&mut shell, Some("V_LIST_GROUPED"));
    let projected = presentation(&shell).unwrap();
    let duplicates = projected
        .rows
        .iter()
        .filter(|row| row.id == "R_LIST_002")
        .collect::<Vec<_>>();
    assert_eq!(duplicates.len(), 2);
    assert_ne!(duplicates[0].instance_id, duplicates[1].instance_id);
    let instance = duplicates[0].instance_id.clone();
    let column = duplicates[0].cells[0].column_id.clone();
    assert_eq!(
        instance_record(
            &shell,
            &instance,
            &column,
            projected.generation,
            projected.layout_generation
        )
        .as_deref(),
        Some("R_LIST_002")
    );
    assert!(
        projected
            .entries
            .iter()
            .any(|entry| matches!(entry,TableEntry::Group(group) if group.count==0))
    );
    let key = projected
        .entries
        .iter()
        .find_map(|entry| match entry {
            TableEntry::Group(group) if group.count > 0 => Some(group.key.clone()),
            _ => None,
        })
        .unwrap();
    assert!(toggle_group_shell(
        &mut shell,
        &key,
        projected.generation,
        projected.layout_generation
    ));
    assert!(
        instance_record(
            &shell,
            &instance,
            &column,
            projected.generation,
            projected.layout_generation
        )
        .is_none()
    );
    assert_eq!(generation(&shell), Some(projected.generation));
}
#[test]
fn selecting_views_preserves_editor_draft_but_hidden_cell_cannot_save() {
    let mut shell = shell();
    let visit = generation(&shell).unwrap();
    editor::begin_shell(&mut shell, "R_LIST_005", "C_NUMBER", visit).unwrap();
    let draft = editor::presentation(&shell).unwrap();
    select(&mut shell, Some("V_LIST_REORDERED"));
    let hidden = editor::presentation(&shell).unwrap();
    assert_eq!(hidden.generation, draft.generation);
    assert_eq!(hidden.original, draft.original);
    assert!(hidden.disabled);
    assert!(
        editor::begin_save(
            &mut shell,
            draft.generation,
            CellEditValue::Number(Some(13.into()))
        )
        .is_err()
    );
    select(&mut shell, None);
    assert!(!editor::presentation(&shell).unwrap().disabled);
    let attempt = editor::begin_save(
        &mut shell,
        draft.generation,
        CellEditValue::Number(Some(13.into())),
    )
    .unwrap();
    let loaded = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .clone()
        .unwrap();
    select(&mut shell, Some("V_LIST_ALL"));
    assert!(validate_edit(&mut shell, &attempt, loaded.clone()).is_err());
    select(&mut shell, None);
    assert!(validate_edit(&mut shell, &attempt, loaded).is_err());
}

#[test]
fn hidden_primary_never_supplies_text_to_an_empty_visible_number() {
    let mut shell = shell();
    select(&mut shell, Some("V_LIST_NONAME"));
    let projected = presentation(&shell).unwrap();
    assert!(projected.columns.iter().all(|column| !column.primary));
    let row = projected
        .rows
        .iter()
        .find(|row| row.id == "R_LIST_020")
        .unwrap();
    assert_eq!(
        row.cells
            .iter()
            .find(|cell| cell.column_id == "C_NUMBER")
            .unwrap()
            .text,
        ""
    );
}
#[test]
fn duplicate_group_instances_share_cached_cells_and_unknown_task_policy_is_explicit() {
    let mut shell = shell();
    select(&mut shell, Some("V_LIST_GROUPED"));
    let projected = presentation(&shell).unwrap();
    let copies = projected
        .rows
        .iter()
        .filter(|row| row.id == "R_LIST_002")
        .collect::<Vec<_>>();
    assert!(Arc::ptr_eq(&copies[0].cells, &copies[1].cells));
    let mut loaded = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .clone()
        .unwrap();
    loaded.document.raw["list_metadata"]["todo_mode"] = serde_json::json!(true);
    loaded.list_experiments = None;
    let read = begin_read(&mut shell).unwrap();
    finish_read(&mut shell, &read, Ok(loaded));
    assert!(
        presentation(&shell)
            .unwrap()
            .view_notice
            .as_deref()
            .is_some_and(|notice| notice.contains("verified"))
    );
    assert!(presentation(&shell).unwrap().rows.is_empty());
    select(&mut shell, None);
    assert!(!presentation(&shell).unwrap().rows.is_empty());
}

#[test]
fn null_completed_preference_is_absent_and_internal_views_stay_out_of_picker() {
    let mut shell = shell();
    fixture::edit_backend_for_test(&mut shell, |document, _| {
        let views = document["list_metadata"]["views"].as_array_mut().unwrap();
        views
            .iter_mut()
            .find(|view| view["id"] == "V_LIST_REORDERED")
            .unwrap()["show_completed_items"] = Value::Null;
        views.push(
            serde_json::json!({"id":"V_INTERNAL_RECORD","name":"Record detail","type":"record"}),
        );
        views.push(serde_json::json!({"id":"V_INTERNAL_CANVAS","name":"Inline canvas","type":"canvas_inline"}));
    });
    fixture::refresh(&mut shell);
    let projected = presentation(&shell).unwrap();
    assert!(
        !projected
            .view_choices
            .iter()
            .any(|view| view.id.starts_with("V_INTERNAL_"))
    );
    select(&mut shell, Some("V_LIST_REORDERED"));
    let projected = presentation(&shell).unwrap();
    assert!(projected.view_notice.is_none());
    assert!(!projected.rows.is_empty());
    fixture::edit_backend_for_test(&mut shell, |document, _| {
        document["list_metadata"]["views"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|view| view["id"] == "V_LIST_REORDERED")
            .unwrap()["show_completed_items"] = serde_json::json!("false");
    });
    fixture::refresh(&mut shell);
    assert!(presentation(&shell).unwrap().view_notice.is_some());
    assert!(presentation(&shell).unwrap().rows.is_empty());
}

#[test]
fn local_group_collapse_preserves_begun_canonical_edit_but_blocks_new_hidden_gestures() {
    let mut shell = shell();
    select(&mut shell, Some("V_LIST_GROUPED"));
    let attempt = begin_edit(&mut shell, "R_LIST_004", "C_CHECK").unwrap();
    let loaded = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .clone()
        .unwrap();
    let before = presentation(&shell).unwrap();
    assert!(toggle_group_shell(
        &mut shell,
        "OPT_DESIGN",
        before.generation,
        before.layout_generation
    ));
    let after = presentation(&shell).unwrap();
    assert_ne!(before.layout_generation, after.layout_generation);
    assert!(!after.rows.iter().any(|row| row.id == "R_LIST_004"));
    assert!(validate_edit(&mut shell, &attempt, loaded).is_ok());
    shell.slack_lists.operations.clear();
    assert!(begin_edit(&mut shell, "R_LIST_004", "C_CHECK").is_err());
}

#[test]
fn board_lanes_cache_instance_cells_and_preserve_canonical_edit_targets() {
    let mut shell = shell();
    fixture::add_board_views(&mut shell).unwrap();
    fixture::refresh(&mut shell);
    select(&mut shell, Some("V_BOARD_MULTI"));
    let view = presentation(&shell).unwrap();
    assert_eq!(view.layout_kind, saved_views::ProjectionLayout::Board);
    assert_eq!(
        view.columns
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        vec!["C_NAME", "C_NUMBER", "C_CHECK"]
    );
    assert!(
        view.lanes
            .iter()
            .any(|lane| lane.count == 0 && lane.rows.is_empty())
    );
    let duplicates = view
        .rows
        .iter()
        .filter(|r| r.id == "R_LIST_002")
        .collect::<Vec<_>>();
    assert_eq!(duplicates.len(), 2);
    assert_ne!(duplicates[0].instance_id, duplicates[1].instance_id);
    assert!(Arc::ptr_eq(&duplicates[0].cells, &duplicates[1].cells));
    assert!(Arc::ptr_eq(
        &view.lanes,
        &presentation(&shell).unwrap().lanes
    ));
    assert_eq!(
        instance_record(
            &shell,
            &duplicates[0].instance_id,
            "C_CHECK",
            view.generation,
            view.layout_generation
        )
        .as_deref(),
        Some("R_LIST_002")
    );
    assert!(
        instance_record(
            &shell,
            &duplicates[0].instance_id,
            "C_MULTI",
            view.generation,
            view.layout_generation
        )
        .is_none()
    );
    let attempt = super::super::begin_edit(&mut shell, "R_LIST_002", "C_CHECK").unwrap();
    select(&mut shell, Some("V_BOARD_UNGROUPED"));
    let loaded = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .clone();
    assert!(super::super::validate_edit(&mut shell, &attempt, loaded).is_err());
    select(&mut shell, None);
    let fallback = presentation(&shell).unwrap();
    assert_eq!(fallback.layout_kind, saved_views::ProjectionLayout::Table);
    assert!(fallback.lanes.is_empty());
}

#[test]
fn board_completion_header_remains_visible_for_guarded_canonical_checkbox() {
    let mut shell = shell();
    fixture::add_board_views(&mut shell).unwrap();
    fixture::enable_board_tasks(&mut shell);
    select(&mut shell, Some("V_BOARD_TASKS"));
    let view = presentation(&shell).unwrap();
    assert_eq!(view.completion_column.as_deref(), Some("C_TODO"));
    assert!(!view.columns.iter().any(|c| c.id == "C_TODO"));
    let row = &view.rows[0];
    assert!(
        row.cells
            .iter()
            .any(|c| c.column_id == "C_TODO" && c.checkbox == Some(false))
    );
    assert_eq!(
        instance_record(
            &shell,
            &row.instance_id,
            "C_TODO",
            view.generation,
            view.layout_generation
        )
        .as_deref(),
        Some(row.id.as_str())
    );
    assert!(super::super::begin_edit(&mut shell, &row.id, "C_TODO").is_ok());
}
