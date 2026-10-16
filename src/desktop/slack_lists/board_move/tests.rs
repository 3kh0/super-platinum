use super::*;
use serde_json::json;
fn shell(checkbox: bool) -> ShellState {
    let alias = if checkbox {
        "slack-list-board-move-checkbox"
    } else {
        "slack-list-board-move"
    };
    let mut shell = ShellState::fixture_variant(crate::media::MediaRegistry::default(), alias);
    let generation = editor::presentation(&shell).unwrap().generation;
    assert!(editor::cancel_shell(&mut shell, generation));
    shell
}
fn loaded(s: &ShellState) -> Loaded {
    s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap()
}
fn open(s: &mut ShellState) -> u64 {
    let v = s.slack_lists.view.as_ref().unwrap();
    let row = v
        .layout
        .lanes
        .iter()
        .flat_map(|l| l.rows.iter())
        .find(|r| r.id == "R_LIST_000")
        .unwrap();
    let (instance, visit, layout) = (row.instance_id.clone(), v.generation, v.layout.generation);
    open_shell(s, &instance, visit, layout).unwrap();
    editor::presentation(s).unwrap().generation
}
fn read(s: &mut ShellState, data: Loaded) {
    let r = begin_read(s).unwrap();
    assert!(!finish_read(s, &r, Ok(data)));
}
fn set_field(data: &mut Loaded, key: &str, value: Option<Value>) {
    let raw = &mut data.records.raw;
    let fields = raw["records"][0]["fields"].as_array_mut().unwrap();
    fields.retain(|f| f["key"] != key);
    if let Some(value) = value {
        fields.push(value);
    }
    data.records = RecordsPage::parse(raw.clone()).unwrap();
}
fn begin(s: &mut ShellState, value: CellEditValue) -> EditAttempt {
    let generation = open(s);
    editor::begin_save(s, generation, value).unwrap()
}
fn lost(s: &mut ShellState, a: &EditAttempt) {
    let data = loaded(s);
    validate_edit(s, a, data).unwrap();
    finish_edit(s, a, Err(Error::Transport("lost".into())));
}

#[test]
fn four_named_destinations_are_disconnected_and_keep_generic_hidden_guard() {
    for alias in [
        "slack-list-board-move",
        "slack-list-board-move-narrow",
        "slack-list-board-move-checkbox",
        "slack-list-board-move-checkbox-narrow",
    ] {
        let mut s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), alias);
        assert!(s.core.session.is_none() && s.core.transport.is_none());
        assert_eq!(fixture::attempt_count(&s), 0);
        let e = editor::presentation(&s).unwrap();
        assert_eq!(e.kind, EditorKind::BoardMove);
        assert!(e.choices.iter().any(|c| c.id.is_empty()));
        let v = s.slack_lists.view.as_ref().unwrap();
        assert!(v.layout.notice.is_none());
        assert!(!layout::cell_visible(v, &e.row_id, &e.column_id));
        assert!(!EditAuthority::Layout.admits(v, &e.row_id, &e.column_id));
        assert_eq!(v.loaded.as_ref().unwrap().records.records.len(), 73);
        assert_eq!(
            v.loaded.as_ref().unwrap().records.next_cursor.as_deref(),
            Some("precision-opaque+/=")
        );
        assert!(editor::cancel_shell(&mut s, e.generation));
        assert_eq!(fixture::attempt_count(&s), 0);
    }
}
#[test]
fn explicit_select_empty_destination_uses_existing_four_field_builder_no_positions() {
    let mut s = shell(false);
    let before = loaded(&s);
    let generation = open(&mut s);
    assert!(
        editor::begin_save(
            &mut s,
            generation,
            CellEditValue::Select(vec!["OPT_PROGRESS".into()])
        )
        .is_err()
    );
    assert!(s.slack_lists.operations.is_empty());
    let a = editor::begin_save(&mut s, generation, CellEditValue::Select(vec![])).unwrap();
    assert_eq!(
        a.edit.cells(),
        json!([{"row_id":"R_LIST_000","column_id":"C_SELECT","select":[]}])
    );
    let session = super_platinum_core::config::WorkspaceSession {
        team_id: "T_FIXTURE".into(),
        user_id: "U_FIXTURE".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: String::new(),
    };
    let request =
        super_platinum_core::slack::api::list_cell_update(&s.core.client, &session, &a.edit);
    assert!(!request.retry_safe());
    let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body else {
        panic!()
    };
    assert_eq!(
        fields.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        vec!["token", "list_id", "cells", "_x_reason"]
    );
    assert_eq!(fields[2].1, serde_json::to_string(&a.edit.cells()).unwrap());
    assert_eq!(fields[3].1, "bulk-update-cells");
    assert_eq!(loaded(&s).document, before.document);
    assert_eq!(loaded(&s).records, before.records);
    assert!(editor::cancel_shell(&mut s, generation));
    assert!(validate_edit(&mut s, &a, before).is_err());
}
#[test]
fn absent_checkbox_first_true_keeps_missing_readback_uncertain_then_false_is_explicit() {
    let mut s = shell(true);
    let mut data = loaded(&s);
    set_field(&mut data, "done", None);
    read(&mut s, data.clone());
    let generation = open(&mut s);
    assert_eq!(
        *editor::presentation(&s).unwrap().original,
        CellEditValue::Boolean(false)
    );
    assert!(editor::begin_save(&mut s, generation, CellEditValue::Boolean(false)).is_err());
    assert!(
        CellEdit::capture(
            &data.document,
            &data.records,
            "R_LIST_000",
            "C_CHECK",
            data.allow_lists.as_deref(),
            data.personal_task_lists.as_ref(),
            CellEditValue::Boolean(true)
        )
        .is_err()
    );
    let a = editor::begin_save(&mut s, generation, CellEditValue::Boolean(true)).unwrap();
    lost(&mut s, &a);
    read(&mut s, data.clone());
    assert!(s.slack_lists.operations.contains_key(&data.document.id));
    assert!(s.slack_lists.view.as_ref().unwrap().editor.is_some());
    assert!(returned(&s).is_none());
    set_field(
        &mut data,
        "done",
        Some(json!({"key":"done","checkbox":true})),
    );
    read(&mut s, data);
    assert!(s.slack_lists.operations.is_empty());
    assert!(s.slack_lists.view.as_ref().unwrap().editor.is_none());
    assert!(returned(&s).unwrap().instance.is_some());
    let a = begin(&mut s, CellEditValue::Boolean(false));
    assert_eq!(a.edit.cells()[0]["checkbox"], false);
}
#[test]
fn malformed_unknown_multi_parented_or_suggested_cards_cannot_gain_board_authority() {
    for mode in 0..8 {
        let mut s = shell(false);
        let mut data = loaded(&s);
        match mode {
            0 => set_field(
                &mut data,
                "status",
                Some(json!({"key":"status","select":["UNKNOWN"]})),
            ),
            1 => set_field(
                &mut data,
                "status",
                Some(json!({"key":"status","select":["OPT_PROGRESS","OPT_DONE"]})),
            ),
            2 => set_field(
                &mut data,
                "status",
                Some(json!({"key":"status","select":"bad"})),
            ),
            3 => {
                data.document.raw["list_metadata"]["schema"][5]["options"]["format"] =
                    json!("multi_select")
            }
            4 => data.records.raw["records"][0]["parent_record_id"] = json!("R_LIST_001"),
            5 => data.records.raw["records"][0]["is_suggested"] = json!(true),
            6 => data.document.raw["list_metadata"]["views"][0]["is_all_items_view"] = json!(true),
            _ => {
                set_field(&mut data, "status", None);
                data.records.raw["records"][0]["fields"]
                    .as_array_mut()
                    .unwrap()
                    .push(
                        json!({"key":"elsewhere","column_id":"C_SELECT","select":["OPT_PROGRESS"]}),
                    );
            }
        }
        data.document = Document::parse(data.document.raw).unwrap();
        data.records = RecordsPage::parse(data.records.raw).unwrap();
        read(&mut s, data);
        let v = s.slack_lists.view.as_ref().unwrap();
        assert!(
            !admits(v, "R_LIST_000", "C_SELECT", v.layout.generation),
            "mode {mode}"
        );
        for r in v
            .layout
            .lanes
            .iter()
            .flat_map(|l| l.rows.iter())
            .filter(|r| r.id == "R_LIST_000")
        {
            assert!(!can_open(&s, &r.instance_id), "mode {mode}");
        }
        assert!(s.slack_lists.operations.is_empty());
    }
}
#[test]
fn missing_and_typed_null_select_are_empty_but_unknown_values_are_not_clearable() {
    for value in [
        None,
        Some(json!({"key":"status","select":null})),
        Some(json!({"key":"status","select":[]})),
    ] {
        let mut s = shell(false);
        let mut data = loaded(&s);
        set_field(&mut data, "status", value);
        read(&mut s, data);
        let a = begin(&mut s, CellEditValue::Select(vec!["OPT_DONE".into()]));
        assert_eq!(a.edit.cells()[0]["select"], json!(["OPT_DONE"]));
    }
}
#[test]
fn preflight_rechecks_exact_board_grouping_schema_permission_and_owner() {
    for mode in 0..5 {
        let mut s = shell(false);
        let a = begin(&mut s, CellEditValue::Select(vec![]));
        let mut data = loaded(&s);
        match mode {
            0 => {
                data.document.raw["list_metadata"]["views"][0]["grouping"]["group_by"] =
                    json!("done")
            }
            1 => {
                data.document.raw["list_metadata"]["schema"][5]["options"]["choices"][0]["label"] =
                    json!("Changed")
            }
            2 => data.allow_lists = None,
            3 => {
                s.slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .layout
                    .selection_generation = next_id()
            }
            _ => s.core.account_epoch += 1,
        }
        data.document = Document::parse(data.document.raw).unwrap();
        assert!(validate_edit(&mut s, &a, data).is_err(), "mode {mode}");
    }
}
#[test]
fn acknowledgement_never_moves_canonical_card_and_matching_read_preserves_unrelated_raw_state() {
    let mut s = shell(false);
    let before = loaded(&s);
    let a = begin(&mut s, CellEditValue::Select(vec![]));
    validate_edit(&mut s, &a, before.clone()).unwrap();
    finish_edit(&mut s, &a, Ok(json!({"ok":true,"cells":a.edit.cells()})));
    assert_eq!(loaded(&s).records, before.records);
    assert!(returned(&s).is_none());
    let generation = editor::presentation(&s).unwrap().generation;
    assert!(editor::begin_save(&mut s, generation, CellEditValue::Select(vec![])).is_err());
    let mut after = before.clone();
    set_field(
        &mut after,
        "status",
        Some(json!({"key":"status","select":[]})),
    );
    let canonical = after.records.clone();
    read(&mut s, after);
    assert!(s.slack_lists.operations.is_empty());
    assert_eq!(loaded(&s).document, before.document);
    assert_eq!(loaded(&s).records, canonical);
    assert_eq!(loaded(&s).records.next_cursor, before.records.next_cursor);
    let r = returned(&s).unwrap();
    assert!(r.instance.is_some());
    assert!(r.instance.as_ref().unwrap().contains("R_LIST_000"));
    finish_edit(&mut s, &a, Err(Error::Transport("duplicate".into())));
    assert!(s.slack_lists.operations.is_empty());
}
#[test]
fn different_read_keeps_current_notice_and_schema_replacement_cannot_resolve_receipt() {
    for changed_schema in [false, true] {
        let mut s = shell(false);
        let a = begin(&mut s, CellEditValue::Select(vec![]));
        lost(&mut s, &a);
        let mut data = loaded(&s);
        if changed_schema {
            data.document.raw["list_metadata"]["schema"][5]["options"]["choices"][0]["label"] =
                json!("Changed");
            data.document = Document::parse(data.document.raw).unwrap();
        }
        read(&mut s, data);
        assert_eq!(!s.slack_lists.operations.is_empty(), changed_schema);
        assert!(returned(&s).is_none());
        if !changed_schema {
            assert!(
                s.slack_lists
                    .view
                    .as_ref()
                    .unwrap()
                    .error
                    .as_deref()
                    .unwrap()
                    .contains("different value")
            );
        }
    }
}
#[test]
fn filter_removal_and_later_layout_revision_leave_truthful_focus_fallback() {
    let mut s = shell(false);
    let mut data = loaded(&s);
    data.document.raw["list_metadata"]["views"][0]["filters"] = json!([{"key":"status","column_id":"C_SELECT","operator":"contains","values":["OPT_PROGRESS"]}]);
    data.document = Document::parse(data.document.raw).unwrap();
    read(&mut s, data);
    s.slack_lists.view.as_mut().unwrap().layout.find.input = "Launch".into();
    s.slack_lists.view.as_mut().unwrap().layout.find.applied = "Launch".into();
    rebuild_rows(&mut s);
    let a = begin(&mut s, CellEditValue::Select(vec!["OPT_DONE".into()]));
    lost(&mut s, &a);
    let mut data = loaded(&s);
    let metadata = data.document.clone();
    set_field(
        &mut data,
        "status",
        Some(json!({"key":"status","select":["OPT_DONE"]})),
    );
    read(&mut s, data);
    assert!(returned(&s).unwrap().instance.is_none());
    let v = s.slack_lists.view.as_ref().unwrap();
    assert_eq!(v.layout.find.applied, "Launch");
    assert_eq!(v.loaded.as_ref().unwrap().document, metadata);
    s.slack_lists.view.as_mut().unwrap().layout.generation = next_id();
    assert!(returned(&s).is_none());
}

#[test]
fn missing_checkbox_is_not_malformed_or_identity_conflicting_and_options_stay_closed() {
    for mode in 0..8 {
        let mut s = shell(true);
        let mut data = loaded(&s);
        match mode {
            0 => {
                data.document.raw["list_metadata"]["schema"][1]["options"] = json!({});
                set_field(&mut data, "done", None);
            }
            1 => data.document.raw["list_metadata"]["schema"][1]["options"] = Value::Null,
            2 => {
                data.document.raw["list_metadata"]["schema"][1]["options"] = json!({"future":true})
            }
            3 => set_field(
                &mut data,
                "done",
                Some(json!({"key":"done","checkbox":null})),
            ),
            4 => set_field(
                &mut data,
                "done",
                Some(json!({"key":"done","checkbox":"false"})),
            ),
            5 => set_field(
                &mut data,
                "done",
                Some(json!({"key":"done","column_id":"WRONG","checkbox":false})),
            ),
            6 => {
                set_field(&mut data, "done", None);
                data.records.raw["records"][0]["fields"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"key":"other","column_id":"C_CHECK","checkbox":false}));
            }
            _ => data.document.raw["list_metadata"]["schema"][1]["type"] = json!("todo_completed"),
        }
        data.document = Document::parse(data.document.raw).unwrap();
        data.records = RecordsPage::parse(data.records.raw).unwrap();
        let col = data.document.column("C_CHECK").unwrap();
        let row = data.records.record("R_LIST_000").unwrap();
        assert_eq!(input(&data, row, col).is_some(), mode == 0, "mode {mode}");
        read(&mut s, data);
        let v = s.slack_lists.view.as_ref().unwrap();
        assert_eq!(
            admits(v, "R_LIST_000", "C_CHECK", v.layout.generation),
            mode == 0,
            "mode {mode}"
        );
    }
}

#[test]
fn schema_backed_empty_lane_and_existing_lane_order_need_no_local_metadata_write() {
    let mut s = shell(false);
    let mut data = loaded(&s);
    data.document.raw["list_metadata"]["views"][0]["grouping"]["order"] =
        json!([{"select":["OPT_DONE"]},{"select":["OPT_PROGRESS"]}]);
    data.document = Document::parse(data.document.raw).unwrap();
    read(&mut s, data.clone());
    let v = s.slack_lists.view.as_ref().unwrap();
    assert!(v.layout.notice.is_none());
    assert!(!v.layout.lanes.iter().any(|l| l.key.is_empty()));
    let a = begin(&mut s, CellEditValue::Select(vec![]));
    assert!(
        editor::presentation(&s)
            .unwrap()
            .choices
            .iter()
            .any(|c| c.id.is_empty())
    );
    assert_eq!(a.edit.cells()[0]["select"], json!([]));
    assert_eq!(loaded(&s).document, data.document);
}

#[test]
fn stale_read_and_new_dialog_cannot_adopt_old_move_focus_or_replay() {
    let mut s = shell(false);
    let a = begin(&mut s, CellEditValue::Select(vec![]));
    lost(&mut s, &a);
    let old = begin_read(&mut s).unwrap();
    finish_read(
        &mut s,
        &old,
        Err(Error::Offline("retired fixture read".into())),
    );
    let current = begin_read(&mut s).unwrap();
    let mut data = loaded(&s);
    set_field(
        &mut data,
        "status",
        Some(json!({"key":"status","select":[]})),
    );
    finish_read(&mut s, &old, Ok(data.clone()));
    assert!(returned(&s).is_none());
    assert!(!s.slack_lists.operations.is_empty());
    finish_read(&mut s, &current, Ok(data));
    assert!(returned(&s).is_some());
    s.slack_lists.view.as_mut().unwrap().generation = next_id();
    assert!(returned(&s).is_none());
    finish_edit(&mut s, &a, Err(Error::Transport("late".into())));
    assert!(s.slack_lists.operations.is_empty());
}

#[test]
fn matching_cell_readback_accepts_concurrent_saved_metadata_without_claiming_it_unchanged() {
    let mut s = shell(false);
    let a = begin(&mut s, CellEditValue::Select(vec!["OPT_DONE".into()]));
    lost(&mut s, &a);
    let mut data = loaded(&s);
    set_field(
        &mut data,
        "status",
        Some(json!({"key":"status","select":["OPT_DONE"]})),
    );
    data.document.raw["list_metadata"]["views"][0]["sorts"] =
        json!([{ "key":"estimate","ascending":false }]);
    data.document.raw["list_metadata"]["views"][0]["filters"] =
        json!([{ "key":"status","operator":"contains","values":["OPT_PROGRESS"] }]);
    data.document = Document::parse(data.document.raw).unwrap();
    let expected = data.document.clone();
    read(&mut s, data);
    assert!(s.slack_lists.operations.is_empty());
    assert_eq!(loaded(&s).document, expected);
    assert!(returned(&s).unwrap().instance.is_none());
}
