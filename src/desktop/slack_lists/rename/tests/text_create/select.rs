use super::*;
use super_platinum_core::slack_lists::{CellEditValue, ColumnCreateKind};
use text_create::select::{Edit, edit_shell};
fn choose(s: &ShellState, serial: u64, kind: &str) -> bool {
    text_create::kind_shell(
        s,
        serial,
        generation(s).unwrap(),
        super::super::super::selection(s).unwrap(),
        text_create::choice(s).unwrap(),
        kind,
    )
}
fn edit(s: &ShellState, serial: u64, action: Edit) -> bool {
    edit_shell(
        s,
        serial,
        generation(s).unwrap(),
        super::super::super::selection(s).unwrap(),
        text_create::choice(s).unwrap(),
        action,
    )
}
fn epoch(s: &ShellState) -> u64 {
    entry(s).unwrap().select_draft.add_epoch
}
fn add(s: &ShellState, serial: u64, label: &str) {
    let epoch = epoch(s);
    assert!(edit(
        s,
        serial,
        Edit::Pending {
            epoch,
            value: label.into()
        }
    ));
    assert!(edit(s, serial, Edit::Add { epoch }));
}
fn start(s: &mut ShellState) -> u64 {
    let serial = open(s);
    assert!(choose(s, serial, "select"));
    assert!(name(s, serial, "Release notes"));
    serial
}
fn observed(s: &mut ShellState) -> (u64, String) {
    let serial = start(s);
    add(s, serial, "Ready");
    add(s, serial, "Équipe 東京");
    let (g, i) = prepare(s, serial).unwrap();
    let Intent::NewTextField(intent) = &i else {
        panic!()
    };
    let options = intent.options().unwrap();
    let id = options["choices"][0]["value"].as_str().unwrap().to_owned();
    admitted(s, &g, &i, &metadata(s, None)).unwrap();
    completed(s, &g, Ok(json!({"ok":true})));
    let mut fresh = addition(s, true);
    fresh.document.raw["list_metadata"]["schema"][1]["type"] = json!("select");
    fresh.document.raw["list_metadata"]["schema"][1]["options"] = options;
    fresh.document = Document::parse(fresh.document.raw).unwrap();
    let check = checking(s);
    assert!(checked(s, &check, Ok(fresh)));
    assert_eq!(
        presentation(s).unwrap().message,
        "A matching Select field is now visible in this Table."
    );
    (serial, id)
}
#[test]
fn same_render_input_duplicate_add_removed_row_and_type_aba_are_owned() {
    let (mut s, _cleanup) = text_shell();
    let serial = start(&mut s);
    let old = text_create::choice(&s).unwrap();
    let visit = generation(&s).unwrap();
    let selection = super::super::super::selection(&s).unwrap();
    let first = epoch(&s);
    let input = |value: &str| {
        edit_shell(
            &s,
            serial,
            visit,
            selection,
            old,
            Edit::Pending {
                epoch: first,
                value: value.into(),
            },
        )
    };
    assert!(input("R"));
    assert!(input(" Ready "));
    assert!(edit(&s, serial, Edit::Add { epoch: first }));
    assert!(!edit(&s, serial, Edit::Add { epoch: first }));
    assert!(!input("Late"));
    let row = entry(&s).unwrap().select_draft.rows[0].clone();
    assert_eq!(row.option.label(), "Ready");
    assert_eq!(entry(&s).unwrap().select_draft.pending, "");
    assert!(edit(
        &s,
        serial,
        Edit::Remove {
            row: row.generation
        }
    ));
    add(&s, serial, "Ready");
    let next = entry(&s).unwrap().select_draft.rows[0].clone();
    assert_ne!(row.generation, next.generation);
    assert!(!edit(
        &s,
        serial,
        Edit::Remove {
            row: row.generation
        }
    ));
    assert!(choose(&s, serial, "text"));
    assert!(choose(&s, serial, "select"));
    assert_ne!(text_create::choice(&s).unwrap(), old);
    assert!(!edit_shell(
        &s,
        serial,
        visit,
        selection,
        old,
        Edit::Add { epoch: first }
    ));
    assert!(entry(&s).unwrap().select_draft.rows.is_empty());
    assert!(!edit_shell(
        &s,
        serial,
        visit + 1,
        selection,
        text_create::choice(&s).unwrap(),
        Edit::Pending {
            epoch: epoch(&s),
            value: "Wrong".into()
        }
    ));
}
#[test]
fn invalid_pending_and_ninth_option_are_retained_without_preparing_or_replaying() {
    let (mut s, _cleanup) = text_shell();
    let serial = start(&mut s);
    for invalid in [" ", "bad\nlabel", &"😀".repeat(129)] {
        let epoch = epoch(&s);
        assert!(edit(
            &s,
            serial,
            Edit::Pending {
                epoch,
                value: invalid.into()
            }
        ));
        assert!(edit(&s, serial, Edit::Add { epoch }));
        assert_eq!(entry(&s).unwrap().select_draft.pending, invalid);
        assert!(entry(&s).unwrap().select_draft.rows.is_empty());
        assert!(prepare(&mut s, serial).is_none());
        assert_eq!(entry(&s).unwrap().stage, Stage::Editing);
    }
    assert!(edit(
        &s,
        serial,
        Edit::Pending {
            epoch: epoch(&s),
            value: String::new()
        }
    ));
    for n in 0..8 {
        add(&s, serial, &format!("Option {n}"));
    }
    let epoch = epoch(&s);
    assert!(edit(
        &s,
        serial,
        Edit::Pending {
            epoch,
            value: "Ninth".into()
        }
    ));
    assert!(edit(&s, serial, Edit::Add { epoch }));
    assert_eq!(entry(&s).unwrap().select_draft.rows.len(), 8);
    assert_eq!(entry(&s).unwrap().select_draft.pending, "Ninth");
    assert!(prepare(&mut s, serial).is_none());
    assert!(close_entry(&s, serial));
    assert!(s.slack_lists.operations.is_empty());
    assert!(entry(&s).is_none());
}
#[test]
fn immutable_select_permit_cancel_and_population_keep_shared_contract() {
    let (mut s, _cleanup) = text_shell();
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let rows = order(&s);
    let serial = start(&mut s);
    add(&s, serial, "Ready");
    let old = entry(&s).unwrap().select_draft.clone();
    assert!(close_entry(&s, serial));
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
    let serial = start(&mut s);
    add(&s, serial, "Équipe 東京");
    let captured = entry(&s).unwrap().select_draft.capture().unwrap().wire();
    let (g, i) = prepare(&mut s, serial).unwrap();
    assert!(!edit(
        &s,
        serial,
        Edit::Add {
            epoch: old.add_epoch
        }
    ));
    assert!(!choose(&s, serial, "text"));
    let permit = admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
    let RequestBody::Form(fields) = permit.request(&s.core.client, &fixture::session()).body else {
        panic!()
    };
    assert_eq!(fields.len(), 7);
    assert_eq!(fields[4], ("type".into(), "select".into()));
    assert_eq!(
        serde_json::from_str::<Value>(&fields[5].1).unwrap(),
        captured
    );
    assert!(admitted(&s, &g, &i, &metadata(&s, None)).is_none());
    completed(
        &mut s,
        &g,
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "lost".into(),
        ))),
    );
    assert!(prepare(&mut s, serial).is_none());
    assert!(!edit(
        &s,
        serial,
        Edit::Remove {
            row: entry(&s).unwrap().select_draft.rows[0].generation
        }
    ));
    assert_eq!(metadata(&s, None).document, before.document);
    assert_eq!(order(&s), rows);
}
#[test]
fn observed_select_opens_existing_editor_and_differing_empty_read_retires_operation() {
    use crate::slack_lists as lists;
    let (mut s, _cleanup) = text_shell();
    let original = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let rows = order(&s);
    let (serial, option) = observed(&mut s);
    assert_eq!(order(&s), rows);
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records,
        original.records
    );
    assert_eq!(
        s.slack_lists.view.as_ref().unwrap().layout.find.applied,
        "Item 0"
    );
    assert!(close_entry(&s, serial));
    let visit = generation(&s).unwrap();
    lists::editor::begin_shell(&mut s, &rows[0], "ACTUAL_SERVER_FIELD", visit).unwrap();
    let editor = lists::editor::presentation(&s).unwrap();
    assert_eq!(editor.kind, lists::editor::EditorKind::Select);
    assert_eq!(*editor.original, CellEditValue::Select(vec![]));
    let attempt = lists::editor::begin_save(
        &mut s,
        editor.generation,
        CellEditValue::Select(vec![option]),
    )
    .unwrap();
    let loaded = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    lists::validate_edit(&mut s, &attempt, loaded.clone()).unwrap();
    lists::finish_edit(&mut s, &attempt, Err(Error::Transport("lost".into())));
    assert!(s.slack_lists.operations.get("FL1").unwrap().phase == lists::Phase::Uncertain);
    let request = lists::begin_read(&mut s).unwrap();
    let revision = s.slack_lists.view.as_ref().unwrap().revision;
    assert!(!lists::finish_read(&mut s, &request, Ok(loaded.clone())));
    assert!(s.slack_lists.operations.is_empty());
    let view = s.slack_lists.view.as_ref().unwrap();
    assert_eq!(view.read, 0);
    assert_ne!(view.revision, revision);
    assert_eq!(
        view.error.as_deref(),
        Some("Slack currently shows a different value. Review it before changing it again.")
    );
    assert_eq!(view.loaded.as_ref().unwrap().records, loaded.records);
    let retained = lists::editor::presentation(&s).unwrap();
    assert!(retained.stale && retained.disabled);
    assert_eq!(retained.generation, editor.generation);
    assert!(
        lists::editor::begin_save(&mut s, editor.generation, CellEditValue::Select(vec![]))
            .is_err()
    );
    let consumed_revision = s.slack_lists.view.as_ref().unwrap().revision;
    assert!(!lists::finish_read(&mut s, &request, Ok(loaded)));
    assert_eq!(
        s.slack_lists.view.as_ref().unwrap().revision,
        consumed_revision
    );
    assert!(lists::editor::cancel_shell(&mut s, editor.generation));
    lists::editor::begin_shell(&mut s, &rows[0], "ACTUAL_SERVER_FIELD", visit).unwrap();
    assert!(!lists::editor::presentation(&s).unwrap().stale);
}
#[test]
fn named_static_select_modes_have_bounded_options_no_requests_and_exact_raw_population() {
    for alias in [
        "slack-list-select-field-create",
        "slack-list-select-field-create-narrow",
    ] {
        let mut s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), alias);
        // fixture_variant is session-free; install already opened the shared editor.
        let shown = presentation(&s).unwrap();
        assert_eq!(shown.field_choice.unwrap().kind, ColumnCreateKind::Select);
        assert_eq!(shown.select_draft.unwrap().rows.len(), 8);
        assert_eq!(shown.stage, Stage::Editing);
        assert!(entry(&s).unwrap().intent.is_none());
        assert!(entry(&s).unwrap().job.is_none());
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
        assert!(close_entry(&s, shown.serial));
        s.slack_lists.view = None;
    }
}

#[test]
fn borrowed_render_hint_refuses_pending_empty_duplicate_and_excess_rows() {
    use super_platinum_core::slack_lists::SelectCreateOption;
    use text_create::select::{Draft, Row};
    let mut draft = Draft::default();
    assert!(!draft.ready());
    for index in 0..8 {
        draft.rows.push(Row {
            generation: index,
            option: SelectCreateOption::capture(
                &"x".repeat(256),
                &format!("OptAAAAAAA{index}"),
                "purple",
            )
            .unwrap(),
        });
    }
    assert!(draft.ready());
    assert!(draft.capture().is_ok());
    draft.pending = "not added".into();
    assert!(!draft.ready());
    assert!(draft.capture().is_err());
    draft.pending.clear();
    let old = draft.rows[7].option.clone();
    draft.rows[7].option = draft.rows[0].option.clone();
    assert!(!draft.ready());
    assert!(draft.capture().is_err());
    draft.rows[7].option = old;
    let extra = draft.rows[0].clone();
    draft.rows.push(extra);
    assert!(!draft.ready());
    draft.rows.clear();
    assert!(!draft.ready());
}
