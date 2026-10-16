use super::*;
use super_platinum_core::slack_lists::ColumnCreateKind;
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
fn date_intent(s: &mut ShellState) -> (Guard, Intent) {
    let serial = open(s);
    assert!(kind(s, serial, "date"));
    assert!(name(s, serial, "Release notes"));
    prepare(s, serial).unwrap()
}
fn date_addition(s: &ShellState) -> network::Metadata {
    let mut m = addition(s, true);
    m.document.raw["list_metadata"]["schema"][1]["type"] = json!("date");
    m.document.raw["list_metadata"]["schema"][1]["options"] =
        json!({"format":"single","date_format":"default"});
    m.document = Document::parse(m.document.raw).unwrap();
    m
}
#[test]
fn same_render_name_events_remain_ordered_while_type_epoch_rejects_stale_callbacks_and_aba() {
    let (mut s, _cleanup) = text_shell();
    let serial = open(&mut s);
    let visit = generation(&s).unwrap();
    let selection = super::super::super::selection(&s).unwrap();
    let rendered = text_create::choice(&s).unwrap();
    assert_eq!(rendered.kind, ColumnCreateKind::Text);
    // The event boundary captures one rendered Choice, just as panel oninput
    // does. No presentation/rerender occurs between these two ordered events.
    let same_handler = |value: &str| {
        text_create::input_shell(&s, serial, visit, selection, rendered, value.into())
    };
    assert!(same_handler("N"));
    assert!(same_handler("No"));
    assert_eq!(entry(&s).unwrap().input, Input::NewTextField("No".into()));
    assert_eq!(text_create::choice(&s), Some(rendered));
    // A chooser from the same render remains valid after those name events.
    assert!(text_create::kind_shell(
        &s, serial, visit, selection, rendered, "date"
    ));
    let date = text_create::choice(&s).unwrap();
    assert_ne!(date.revision, rendered.revision);
    assert_eq!(entry(&s).unwrap().input, Input::NewTextField("No".into()));
    assert!(!same_handler("Old Text"));
    assert!(!text_create::kind_shell(
        &s, serial, visit, selection, rendered, "text"
    ));
    assert!(kind(&s, serial, "text"));
    let text = text_create::choice(&s).unwrap();
    assert_eq!(text.kind, rendered.kind);
    assert_ne!(text.revision, rendered.revision);
    assert!(!same_handler("Old after type ABA"));
    assert!(!text_create::kind_shell(
        &s, serial, visit, selection, rendered, "date"
    ));
    assert!(!text_create::input_shell(
        &s,
        serial,
        visit,
        selection,
        date,
        "Old Date".into()
    ));
    let current_handler =
        |value: &str| text_create::input_shell(&s, serial, visit, selection, text, value.into());
    assert!(current_handler("Notes"));
    assert!(current_handler("Other"));
    assert!(current_handler("Notes"));
    assert_eq!(text_create::choice(&s), Some(text));
    assert_eq!(
        entry(&s).unwrap().input,
        Input::NewTextField("Notes".into())
    );
    for invalid in [
        "NUMBER",
        "date0",
        "text ",
        "Checkbox",
        "",
        "{\"type\":\"date\"}",
    ] {
        assert!(!kind(&s, serial, invalid));
        assert_eq!(text_create::choice(&s), Some(text));
    }
}
#[test]
fn date_cancel_discards_only_editor_and_reopen_defaults_to_text_with_fresh_revision() {
    let (mut s, _cleanup) = text_shell();
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let rows = order(&s);
    let serial = open(&mut s);
    assert!(kind(&s, serial, "date"));
    assert!(name(&s, serial, "Count"));
    let old = text_create::choice(&s).unwrap();
    assert!(entry(&s).unwrap().job.is_none());
    assert!(entry(&s).unwrap().intent.is_none());
    assert!(close_entry(&s, serial));
    assert!(entry(&s).is_none());
    assert_eq!(metadata(&s, None).document, before.document);
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
    assert_eq!(order(&s), rows);
    let next = open(&mut s);
    assert_ne!(next, serial);
    let fresh = text_create::choice(&s).unwrap();
    assert_eq!(fresh.kind, ColumnCreateKind::Text);
    assert_ne!(fresh.revision, old.revision);
    assert!(!text_create::input_shell(
        &s,
        next,
        generation(&s).unwrap(),
        super::super::super::selection(&s).unwrap(),
        old,
        "Stale".into()
    ));
    assert_eq!(entry(&s).unwrap().input, Input::NewTextField(String::new()));
}
#[test]
fn date_permit_fixed_options_and_name_type_are_immutable_from_preparation_through_uncertain() {
    let (mut s, _cleanup) = text_shell();
    let (g, i) = date_intent(&mut s);
    let choice = text_create::choice(&s).unwrap();
    let Intent::NewTextField(intent) = &i else {
        panic!()
    };
    assert_eq!(intent.kind(), ColumnCreateKind::DateSingleDefault);
    assert!(!kind(&s, g.serial, "text"));
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
            ("type".into(), "date".into()),
            (
                "options".into(),
                "{\"date_format\":\"default\",\"format\":\"single\"}".into()
            ),
            ("_x_reason".into(), "create-column".into())
        ]
    );
    assert!(!kind(&s, g.serial, "text"));
    assert!(!name(&s, g.serial, "Other"));
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
    assert_eq!(
        entry(&s).unwrap().input,
        Input::NewTextField("Release notes".into())
    );
    assert!(prepare(&mut s, g.serial).is_none());
}
#[test]
fn date_observation_retains_full_population_find_sort_filter_and_projects_empty_and_opens_existing_calendar()
 {
    let (mut s, _cleanup) = text_shell();
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let rows = order(&s);
    let (g, i) = date_intent(&mut s);
    admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
    let ack = json!({"ok":true,"column":{"id":"IGNORED","type":"date"}});
    completed(&mut s, &g, Ok(ack.clone()));
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert_eq!(entry(&s).unwrap().raw_reply, Some(ack));
    assert_eq!(metadata(&s, None).document, before.document);
    let check = checking(&s);
    let fresh = date_addition(&s);
    assert!(checked(&mut s, &check, Ok(fresh)));
    let e = entry(&s).unwrap();
    assert_eq!(e.stage, Stage::Observed);
    let shown = crate::slack_lists::rename::presentation(&s).unwrap();
    assert_eq!(shown.stage, Stage::Observed);
    assert_eq!(
        shown.message,
        "A matching Date field is now visible in this Table."
    );
    let field = e.observed_field.unwrap();
    assert_eq!(field.id, "ACTUAL_SERVER_FIELD");
    assert_eq!(order(&s), rows);
    let v = s.slack_lists.view.as_ref().unwrap();
    assert_eq!(v.loaded.as_ref().unwrap().records, before.records);
    assert_eq!(v.layout.find.applied, "Item 0");
    assert_eq!(v.layout.find.revision, 17);
    let p = crate::slack_lists::presentation(&s).unwrap();
    let mut seen = 0;
    for e in p.entries.iter() {
        if let crate::slack_lists::layout::TableEntry::Row(row) = e {
            let cell = row.cells.iter().find(|c| c.column_id == field.id).unwrap();
            assert_eq!(cell.text, "");
            assert_eq!(cell.checkbox, None);
            seen += 1;
        }
    }
    assert_eq!(seen, rows.len());
    let mut restored = metadata(&s, None).document.raw;
    restored["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    restored["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    assert_eq!(restored, before.document.raw);
    assert!(close_entry(&s, g.serial));
    let visit = generation(&s).unwrap();
    crate::slack_lists::editor::begin_shell(&mut s, &rows[0], &field.id, visit).unwrap();
    let editor = crate::slack_lists::editor::presentation(&s).unwrap();
    assert_eq!(editor.kind, crate::slack_lists::editor::EditorKind::Date);
    assert_eq!(
        *editor.original,
        super_platinum_core::slack_lists::CellEditValue::Date(None)
    );
    let attempt = crate::slack_lists::editor::begin_save(
        &mut s,
        editor.generation,
        super_platinum_core::slack_lists::CellEditValue::Date(Some("2028-02-29".into())),
    )
    .unwrap();
    assert_eq!(
        attempt.edit.cells(),
        json!([{"row_id":rows[0],"column_id":field.id,"date":["2028-02-29"]}])
    );
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
#[test]
fn wrong_type_or_options_readback_is_current_conflict_and_cannot_rearm_date_write() {
    for text in [true, false] {
        let (mut s, _cleanup) = text_shell();
        let (g, i) = date_intent(&mut s);
        admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
        completed(&mut s, &g, Ok(json!({"ok":true})));
        let mut fresh = if text {
            addition(&s, true)
        } else {
            date_addition(&s)
        };
        if !text {
            fresh.document.raw["list_metadata"]["schema"][1]["options"] =
                json!({"format":"range","date_format":"default"});
            fresh.document = Document::parse(fresh.document.raw).unwrap();
        }
        let check = checking(&s);
        assert!(checked(&mut s, &check, Ok(fresh)));
        let e = entry(&s).unwrap();
        assert_eq!(e.stage, Stage::Conflict);
        assert!(e.observed_field.is_none());
        assert_eq!(e.field_choice.kind, ColumnCreateKind::DateSingleDefault);
        assert!(e.review.as_ref().unwrap().navigation_current(&s));
        assert!(prepare(&mut s, g.serial).is_none());
        assert!(close_entry(&s, g.serial));
    }
}
