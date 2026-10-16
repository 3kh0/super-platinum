use super::*;
struct Cleanup(Key);
impl Drop for Cleanup {
    fn drop(&mut self) {
        with_registry(|r| {
            r.entries.remove(&self.0);
        });
    }
}
fn field_shell() -> (ShellState, Cleanup) {
    let mut s = shell();
    let mut m = metadata(&s, None);
    m.document.raw["list_metadata"]["schema"].as_array_mut().unwrap().push(json!({"id":"C_EXTRA","key":"extra","name":"Extra","type":"text","options":{"format":"text"},"opaque":1}));
    m.document.raw["list_metadata"]["schema"].as_array_mut().unwrap().push(json!({"id":"C_N","key":"number","name":"Number","type":"number","options":{"precision":2}}));
    m.document.raw["list_metadata"]["views"] = json!([{"id":"V_FIELDS","name":"Custom","type":"table","columns":[
        {"id":"name","key":"name","visible":true},{"id":"C_EXTRA","key":"extra","visible":true,"opaque":2},{"id":"C_N","key":"number","visible":false}],
        "filters":[],"sorts":[],"grouping":{},"opaque":3}]);
    m.document.raw["list_metadata"]["default_view"] = json!("V_FIELDS");
    m.document = Document::parse(m.document.raw).unwrap();
    s.slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap()
        .document = m.document;
    s.slack_lists.view.as_mut().unwrap().layout = Default::default();
    rebuild_rows(&mut s);
    assert_eq!(selected_view(&s), Some("V_FIELDS"));
    let c = Cleanup(Key::current(&s).unwrap());
    (s, c)
}
fn start(s: &mut ShellState) -> (Guard, Intent) {
    let visit = generation(s).unwrap();
    let selected = selection(s).unwrap();
    assert!(crate::slack_lists::rename::visibility::open_shell(
        s, visit, selected
    ));
    let e = entry(s).unwrap();
    assert!(change(s, None, Some(false)));
    prepare(s, e.serial).unwrap()
}
fn change(s: &ShellState, column: Option<&str>, value: Option<bool>) -> bool {
    let e = entry(s).unwrap();
    let Target::Visibility {
        column: rendered, ..
    } = &e.target
    else {
        panic!()
    };
    crate::slack_lists::rename::visibility::change(
        s,
        e.serial,
        generation(s).unwrap(),
        e.visibility_choice,
        rendered,
        column,
        value,
    )
}
fn desired(s: &ShellState) -> network::Metadata {
    let mut m = metadata(s, None);
    m.document.raw["list_metadata"]["views"][0]["columns"][1]["visible"] = json!(false);
    m.document = Document::parse(m.document.raw).unwrap();
    m
}
#[test]
fn modal_staging_keeps_projection_raw_records_and_hidden_choice_available() {
    let (mut s, _c) = field_shell();
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let columns = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .layout
        .columns
        .iter()
        .map(|c| c.id.clone())
        .collect::<Vec<_>>();
    let visit = generation(&s).unwrap();
    let selected = selection(&s).unwrap();
    assert!(crate::slack_lists::rename::visibility::open_shell(
        &mut s, visit, selected
    ));
    let e = entry(&s).unwrap();
    assert!(change(&s, None, Some(false)));
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
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .layout
            .columns
            .iter()
            .map(|c| c.id.clone())
            .collect::<Vec<_>>(),
        columns
    );
    let p = presentation(&s).unwrap().visibility.unwrap();
    assert_eq!(p.choices.len(), 2);
    assert!(change(&s, Some("C_N"), None));
    assert_eq!(entry(&s).unwrap().input, Input::Visibility(false));
    assert!(change(&s, Some("C_EXTRA"), None));
    assert_eq!(entry(&s).unwrap().input, Input::Visibility(true));
    assert!(!change(&s, Some("name"), None));
    assert!(!close_entry(&s, e.serial + 1));
    assert!(close_entry(&s, e.serial));
    assert!(entry(&s).is_none());
    assert_eq!(metadata(&s, None).document, before.document);
}
#[test]
fn fields_share_exclusion_and_reject_wrong_input_and_selection_aba() {
    let (mut s, _c) = field_shell();
    let visit = generation(&s).unwrap();
    let selected = selection(&s).unwrap();
    assert!(crate::slack_lists::rename::visibility::open_shell(
        &mut s, visit, selected
    ));
    let e = entry(&s).unwrap();
    assert!(!open_shell(&mut s, "name", visit));
    assert!(!crate::slack_lists::rename::precision::open_shell(
        &mut s, "C_N", visit
    ));
    assert!(!open_target(&mut s, Target::View("V_FIELDS".into()), visit));
    with_registry(|r| {
        r.entries.get_mut(&Key::current(&s).unwrap()).unwrap().input = Input::Name("false".into())
    });
    assert!(prepare(&mut s, e.serial).is_none());
    with_registry(|r| {
        r.entries.get_mut(&Key::current(&s).unwrap()).unwrap().input = Input::Visibility(false)
    });
    s.slack_lists
        .view
        .as_mut()
        .unwrap()
        .layout
        .selection_generation = next_id();
    assert!(!change(&s, None, Some(true)));
    assert!(prepare(&mut s, e.serial).is_none());
}
#[test]
fn field_choice_revision_rejects_old_input_and_old_chooser_including_aba() {
    let (mut s, _c) = field_shell();
    let visit = generation(&s).unwrap();
    let selected = selection(&s).unwrap();
    assert!(crate::slack_lists::rename::visibility::open_shell(
        &mut s, visit, selected
    ));
    let original = entry(&s).unwrap();
    assert!(change(&s, Some("C_N"), None));
    assert!(!crate::slack_lists::rename::visibility::change(
        &s,
        original.serial,
        visit,
        original.visibility_choice,
        "C_EXTRA",
        None,
        Some(true)
    ));
    assert!(!crate::slack_lists::rename::visibility::change(
        &s,
        original.serial,
        visit,
        original.visibility_choice,
        "C_EXTRA",
        Some("C_EXTRA"),
        None
    ));
    assert_eq!(entry(&s).unwrap().input, Input::Visibility(false));
    assert!(change(&s, Some("C_EXTRA"), None));
    assert!(!crate::slack_lists::rename::visibility::change(
        &s,
        original.serial,
        visit,
        original.visibility_choice,
        "C_EXTRA",
        None,
        Some(false)
    ));
    assert_eq!(entry(&s).unwrap().input, Input::Visibility(true));
    assert!(change(&s, None, Some(false)));
    assert_eq!(entry(&s).unwrap().input, Input::Visibility(false));
}
#[test]
fn ack_never_projects_and_current_readback_changes_only_visibility() {
    let (mut s, _c) = field_shell();
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let (g, i) = start(&mut s);
    assert!(
        admitted(&s, &g, &i, &metadata(&s, None))
            .unwrap()
            .is_visibility()
    );
    completed(&mut s, &g, Ok(json!({"ok":true,"list":{"id":"wrong"}})));
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert_eq!(metadata(&s, None).document, before.document);
    assert!(!close_entry(&s, g.serial));
    assert!(prepare(&mut s, g.serial).is_none());
    let check = checking(&s);
    let m = desired(&s);
    assert!(checked(&mut s, &check, Ok(m)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
    assert!(
        !s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .layout
            .columns
            .iter()
            .any(|c| c.id == "C_EXTRA")
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
    let mut restored = metadata(&s, None).document.raw;
    restored["list_metadata"]["views"][0]["columns"][1]["visible"] = json!(true);
    assert_eq!(restored, before.document.raw);
    assert!(close_entry(&s, g.serial));
    assert!(crate::slack_lists::rename::visibility::can_open(&s));
    let visit = generation(&s).unwrap();
    let selected = selection(&s).unwrap();
    assert!(crate::slack_lists::rename::visibility::open_shell(
        &mut s, visit, selected
    ));
    assert_eq!(
        presentation(&s).unwrap().visibility.unwrap().original,
        false
    );
}
#[test]
fn stale_read_does_not_project_and_same_serial_cannot_replay() {
    let (mut s, _c) = field_shell();
    let (g, i) = start(&mut s);
    admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
    completed(
        &mut s,
        &g,
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "lost".into(),
        ))),
    );
    let check = checking(&s);
    let before = metadata(&s, None).document;
    let m = desired(&s);
    s.slack_lists
        .view
        .as_mut()
        .unwrap()
        .layout
        .selection_generation = next_id();
    assert!(!checked(&mut s, &check, Ok(m)));
    assert_eq!(metadata(&s, None).document, before);
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert!(prepare(&mut s, g.serial).is_none());
}
#[test]
fn static_named_fields_fixture_has_hidden_field_without_transport_or_write_capability() {
    for name in [
        "slack-list-field-visibility",
        "slack-list-field-visibility-narrow",
    ] {
        let s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
        let _c = Cleanup(Key::current(&s).unwrap());
        assert!(s.core.session.is_none() && s.core.transport.is_none());
        assert!(!fixture::enabled(&s));
        let p = presentation(&s).unwrap();
        assert_eq!(p.stage, Stage::Editing);
        let v = p.visibility.unwrap();
        assert!(!v.original);
        assert!(!v.desired);
        assert!(v.choices.iter().any(|c| c.0 == "C_NUMBER"));
        let loaded = s
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap();
        assert_eq!(
            loaded.records.raw["records"][5]["fields"][2]["number"],
            json!([12.5])
        );
        assert_eq!(
            loaded.records.raw["records"][11]["parent_record_id"],
            "R_LIST_010"
        );
        assert!(!loaded.records.complete());
    }
}
