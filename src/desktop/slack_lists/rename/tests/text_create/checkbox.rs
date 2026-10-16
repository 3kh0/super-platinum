use super::*;
use super_platinum_core::slack_lists::{CellValue, ColumnCreateKind};

fn kind(s: &ShellState, serial: u64, value: &str) -> bool {
    text_create::kind_shell(
        s,
        serial,
        generation(s).unwrap(),
        super::super::super::selection(s).unwrap(),
        text_create::choice(s).unwrap(),
        value,
    )
}
fn checkbox_addition(s: &ShellState, options: Option<Value>) -> network::Metadata {
    let mut m = addition(s, true);
    let column = &mut m.document.raw["list_metadata"]["schema"][1];
    column["type"] = json!("checkbox");
    column.as_object_mut().unwrap().remove("options");
    if let Some(options) = options {
        column["options"] = options;
    }
    m.document = Document::parse(m.document.raw).unwrap();
    m
}
fn begin(s: &mut ShellState) -> (Guard, Intent) {
    let serial = open(s);
    assert!(kind(s, serial, "checkbox"));
    assert!(name(s, serial, "Release notes"));
    prepare(s, serial).unwrap()
}

#[test]
fn checkbox_type_epoch_preserves_name_and_refuses_stale_name_choice_and_aba() {
    let (mut s, _cleanup) = text_shell();
    let serial = open(&mut s);
    assert!(name(&s, serial, "Verified"));
    let visit = generation(&s).unwrap();
    let selection = super::super::super::selection(&s).unwrap();
    let text = text_create::choice(&s).unwrap();
    assert!(kind(&s, serial, "checkbox"));
    let checkbox = text_create::choice(&s).unwrap();
    assert_eq!(checkbox.kind, ColumnCreateKind::Checkbox);
    assert_eq!(
        entry(&s).unwrap().input,
        Input::NewTextField("Verified".into())
    );
    assert!(!text_create::input_shell(
        &s,
        serial,
        visit,
        selection,
        text,
        "old Text".into()
    ));
    assert!(kind(&s, serial, "number"));
    assert!(!text_create::kind_shell(
        &s, serial, visit, selection, checkbox, "text"
    ));
    assert!(kind(&s, serial, "checkbox"));
    let current = text_create::choice(&s).unwrap();
    assert_ne!(current.revision, checkbox.revision);
    assert!(!text_create::input_shell(
        &s,
        serial,
        visit,
        selection,
        checkbox,
        "old Checkbox".into()
    ));
    assert!(text_create::input_shell(
        &s,
        serial,
        visit,
        selection,
        current,
        "V".into()
    ));
    assert!(text_create::input_shell(
        &s,
        serial,
        visit,
        selection,
        current,
        "Verified".into()
    ));
    for invalid in ["Checkbox", "checkbox ", "completed", "todo_completed"] {
        assert!(!kind(&s, serial, invalid));
    }
    assert!(close_entry(&s, serial));
    open(&mut s);
    assert_eq!(
        text_create::choice(&s).unwrap().kind,
        ColumnCreateKind::Text
    );
    assert_eq!(entry(&s).unwrap().input, Input::NewTextField(String::new()));
}

#[test]
fn checkbox_create_uses_six_fields_and_keeps_type_name_and_permit_fixed_after_admission() {
    let (mut s, _cleanup) = text_shell();
    let (g, i) = begin(&mut s);
    let choice = text_create::choice(&s).unwrap();
    let Intent::NewTextField(intent) = &i else {
        panic!()
    };
    assert_eq!(intent.kind(), ColumnCreateKind::Checkbox);
    assert_eq!(intent.kind().options(), None);
    assert!(!kind(&s, g.serial, "number"));
    assert!(!name(&s, g.serial, "Other"));
    let permit = admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
    let RequestBody::Form(fields) = permit.request(&s.core.client, &fixture::session()).body else {
        panic!()
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "fixture-not-a-token".into()),
            ("list_id".into(), "FL1".into()),
            ("view_id".into(), "V_TEXT".into()),
            ("name".into(), "Release notes".into()),
            ("type".into(), "checkbox".into()),
            ("_x_reason".into(), "create-column".into()),
        ]
    );
    assert!(admitted(&s, &g, &i, &metadata(&s, None)).is_none());
    completed(
        &mut s,
        &g,
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "lost".into(),
        ))),
    );
    assert!(!kind(&s, g.serial, "text"));
    assert!(!name(&s, g.serial, "Other"));
    assert_eq!(text_create::choice(&s), Some(choice));
    assert!(prepare(&mut s, g.serial).is_none());
}

#[test]
fn observed_checkbox_keeps_options_variant_all_raw_rows_and_requires_explicit_first_toggle() {
    for options in [None, Some(json!({}))] {
        let (mut s, _cleanup) = text_shell();
        let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
        let rows = order(&s);
        let (g, i) = begin(&mut s);
        admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
        completed(&mut s, &g, Ok(json!({"ok":true})));
        assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
        let check = checking(&s);
        let fresh = checkbox_addition(&s, options.clone());
        assert!(checked(&mut s, &check, Ok(fresh)));
        let e = entry(&s).unwrap();
        assert_eq!(e.stage, Stage::Observed);
        let field = e.observed_field.unwrap();
        assert_eq!(order(&s), rows);
        let loaded = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
        assert_eq!(loaded.records, before.records);
        let column = loaded.document.column(&field.id).unwrap();
        assert_eq!(column.raw.get("options"), options.as_ref());
        assert!(loaded.records.records.iter().all(
            |row| column.cell(row) == CellValue::Empty && !row.fields.contains_key(&field.key)
        ));
        let p = crate::slack_lists::presentation(&s).unwrap();
        assert_eq!(p.find_applied, "Item 0");
        assert_eq!(p.find_revision, 17);
        for item in p.entries.iter() {
            if let crate::slack_lists::layout::TableEntry::Row(row) = item {
                let cell = row.cells.iter().find(|c| c.column_id == field.id).unwrap();
                assert_eq!(cell.checkbox, Some(false));
                assert!(cell.editable);
                assert!(!cell.can_edit); // Generic typed editor still refuses a missing bool.
            }
        }
        assert!(crate::slack_lists::begin_edit(&mut s, &rows[0], &field.id).is_err());
        assert!(s.slack_lists.operations.is_empty());
        assert!(close_entry(&s, g.serial));
        let attempt = crate::slack_lists::begin_edit(&mut s, &rows[0], &field.id).unwrap();
        assert_eq!(
            attempt.edit.cells(),
            json!([{"row_id":rows[0],"column_id":field.id,"checkbox":true}])
        );
        assert!(crate::slack_lists::validate_edit(&mut s, &attempt, loaded.clone()).is_ok());
        assert_eq!(
            s.slack_lists
                .view
                .as_ref()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap()
                .records,
            before.records
        );
    }
}

#[test]
fn unsupported_checkbox_options_remain_conflict_without_projection_or_replay() {
    for options in [
        json!(null),
        json!({"markAsDoneWhenChecked":true}),
        json!([]),
    ] {
        let (mut s, _cleanup) = text_shell();
        let before = s
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records
            .clone();
        let (g, i) = begin(&mut s);
        admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
        completed(&mut s, &g, Ok(json!({"ok":true})));
        let check = checking(&s);
        let m = checkbox_addition(&s, Some(options));
        assert!(checked(&mut s, &check, Ok(m)));
        assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
        assert!(entry(&s).unwrap().observed_field.is_none());
        assert!(prepare(&mut s, g.serial).is_none());
        assert_eq!(
            s.slack_lists
                .view
                .as_ref()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap()
                .records,
            before
        );
    }
}
