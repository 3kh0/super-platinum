use super::*;
use super_platinum_core::slack_lists::{CellEditValue, ColumnCreateKind, LinkValue};

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
fn link_intent(s: &mut ShellState) -> (Guard, Intent) {
    let serial = open(s);
    assert!(kind(s, serial, "link"));
    assert!(name(s, serial, "Release notes"));
    prepare(s, serial).unwrap()
}
fn link_addition(s: &ShellState, options: Option<Value>) -> network::Metadata {
    let mut m = addition(s, true);
    let column = m.document.raw["list_metadata"]["schema"][1]
        .as_object_mut()
        .unwrap();
    column.insert("type".into(), json!("link"));
    column.remove("options");
    if let Some(value) = options {
        column.insert("options".into(), value);
    }
    m.document = Document::parse(m.document.raw).unwrap();
    m
}

#[test]
fn link_type_and_name_callbacks_keep_epoch_and_once_only_six_field_intent() {
    let (mut s, _cleanup) = text_shell();
    let serial = open(&mut s);
    let visit = generation(&s).unwrap();
    let selection = super::super::super::selection(&s).unwrap();
    assert!(kind(&s, serial, "link"));
    let choice = text_create::choice(&s).unwrap();
    assert_eq!(choice.kind, ColumnCreateKind::Link);
    let input = |v: &str| text_create::input_shell(&s, serial, visit, selection, choice, v.into());
    assert!(input("R"));
    assert!(input("Reference"));
    assert_eq!(text_create::choice(&s), Some(choice));
    assert!(!kind(&s, serial, "url"));
    assert!(kind(&s, serial, "text"));
    assert!(kind(&s, serial, "link"));
    assert!(!input("stale"));
    assert!(!text_create::kind_shell(
        &s, serial, visit, selection, choice, "text"
    ));
    assert_eq!(
        entry(&s).unwrap().input,
        Input::NewTextField("Reference".into())
    );
    let (guard, intent) = prepare(&mut s, serial).unwrap();
    let permit = admitted(&s, &guard, &intent, &metadata(&s, None)).unwrap();
    let RequestBody::Form(fields) = permit.request(&s.core.client, &fixture::session()).body else {
        panic!()
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "fixture-not-a-token".into()),
            ("list_id".into(), "FL1".into()),
            ("view_id".into(), "V_TEXT".into()),
            ("name".into(), "Reference".into()),
            ("type".into(), "link".into()),
            ("_x_reason".into(), "create-column".into())
        ]
    );
    assert!(admitted(&s, &guard, &intent, &metadata(&s, None)).is_none());
    assert!(!name(&s, serial, "Other"));
    assert!(!kind(&s, serial, "text"));
    completed(
        &mut s,
        &guard,
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "lost".into(),
        ))),
    );
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert!(prepare(&mut s, serial).is_none());
}

#[test]
fn both_link_observation_shapes_keep_population_find_and_open_existing_link_editor() {
    for options in [None, Some(json!({}))] {
        let (mut s, _cleanup) = text_shell();
        let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
        let rows = order(&s);
        let (g, i) = link_intent(&mut s);
        admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
        completed(&mut s, &g, Ok(json!({"ok":true})));
        assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
        assert_eq!(metadata(&s, None).document, before.document);
        let check = checking(&s);
        let fresh = link_addition(&s, options.clone());
        assert!(checked(&mut s, &check, Ok(fresh)));
        let p = presentation(&s).unwrap();
        assert_eq!(p.stage, Stage::Observed);
        assert_eq!(
            p.message,
            "A matching Link field is now visible in this Table."
        );
        let field = p.observed_field.unwrap();
        assert_eq!(field.id, "ACTUAL_SERVER_FIELD");
        assert_eq!(order(&s), rows);
        let view = s.slack_lists.view.as_ref().unwrap();
        assert_eq!(view.loaded.as_ref().unwrap().records, before.records);
        assert_eq!(
            view.loaded
                .as_ref()
                .unwrap()
                .document
                .column(&field.id)
                .unwrap()
                .raw
                .get("options"),
            options.as_ref()
        );
        assert_eq!(view.layout.find.applied, "Item 0");
        assert_eq!(view.layout.find.revision, 17);
        assert!(close_entry(&s, g.serial));
        let visit = generation(&s).unwrap();
        crate::slack_lists::editor::begin_shell(&mut s, &rows[0], &field.id, visit).unwrap();
        let editor = crate::slack_lists::editor::presentation(&s).unwrap();
        assert_eq!(editor.kind, crate::slack_lists::editor::EditorKind::Link);
        assert_eq!(*editor.original, CellEditValue::Link(None));
        // Cancel stays the existing editor path; it cannot mutate the captured page.
        assert!(crate::slack_lists::editor::cancel_shell(
            &mut s,
            editor.generation
        ));
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
        crate::slack_lists::editor::begin_shell(&mut s, &rows[0], &field.id, visit).unwrap();
        let editor = crate::slack_lists::editor::presentation(&s).unwrap();
        let attempt = crate::slack_lists::editor::begin_save(
            &mut s,
            editor.generation,
            CellEditValue::Link(Some(LinkValue {
                original_url: "https://example.com/spec".into(),
                display_name: Some("Spec".into()),
                display_as_url: false,
            })),
        )
        .unwrap();
        assert_eq!(
            attempt.edit.cells(),
            json!([{"row_id":rows[0],"column_id":field.id,"link":[{"original_url":"https://example.com/spec","display_name":"Spec","display_as_url":false}]}])
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
}

#[test]
fn unsupported_link_options_remain_conflict_with_no_observed_field_or_replay() {
    for options in [
        Value::Null,
        json!({"format":"link"}),
        json!({"default_value":null}),
    ] {
        let (mut s, _cleanup) = text_shell();
        let (g, i) = link_intent(&mut s);
        admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
        completed(&mut s, &g, Ok(json!({"ok":true})));
        let check = checking(&s);
        let fresh = link_addition(&s, Some(options));
        assert!(checked(&mut s, &check, Ok(fresh)));
        assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
        assert!(entry(&s).unwrap().observed_field.is_none());
        assert!(prepare(&mut s, g.serial).is_none());
        assert!(close_entry(&s, g.serial));
    }
}

#[test]
fn link_static_aliases_only_stage_shared_review_and_cancel_without_requests() {
    for alias in [
        "slack-list-link-field-create",
        "slack-list-link-field-create-narrow",
    ] {
        let mut s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), alias);
        let _cleanup = Cleanup(Key::current(&s).unwrap());
        let p = presentation(&s).unwrap();
        assert_eq!(p.field_choice.unwrap().kind, ColumnCreateKind::Link);
        assert_eq!(p.name, "Reference");
        assert!(p.can_confirm && p.observed_field.is_none());
        assert!(s.core.session.is_none() && s.core.transport.is_none());
        assert!(crate::slack_lists::fixture::enabled(&s));
        let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
        assert_eq!(before.records.records.len(), 73);
        assert_eq!(
            before.records.raw["response_metadata"]["next_cursor"],
            "precision-opaque+/="
        );
        assert_eq!(crate::slack_lists::fixture::attempt_count(&s), 0);
        assert!(close_entry(&s, p.serial));
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
        let next = open(&mut s);
        assert_eq!(
            text_create::choice(&s).unwrap().kind,
            ColumnCreateKind::Text
        );
        assert!(close_entry(&s, next));
    }
}
