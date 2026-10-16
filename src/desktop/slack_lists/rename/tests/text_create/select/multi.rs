use super::*;
use super_platinum_core::slack_lists::{CellPresentation, SelectCreateFormat};

fn set_format(s: &ShellState, serial: u64, format: SelectCreateFormat) {
    assert!(edit(s, serial, Edit::Format(format)));
}
fn observed_multi(s: &mut ShellState) -> (Vec<String>, String) {
    let serial = start(s);
    add(s, serial, "Ready");
    add(s, serial, "\u{c9}quipe \u{6771}\u{4eac}");
    set_format(s, serial, SelectCreateFormat::Multiple);
    let options = entry(s).unwrap().select_draft.capture().unwrap().wire();
    let ids = options["choices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["value"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let (g, i) = prepare(s, serial).unwrap();
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let permit = admitted(s, &g, &i, &metadata(s, None)).unwrap();
    let RequestBody::Form(fields) = permit.request(&s.core.client, &fixture::session()).body else {
        panic!()
    };
    assert_eq!(fields.len(), 7);
    assert_eq!(fields[4], ("type".into(), "select".into()));
    assert_eq!(
        serde_json::from_str::<Value>(&fields[5].1).unwrap(),
        options
    );
    assert!(!edit(s, serial, Edit::Format(SelectCreateFormat::Single)));
    assert!(admitted(s, &g, &i, &metadata(s, None)).is_none());
    completed(s, &g, Ok(json!({"ok":true})));
    assert_eq!(entry(s).unwrap().stage, Stage::Uncertain);
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .document,
        before.document
    );
    let mut fresh = addition(s, true);
    fresh.document.raw["list_metadata"]["schema"][1]["type"] = json!("select");
    fresh.document.raw["list_metadata"]["schema"][1]["options"] = options.clone();
    fresh.document = Document::parse(fresh.document.raw).unwrap();
    let mut wrong = network::Metadata {
        document: fresh.document.clone(),
        allow: fresh.allow.clone(),
        personal: fresh.personal.clone(),
    };
    wrong.document.raw["list_metadata"]["schema"][1]["options"]["format"] = json!("single_select");
    wrong.document = Document::parse(wrong.document.raw).unwrap();
    let check = checking(s);
    assert!(checked(s, &check, Ok(wrong)));
    assert_eq!(entry(s).unwrap().stage, Stage::Conflict);
    let check = checking(s);
    assert!(checked(s, &check, Ok(fresh)));
    assert_eq!(entry(s).unwrap().stage, Stage::Observed);
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
    assert!(close_entry(s, serial));
    (ids, order(s)[0].clone())
}

#[test]
fn cardinality_aba_retires_all_callbacks_but_preserves_options_and_pending_invalid_text() {
    let (mut s, _cleanup) = text_shell();
    let serial = start(&mut s);
    add(&s, serial, "Ready");
    let old = entry(&s).unwrap();
    let visit = generation(&s).unwrap();
    let selection = crate::slack_lists::rename::selection(&s).unwrap();
    assert!(edit(
        &s,
        serial,
        Edit::Pending {
            epoch: old.select_draft.add_epoch,
            value: "bad\nlabel".into()
        }
    ));
    set_format(&s, serial, SelectCreateFormat::Multiple);
    let multi = entry(&s).unwrap();
    assert_eq!(multi.select_draft.pending, "bad\nlabel");
    assert_eq!(
        multi.select_draft.rows[0].generation,
        old.select_draft.rows[0].generation
    );
    assert_eq!(
        multi.select_draft.rows[0].option,
        old.select_draft.rows[0].option
    );
    assert!(!multi.select_draft.ready());
    assert!(prepare(&mut s, serial).is_none());
    set_format(&s, serial, SelectCreateFormat::Single);
    assert_ne!(entry(&s).unwrap().field_choice, old.field_choice);
    for action in [
        Edit::Format(SelectCreateFormat::Multiple),
        Edit::Add {
            epoch: old.select_draft.add_epoch,
        },
        Edit::Remove {
            row: old.select_draft.rows[0].generation,
        },
        Edit::Pending {
            epoch: old.select_draft.add_epoch,
            value: "late".into(),
        },
    ] {
        assert!(!edit_shell(
            &s,
            serial,
            visit,
            selection,
            old.field_choice,
            action
        ));
    }
    assert!(!text_create::input_shell(
        &s,
        serial,
        visit,
        selection,
        old.field_choice,
        "Late name".into()
    ));
    assert_eq!(entry(&s).unwrap().select_draft.pending, "bad\nlabel");
    assert!(close_entry(&s, serial));
    assert!(entry(&s).is_none());
    assert!(s.slack_lists.operations.is_empty());
}

#[test]
fn observed_multi_uses_existing_editor_for_two_reverse_ids_remove_one_and_clear() {
    use crate::slack_lists as lists;
    let (mut s, _cleanup) = text_shell();
    let original = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let order_before = order(&s);
    let (ids, row) = observed_multi(&mut s);
    let desired = vec![ids[1].clone(), ids[0].clone()];
    for values in [desired.clone(), vec![ids[1].clone()], vec![]] {
        let visit = generation(&s).unwrap();
        lists::editor::begin_shell(&mut s, &row, "ACTUAL_SERVER_FIELD", visit).unwrap();
        let editor = lists::editor::presentation(&s).unwrap();
        assert_eq!(editor.kind, lists::editor::EditorKind::MultiSelect);
        assert_eq!(
            editor
                .choices
                .iter()
                .map(|c| c.id.clone())
                .collect::<Vec<_>>(),
            ids
        );
        let attempt = lists::editor::begin_save(
            &mut s,
            editor.generation,
            CellEditValue::Select(values.clone()),
        )
        .unwrap();
        let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
        lists::validate_edit(&mut s, &attempt, before.clone()).unwrap();
        lists::finish_edit(&mut s, &attempt, Ok(json!({"ok":true})));
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
        let mut loaded = before;
        let mut raw = loaded.records.raw.clone();
        let fields = raw["records"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["id"] == row)
            .unwrap()["fields"]
            .as_array_mut()
            .unwrap();
        fields.retain(|f| f["key"] != "actual_server_key");
        fields.push(json!({"key":"actual_server_key","column_id":"ACTUAL_SERVER_FIELD","row_id":row,"select":values}));
        loaded.records = RecordsPage::parse(raw).unwrap();
        let read = lists::begin_read(&mut s).unwrap();
        assert!(!lists::finish_read(&mut s, &read, Ok(loaded.clone())));
        assert!(s.slack_lists.operations.is_empty());
        assert!(lists::editor::presentation(&s).is_none());
        assert_eq!(s.slack_lists.view.as_ref().unwrap().read, 0);
        let revision = s.slack_lists.view.as_ref().unwrap().revision;
        assert!(!lists::finish_read(&mut s, &read, Ok(loaded.clone())));
        assert_eq!(s.slack_lists.view.as_ref().unwrap().revision, revision);
        let record = loaded.records.records.iter().find(|r| r.id == row).unwrap();
        let column = loaded.document.column("ACTUAL_SERVER_FIELD").unwrap();
        assert_eq!(
            column
                .edit_value(
                    record,
                    &loaded.document.id,
                    loaded.personal_task_lists.as_ref()
                )
                .unwrap(),
            CellEditValue::Select(values.clone())
        );
        if values == desired {
            let CellPresentation::Select(labels) = column.presentation(record) else {
                panic!()
            };
            assert_eq!(labels.iter().map(|v| v.id.clone()).collect::<Vec<_>>(), ids);
            assert_ne!(values, ids); // display order is not wire/readback normalization.
        }
        assert_eq!(order(&s), order_before);
        assert_eq!(
            loaded.records.raw["response_metadata"],
            original.records.raw["response_metadata"]
        );
        assert_eq!(loaded.records.records.len(), 73);
        let mut restored = loaded.records.raw.clone();
        for row in restored["records"].as_array_mut().unwrap() {
            row["fields"]
                .as_array_mut()
                .unwrap()
                .retain(|field| field["key"] != "actual_server_key");
        }
        assert_eq!(restored, original.records.raw);
        assert_eq!(
            s.slack_lists.view.as_ref().unwrap().layout.find.applied,
            "Item 0"
        );
    }
}

#[test]
fn differing_known_empty_multi_read_retires_the_attempt_and_requires_cancel_reopen() {
    use crate::slack_lists as lists;
    let (mut s, _cleanup) = text_shell();
    let (ids, row) = observed_multi(&mut s);
    let visit = generation(&s).unwrap();
    lists::editor::begin_shell(&mut s, &row, "ACTUAL_SERVER_FIELD", visit).unwrap();
    let editor = lists::editor::presentation(&s).unwrap();
    let attempt =
        lists::editor::begin_save(&mut s, editor.generation, CellEditValue::Select(ids)).unwrap();
    let loaded = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    lists::validate_edit(&mut s, &attempt, loaded.clone()).unwrap();
    lists::finish_edit(&mut s, &attempt, Err(Error::Transport("lost".into())));
    let read = lists::begin_read(&mut s).unwrap();
    assert!(!lists::finish_read(&mut s, &read, Ok(loaded.clone())));
    assert!(s.slack_lists.operations.is_empty());
    let retained = lists::editor::presentation(&s).unwrap();
    assert!(retained.stale && retained.disabled);
    assert_eq!(
        s.slack_lists.view.as_ref().unwrap().error.as_deref(),
        Some("Slack currently shows a different value. Review it before changing it again.")
    );
    assert!(
        lists::editor::begin_save(&mut s, editor.generation, CellEditValue::Select(vec![]))
            .is_err()
    );
    assert!(!lists::finish_read(&mut s, &read, Ok(loaded)));
    assert!(lists::editor::cancel_shell(&mut s, editor.generation));
    lists::editor::begin_shell(&mut s, &row, "ACTUAL_SERVER_FIELD", visit).unwrap();
    assert_eq!(
        *lists::editor::presentation(&s).unwrap().original,
        CellEditValue::Select(vec![])
    );
}

#[test]
fn named_multi_static_aliases_are_disconnected_ready_and_never_borrow_single_executor() {
    for alias in [
        "slack-list-multi-select-field-create",
        "slack-list-multi-select-field-create-narrow",
    ] {
        let s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), alias);
        assert!(s.core.session.is_none() && s.core.transport.is_none());
        let p = presentation(&s).unwrap();
        assert!(p.can_confirm);
        assert_eq!(p.select_draft.unwrap().format, SelectCreateFormat::Multiple);
        let e = entry(&s).unwrap();
        assert_eq!(e.select_draft.rows.len(), 8);
        assert!(e.intent.is_none() && e.job.is_none());
        assert_eq!(crate::slack_lists::fixture::attempt_count(&s), 0);
        assert_eq!(
            s.slack_lists
                .view
                .as_ref()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap()
                .records
                .records
                .len(),
            73
        );
        assert!(text_create::select::close_fixture(&s));
    }
}

#[test]
fn reordered_known_read_is_different_and_cannot_be_normalized_into_success() {
    use crate::slack_lists as lists;
    let (mut s, _cleanup) = text_shell();
    let (ids, row) = observed_multi(&mut s);
    let visit = generation(&s).unwrap();
    lists::editor::begin_shell(&mut s, &row, "ACTUAL_SERVER_FIELD", visit).unwrap();
    let editor = lists::editor::presentation(&s).unwrap();
    let attempt = lists::editor::begin_save(
        &mut s,
        editor.generation,
        CellEditValue::Select(vec![ids[1].clone(), ids[0].clone()]),
    )
    .unwrap();
    let mut loaded = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    lists::validate_edit(&mut s, &attempt, loaded.clone()).unwrap();
    lists::finish_edit(&mut s, &attempt, Ok(json!({"ok":true})));
    let mut raw = loaded.records.raw.clone();
    raw["records"].as_array_mut().unwrap().iter_mut().find(|r|r["id"]==row).unwrap()["fields"].as_array_mut().unwrap().push(json!({"key":"actual_server_key","column_id":"ACTUAL_SERVER_FIELD","row_id":row,"select":ids}));
    loaded.records = RecordsPage::parse(raw).unwrap();
    let read = lists::begin_read(&mut s).unwrap();
    assert!(!lists::finish_read(&mut s, &read, Ok(loaded.clone())));
    assert!(s.slack_lists.operations.is_empty());
    let retained = lists::editor::presentation(&s).unwrap();
    assert_eq!(retained.generation, editor.generation);
    assert!(retained.stale && retained.disabled);
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records,
        loaded.records
    );
    assert_eq!(
        s.slack_lists.view.as_ref().unwrap().error.as_deref(),
        Some("Slack currently shows a different value. Review it before changing it again.")
    );
    assert!(!lists::finish_read(&mut s, &read, Ok(loaded)));
    assert!(lists::editor::cancel_shell(&mut s, editor.generation));
    lists::editor::begin_shell(&mut s, &row, "ACTUAL_SERVER_FIELD", visit).unwrap();
    assert_eq!(
        *lists::editor::presentation(&s).unwrap().original,
        CellEditValue::Select(ids)
    );
}
