use super::*;
use super_platinum_core::slack_lists::{CellEditValue, ColumnCreateKind};
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
fn people_intent(s: &mut ShellState) -> (Guard, Intent) {
    let serial = open(s);
    assert!(kind(s, serial, "user"));
    assert!(name(s, serial, "Release notes"));
    prepare(s, serial).unwrap()
}
fn people_addition(s: &ShellState) -> network::Metadata {
    let mut m = addition(s, true);
    m.document.raw["list_metadata"]["schema"][1]["type"] = json!("user");
    m.document.raw["list_metadata"]["schema"][1]["options"] =
        json!({"format":"single_entity","show_member_name":true,"default_value":null});
    m.document = Document::parse(m.document.raw).unwrap();
    m
}
#[test]
fn people_type_epoch_preserves_consecutive_name_events_and_refuses_old_aba_handlers() {
    let (mut s, _cleanup) = text_shell();
    let serial = open(&mut s);
    let visit = generation(&s).unwrap();
    let selection = super::super::super::selection(&s).unwrap();
    let old = text_create::choice(&s).unwrap();
    assert!(kind(&s, serial, "user"));
    let people = text_create::choice(&s).unwrap();
    let input = |v: &str| text_create::input_shell(&s, serial, visit, selection, people, v.into());
    assert!(input("R"));
    assert!(input("Reviewer"));
    assert_eq!(
        entry(&s).unwrap().input,
        Input::NewTextField("Reviewer".into())
    );
    assert_eq!(text_create::choice(&s), Some(people));
    assert!(!text_create::input_shell(
        &s,
        serial,
        visit,
        selection,
        old,
        "stale".into()
    ));
    assert!(kind(&s, serial, "text"));
    assert!(kind(&s, serial, "user"));
    assert_ne!(text_create::choice(&s).unwrap().revision, people.revision);
    assert!(!input("stale People"));
    assert!(!text_create::kind_shell(
        &s, serial, visit, selection, people, "text"
    ));
    assert!(name(&s, serial, " "));
    assert!(prepare(&mut s, serial).is_none());
    assert_eq!(entry(&s).unwrap().input, Input::NewTextField(" ".into()));
    assert_eq!(entry(&s).unwrap().stage, Stage::Editing);
}
#[test]
fn people_prepare_captures_exact_options_and_cannot_rearm_after_uncertain() {
    let (mut s, _cleanup) = text_shell();
    let (g, i) = people_intent(&mut s);
    let old = text_create::choice(&s).unwrap();
    let Intent::NewTextField(intent) = &i else {
        panic!()
    };
    assert_eq!(intent.kind(), ColumnCreateKind::PeopleSingleDefault);
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
            ("type".into(), "user".into()),
            (
                "options".into(),
                r#"{"default_value":null,"format":"single_entity","show_member_name":true}"#.into()
            ),
            ("_x_reason".into(), "create-column".into())
        ]
    );
    assert!(admitted(&s, &g, &i, &metadata(&s, None)).is_none());
    assert!(!kind(&s, g.serial, "text"));
    assert!(!name(&s, g.serial, "Other"));
    completed(
        &mut s,
        &g,
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "lost".into(),
        ))),
    );
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert!(prepare(&mut s, g.serial).is_none());
    assert_eq!(text_create::choice(&s), Some(old));
}
#[test]
fn observed_people_keeps_raw_population_and_opens_existing_single_person_editor() {
    let (mut s, _cleanup) = text_shell();
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let rows = order(&s);
    let (g, i) = people_intent(&mut s);
    admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
    completed(&mut s, &g, Ok(json!({"ok":true})));
    assert_eq!(metadata(&s, None).document, before.document);
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    let check = checking(&s);
    let fresh = people_addition(&s);
    assert!(checked(&mut s, &check, Ok(fresh)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
    let p = crate::slack_lists::rename::presentation(&s).unwrap();
    assert_eq!(
        p.message,
        "A matching People field is now visible in this Table."
    );
    let field = p.observed_field.unwrap();
    assert_eq!(field.id, "ACTUAL_SERVER_FIELD");
    assert_eq!(order(&s), rows);
    let view = s.slack_lists.view.as_ref().unwrap();
    assert_eq!(view.loaded.as_ref().unwrap().records, before.records);
    assert_eq!(view.layout.find.applied, "Item 0");
    assert_eq!(view.layout.find.revision, 17);
    let mut raw = metadata(&s, None).document.raw;
    raw["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    raw["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    assert_eq!(raw, before.document.raw);
    assert!(close_entry(&s, g.serial));
    let visit = generation(&s).unwrap();
    crate::slack_lists::editor::begin_shell(&mut s, &rows[0], &field.id, visit).unwrap();
    let editor = crate::slack_lists::editor::presentation(&s).unwrap();
    assert_eq!(editor.kind, crate::slack_lists::editor::EditorKind::User);
    assert_eq!(*editor.original, CellEditValue::Users(vec![]));
    assert!(
        crate::slack_lists::editor::begin_save(
            &mut s,
            editor.generation,
            CellEditValue::Users(vec![])
        )
        .is_err()
    );
    let attempt = crate::slack_lists::editor::begin_save(
        &mut s,
        editor.generation,
        CellEditValue::Users(vec!["U00000001".into()]),
    )
    .unwrap();
    assert_eq!(
        attempt.edit.cells(),
        json!([{"row_id":rows[0],"column_id":field.id,"user":["U00000001"]}])
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
    // User absence is known empty, unlike an unknown Checkbox readback.
    // It retires an unmatched operation; it cannot be held for a later success.
    use crate::slack_lists as lists;
    let loaded = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    lists::validate_edit(&mut s, &attempt, loaded.clone()).unwrap();
    lists::finish_edit(&mut s, &attempt, Err(Error::Transport("lost".into())));
    let read = lists::begin_read(&mut s).unwrap();
    assert!(!lists::finish_read(&mut s, &read, Ok(loaded.clone())));
    assert!(s.slack_lists.operations.is_empty());
    let retained = lists::editor::presentation(&s).unwrap();
    assert!(retained.stale && retained.disabled);
    assert_eq!(retained.generation, editor.generation);
    assert_eq!(
        s.slack_lists.view.as_ref().unwrap().error.as_deref(),
        Some("Slack currently shows a different value. Review it before changing it again.")
    );
    let mut matching = loaded;
    let mut raw = matching.records.raw.clone();
    raw["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["id"] == rows[0])
        .unwrap()["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":field.key,"column_id":field.id,"user":["U00000001"]}));
    matching.records = RecordsPage::parse(raw).unwrap();
    let later = lists::begin_read(&mut s).unwrap();
    assert!(!lists::finish_read(&mut s, &later, Ok(matching)));
    assert!(s.slack_lists.operations.is_empty());
    assert!(lists::editor::presentation(&s).unwrap().stale);
    assert!(
        lists::editor::begin_save(&mut s, editor.generation, CellEditValue::Users(vec![])).is_err()
    );
    assert!(lists::editor::cancel_shell(&mut s, editor.generation));
}
#[test]
fn people_options_normalization_is_conflict_not_confirmation_or_retry() {
    let (mut s, _cleanup) = text_shell();
    let (g, i) = people_intent(&mut s);
    admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
    completed(&mut s, &g, Ok(json!({"ok":true})));
    let mut fresh = people_addition(&s);
    fresh.document.raw["list_metadata"]["schema"][1]["options"]
        .as_object_mut()
        .unwrap()
        .remove("default_value");
    fresh.document = Document::parse(fresh.document.raw).unwrap();
    let check = checking(&s);
    assert!(checked(&mut s, &check, Ok(fresh)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
    assert!(entry(&s).unwrap().observed_field.is_none());
    assert!(prepare(&mut s, g.serial).is_none());
    assert!(close_entry(&s, g.serial));
}
#[test]
fn people_static_aliases_only_stage_review_and_cancel_preserves_raw_page() {
    for alias in [
        "slack-list-people-field-create",
        "slack-list-people-field-create-narrow",
    ] {
        let mut s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), alias);
        let _cleanup = Cleanup(Key::current(&s).unwrap());
        let p = presentation(&s).unwrap();
        assert_eq!(
            p.field_choice.unwrap().kind,
            ColumnCreateKind::PeopleSingleDefault
        );
        assert_eq!(p.name, "Reviewer");
        assert!(p.can_confirm && p.observed_field.is_none());
        assert!(s.core.session.is_none() && s.core.transport.is_none());
        assert!(crate::slack_lists::fixture::enabled(&s));
        let page = &s
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records;
        assert_eq!(page.records.len(), 73);
        assert_eq!(
            page.raw["response_metadata"]["next_cursor"],
            "precision-opaque+/="
        );
        assert_eq!(crate::slack_lists::fixture::attempt_count(&s), 0);
        let before = metadata(&s, None).document;
        let records = s
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records
            .clone();
        assert!(close_entry(&s, p.serial));
        assert!(entry(&s).is_none());
        assert_eq!(metadata(&s, None).document, before);
        assert_eq!(
            s.slack_lists
                .view
                .as_ref()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap()
                .records,
            records
        );
        let next = open(&mut s);
        assert_ne!(next, p.serial);
        assert_eq!(
            text_create::choice(&s).unwrap().kind,
            ColumnCreateKind::Text
        );
        assert!(close_entry(&s, next));
    }
}
