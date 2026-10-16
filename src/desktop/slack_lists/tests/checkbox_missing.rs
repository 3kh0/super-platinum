use super::*;
use super_platinum_core::slack_lists::CellValue;

fn missing(options: Option<Value>) -> Loaded {
    let mut data = loaded(false);
    let mut raw = data.records.raw.clone();
    raw["records"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .retain(|field| field["key"] != "done");
    data.records = RecordsPage::parse(raw).unwrap();
    if let Some(options) = options {
        let mut raw = data.document.raw.clone();
        raw["list_metadata"]["schema"][1]["options"] = options;
        data.document = Document::parse(raw).unwrap();
    }
    data
}
fn present(data: &Loaded, field: Value) -> Loaded {
    let mut data = data.clone();
    let mut raw = data.records.raw.clone();
    raw["records"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .push(field);
    data.records = RecordsPage::parse(raw).unwrap();
    data
}
fn projected(shell: &ShellState) -> CellView {
    presentation(shell).unwrap().rows[0]
        .cells
        .iter()
        .find(|c| c.column_id == "done")
        .unwrap()
        .clone()
}

#[test]
fn missing_checkbox_is_input_only_while_readonly_and_malformed_cells_keep_existing_presentation() {
    for options in [None, Some(json!({}))] {
        let mut shell = make_shell();
        let data = missing(options);
        let original = data.records.clone();
        read(&mut shell, data.clone());
        let p = projected(&shell);
        assert_eq!(p.checkbox, Some(false));
        assert!(p.editable && !p.can_edit);
        assert_eq!(
            data.document
                .column("done")
                .unwrap()
                .cell(&data.records.records[0]),
            CellValue::Empty
        );
        assert_eq!(
            shell
                .slack_lists
                .view
                .as_ref()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap()
                .records,
            original
        );
        for readonly in ["read", "none"] {
            let mut denied = data.clone();
            let mut raw = denied.document.raw.clone();
            raw["access"] = json!(readonly);
            denied.document = Document::parse(raw).unwrap();
            read(&mut shell, denied.clone());
            assert_eq!(projected(&shell).checkbox, None);
            assert!(!projected(&shell).editable);
            assert!(begin_edit(&mut shell, "row1", "done").is_err());
            for value in [false, true] {
                read(
                    &mut shell,
                    present(&denied, json!({"key":"done","checkbox":value})),
                );
                assert_eq!(projected(&shell).checkbox, Some(value));
                assert!(!projected(&shell).editable);
            }
        }
        for field in [
            json!({"key":"done"}),
            json!({"key":"done","checkbox":null}),
            json!({"key":"done","checkbox":"false"}),
            json!({"key":"done","checkbox":false,"column_id":"WRONG"}),
            json!({"key":"done","checkbox":false,"row_id":"OTHER"}),
            json!({"key":"different_key","column_id":"done","checkbox":false}),
        ] {
            read(&mut shell, present(&data, field));
            assert!(!projected(&shell).editable);
            assert!(begin_edit(&mut shell, "row1", "done").is_err());
        }
    }
    for (kind, options) in [
        ("completed", None),
        ("todo_completed", None),
        ("checkbox", Some(json!(null))),
        ("checkbox", Some(json!({"markAsDoneWhenChecked":true}))),
    ] {
        let mut shell = make_shell();
        let mut data = missing(options);
        let mut raw = data.document.raw.clone();
        raw["list_metadata"]["schema"][1]["type"] = json!(kind);
        data.document = Document::parse(raw).unwrap();
        read(&mut shell, data);
        assert_eq!(projected(&shell).checkbox, None);
        assert!(!projected(&shell).editable);
        assert!(begin_edit(&mut shell, "row1", "done").is_err());
    }
}

#[test]
fn missing_readback_cannot_resolve_first_toggle_but_actual_true_then_explicit_false_can() {
    fn observed_read(shell: &mut ShellState, data: Loaded, previous: u64) -> u64 {
        let attempt = begin_read(shell).expect("actual guarded read admission");
        assert!(attempt.read > previous && attempt.read > 0);
        let view = shell.slack_lists.view.as_ref().unwrap();
        assert!(view.loading);
        assert_eq!(view.read, attempt.read);
        assert!(!finish_read(shell, &attempt, Ok(data.clone())));
        let view = shell.slack_lists.view.as_ref().unwrap();
        assert_eq!(view.read, 0, "completion consumes the active read ID");
        assert!(!view.loading);
        assert!(view.revision > attempt.revision);
        assert_eq!(view.loaded.as_ref().unwrap().records, data.records);
        let revision = view.revision;
        assert!(!finish_read(shell, &attempt, Ok(data)));
        let view = shell.slack_lists.view.as_ref().unwrap();
        assert_eq!(view.read, 0);
        assert_eq!(
            view.revision, revision,
            "duplicate completion cannot reapply"
        );
        attempt.read
    }
    let mut shell = make_shell();
    let data = missing(Some(json!({})));
    read(&mut shell, data.clone());
    let first = begin_edit(&mut shell, "row1", "done").unwrap();
    assert_eq!(
        first.edit.cells(),
        json!([{"row_id":"row1","column_id":"done","checkbox":true}])
    );
    validate_edit(&mut shell, &first, data.clone()).unwrap();
    finish_edit(
        &mut shell,
        &first,
        Ok(json!({"ok":true,"cells":[{"row_id":"row1","column_id":"done","checkbox":true}]})),
    );
    assert!(shell.slack_lists.operations.contains_key("FL1"));
    let missing_read = observed_read(&mut shell, data.clone(), 0);
    assert!(shell.slack_lists.operations["FL1"].phase == Phase::Uncertain);
    assert_eq!(
        first.edit.readback_value(&data.document, &data.records),
        None
    );
    assert_eq!(projected(&shell).checkbox, Some(false));
    assert!(begin_edit(&mut shell, "row1", "done").is_err());
    let yes = present(
        &data,
        json!({"key":"done","row_id":"row1","column_id":"done","checkbox":true}),
    );
    let true_read = observed_read(&mut shell, yes.clone(), missing_read);
    assert!(shell.slack_lists.operations.is_empty());
    assert_eq!(projected(&shell).checkbox, Some(true));
    let second = begin_edit(&mut shell, "row1", "done").unwrap();
    assert_ne!(first.serial, second.serial);
    assert_eq!(
        second.edit.cells(),
        json!([{"row_id":"row1","column_id":"done","checkbox":false}])
    );
    validate_edit(&mut shell, &second, yes).unwrap();
    finish_edit(&mut shell, &second, Err(Error::Transport("lost".into())));
    let no = present(
        &data,
        json!({"key":"done","column_id":"done","checkbox":false}),
    );
    observed_read(&mut shell, no, true_read);
    assert!(shell.slack_lists.operations.is_empty());
    assert_eq!(projected(&shell).checkbox, Some(false));
}

#[test]
fn missing_first_toggle_refuses_changed_raw_preflight_and_stale_owner() {
    for owner in [false, true] {
        let mut shell = make_shell();
        let data = missing(None);
        read(&mut shell, data.clone());
        let first = begin_edit(&mut shell, "row1", "done").unwrap();
        let fresh = if owner {
            shell.core.account_epoch += 1;
            data.clone()
        } else {
            present(&data, json!({"key":"done","checkbox":false}))
        };
        assert!(validate_edit(&mut shell, &first, fresh).is_err());
        assert!(shell.slack_lists.operations["FL1"].phase == Phase::Preparing);
        assert_eq!(
            shell
                .slack_lists
                .view
                .as_ref()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap()
                .records,
            data.records
        );
    }
}

#[test]
fn table_board_and_detail_share_missing_input_and_detail_authority_without_backfill() {
    let mut shell = make_shell();
    let mut data = missing(None);
    let mut raw = data.document.raw.clone();
    raw["list_metadata"]["schema"].as_array_mut().unwrap().push(json!({"id":"status","key":"status","name":"Status","type":"select","options":{"format":"single_select","choices":[{"value":"a","label":"Alpha"}]}}));
    let columns = json!([{"id":"name","key":"name","visible":true},{"id":"done","key":"done","visible":true},{"id":"status","key":"status","visible":true}]);
    raw["list_metadata"]["views"] = json!([{"id":"table","name":"Table","type":"table","columns":columns},{"id":"board","name":"Board","type":"board","columns":columns,"grouping":{"group_by":"status","group_by_column_id":"status"}}]);
    data.document = Document::parse(raw).unwrap();
    let mut raw = data.records.raw.clone();
    raw["records"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":"status","select":["a"]}));
    data.records = RecordsPage::parse(raw).unwrap();
    read(&mut shell, data.clone());
    for id in ["table", "board"] {
        let v = shell.slack_lists.view.as_ref().unwrap();
        let (visit, layout) = (v.generation, v.layout.generation);
        assert!(layout::select_shell(
            &mut shell,
            Some(id.into()),
            visit,
            layout
        ));
        let p = presentation(&shell).unwrap();
        assert_eq!(p.selected_view.as_deref(), Some(id));
        assert!(p.view_notice.is_none());
        let cell = if id == "table" {
            p.rows[0]
                .cells
                .iter()
                .find(|c| c.column_id == "done")
                .unwrap()
        } else {
            p.lanes[0].rows[0]
                .cells
                .iter()
                .find(|c| c.column_id == "done")
                .unwrap()
        };
        assert_eq!(cell.checkbox, Some(false));
        assert!(cell.editable);
    }
    let p = presentation(&shell).unwrap();
    let instance = p.lanes[0].rows[0].instance_id.clone();
    assert!(record_detail::open_shell(
        &mut shell,
        &instance,
        p.generation,
        p.layout_generation
    ));
    let detail = presentation(&shell).unwrap().detail.unwrap();
    assert_eq!(
        detail
            .row
            .cells
            .iter()
            .find(|c| c.column_id == "done")
            .unwrap()
            .checkbox,
        Some(false)
    );
    assert!(begin_edit(&mut shell, "row1", "done").is_err());
    let attempt =
        record_detail::begin_checkbox_shell(&mut shell, detail.generation, "done").unwrap();
    assert_eq!(
        attempt.edit.cells(),
        json!([{"row_id":"row1","column_id":"done","checkbox":true}])
    );
    validate_edit(&mut shell, &attempt, data.clone()).unwrap();
    assert_eq!(
        shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records,
        data.records
    );
}
