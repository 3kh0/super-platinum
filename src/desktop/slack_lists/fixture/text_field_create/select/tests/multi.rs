use super::*;
fn exact(shell: &ShellState, index: usize) {
    let s = state(shell).unwrap();
    let mut opts = options();
    opts["format"] = json!("multi_select");
    oracle::exact_multi(
        &snapshot(shell).unwrap(),
        index,
        &s.document,
        &s.pages,
        &s.sync,
        &opts,
    )
    .unwrap();
}
#[test]
fn actual_multi_three_single_use_writes_preserve_reverse_order_until_schema_display() {
    let mut shell = setup_kind(true);
    let original = raw(&shell);
    let document = state(&shell).unwrap().document.clone();
    let values = state(&shell).unwrap().values.clone();
    let pages = state(&shell).unwrap().pages.clone();
    let mut generations = vec![];
    for index in 0..3 {
        let g = open(&mut shell);
        assert!(!generations.contains(&g));
        generations.push(g);
        let p = editor::presentation(&shell).unwrap();
        assert_eq!(p.kind, editor::EditorKind::MultiSelect);
        assert_eq!(
            *p.original,
            CellEditValue::Select(if index == 0 {
                vec![]
            } else {
                values[index - 1].clone()
            })
        );
        let unchanged = snapshot(&shell).unwrap();
        assert!(editor::begin_save(&mut shell, g, (*p.original).clone()).is_err());
        assert_eq!(snapshot(&shell).unwrap(), unchanged);
        let attempt =
            editor::begin_save(&mut shell, g, CellEditValue::Select(values[index].clone()))
                .unwrap();
        assert_eq!(attempt.edit.cells()[0]["select"], json!(values[index]));
        super::super::super::super::dispatch(&mut shell, attempt.clone());
        exact(&shell, index * 3 + 1);
        let held = snapshot(&shell).unwrap();
        assert!(
            editor::begin_save(&mut shell, g, CellEditValue::Select(values[index].clone()))
                .is_err()
        );
        assert_eq!(snapshot(&shell).unwrap(), held);
        super::super::super::super::complete_pending(&mut shell);
        exact(&shell, index * 3 + 2);
        assert_eq!(raw(&shell), pages[index]);
        super::super::super::super::refresh(&mut shell);
        exact(&shell, index * 3 + 3);
        assert_eq!(raw(&shell), pages[index + 1]);
        assert!(editor::presentation(&shell).is_none());
        assert!(shell.slack_lists.operations.is_empty());
        assert_eq!(shell.slack_lists.view.as_ref().unwrap().read, 0);
        let current = loaded(shell.slack_lists.fixture.as_ref().unwrap());
        assert_eq!(current.document.raw, document);
        let mut stripped = current.records.raw.clone();
        for row in stripped["records"].as_array_mut().unwrap() {
            row["fields"]
                .as_array_mut()
                .unwrap()
                .retain(|f| f["key"] != KEY);
        }
        assert_eq!(stripped, original);
        let presented = crate::slack_lists::presentation(&shell).unwrap();
        let row = presented.rows.iter().find(|r| r.id == ROW).unwrap();
        let cell = row.cells.iter().find(|c| c.column_id == COLUMN).unwrap();
        // Raw reverse order must not be normalized by the write or readback.
        assert_eq!(
            attempt
                .edit
                .readback_value(&current.document, &current.records),
            Some(CellEditValue::Select(values[index].clone()))
        );
        assert!(cell.can_edit);
    }
    let g = open(&mut shell);
    assert!(!generations.contains(&g));
    let before = snapshot(&shell).unwrap();
    assert!(editor::begin_save(&mut shell, g, CellEditValue::Select(vec![])).is_err());
    assert!(editor::cancel_shell(&mut shell, g));
    assert_eq!(snapshot(&shell).unwrap(), before);
    let s = state(&shell).unwrap();
    let mut opts = options();
    opts["format"] = json!("multi_select");
    for mutation in 0..6 {
        let mut bad = before.clone();
        match mutation {
            0 => bad["reads"][0]["readback"]["value"]
                .as_array_mut()
                .unwrap()
                .reverse(),
            1 => bad["preflights"][0]["attempt"]["cells"][0]["select"]
                .as_array_mut()
                .unwrap()
                .reverse(),
            2 => bad["server"] = pages[0].clone(),
            3 => bad["reads"][2]["readback"] = Value::Null,
            4 => bad["requests"][2]["retrySafe"] = json!(true),
            _ => {
                bad["preflights"][1]["population"]["response_metadata"]["next_cursor"] =
                    json!("changed")
            }
        }
        assert!(oracle::exact_multi(&bad, 9, &s.document, &s.pages, &s.sync, &opts).is_err());
    }
}
#[test]
fn multi_observer_refuses_retired_owner_without_mutating_held_custody() {
    let mut shell = setup_kind(true);
    let g = open(&mut shell);
    let values = state(&shell).unwrap().values.clone();
    let a = editor::begin_save(&mut shell, g, CellEditValue::Select(values[0].clone())).unwrap();
    let data = loaded(shell.slack_lists.fixture.as_ref().unwrap());
    shell.core.account_epoch += 1;
    assert!(preflight(&mut shell, &a, &data).is_err());
    assert!(snapshot(&shell).is_err());
    let s = shell
        .slack_lists
        .fixture
        .as_ref()
        .unwrap()
        .select_create
        .as_ref()
        .unwrap();
    assert!(s.preflights.is_empty() && s.requests.is_empty() && s.reads.is_empty());
}
