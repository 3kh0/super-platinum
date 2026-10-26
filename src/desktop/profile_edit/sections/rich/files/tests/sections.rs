use super::*;
use crate::profile_edit::sections::rich::search::file_source::sections as shared;
use super_platinum_core::{
    config::Session,
    slack::{Transport, api::CanvasSectionsRequest},
};
const ID: &str = "FFILE0001";
const SECTION: &str = "temp:C:0123456789abcdef01234567";
fn authenticated() -> (ShellState, Token) {
    let (mut shell, old) = setup();
    let session = crate::profile_edit::fixture::session(&old.owner);
    shell.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        unified_user_client_enabled: BTreeMap::new(),
        workspaces: BTreeMap::from([(old.owner.team.clone(), session)]),
    });
    shell.core.transport = Some(Arc::new(Transport::disconnected_fixture("synthetic")));
    let token = Token::capture(&shell).unwrap();
    shell.profile_edit.draft.as_mut().unwrap().token = token.clone();
    shell.profile_edit.sections.draft.as_mut().unwrap().token = token.clone();
    put(
        &mut shell,
        &token,
        &response(ID, "Canonical canvas", "quip"),
    );
    (shell, token)
}
fn publish(shell: &mut ShellState, token: &Token, text: &str) {
    let request = CanvasSectionsRequest::new(ID, &[SECTION.into()]).unwrap();
    let read = shared::Read::capture(shell, &token.owner.team, request).unwrap();
    let reply=read.request().decode(json!({"ok":true,"sections":[{"id":SECTION,"rich_text_blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":text}]}]}]}]}));
    read.publish(shell, &reply).unwrap();
}
fn raw() -> Value {
    json!({"type":"canvas","file_id":ID,"section_id":SECTION,"url":"https://fixture.invalid/canvas","text":"https://fixture.invalid/canvas","opaque":{"retain":7},"style":{"bold":true}})
}
#[test]
fn section_missing_is_not_file_metadata_and_exact_units_survive_mounted_source_change() {
    let (mut shell, token) = authenticated();
    let wire = raw();
    let mut observed = Observation::capture(&shell, &token, ID).unwrap();
    assert!(observed.label(&wire).is_err());
    assert_eq!(
        String::from_utf16(&observed.section_label(&shell, &wire).unwrap()).unwrap(),
        "Canonical canvas: \u{2026}"
    );
    publish(
        &mut shell,
        &token,
        &format!("{}\u{1f600} tail", "a".repeat(24)),
    );
    assert!(!observed.current(&shell));
    let mut observed = Observation::capture(&shell, &token, ID).unwrap();
    let units = observed.section_label(&shell, &wire).unwrap();
    assert_eq!(&units[units.len() - 2..], &[0xd83d, 0x2026]);
    let mut catalog = catalog::Catalog::default();
    catalog.insert_section(&wire, &units).unwrap();
    publish(&mut shell, &token, "Later section");
    assert!(!observed.current(&shell));
    let witness = catalog::Witness {
        value: wire.clone(),
        label: String::new(),
        label_units: Some(units),
        presentation: Some(catalog::Presentation::canvas_section()),
    };
    let context = catalog.context(&[witness], &token.owner).unwrap();
    let changed = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Saved "},wire]}]}),
    ];
    let form = shell
        .profile_edit
        .sections
        .draft
        .as_mut()
        .unwrap()
        .form
        .as_mut()
        .unwrap();
    assert!(form.change_rich("long", changed.clone(), context).is_ok());
    assert!(form.validation("long").is_ok());
    assert!(form.capture().is_ok());
    assert!(form.value("long") == Some(&SectionValue::LongText(changed)));
}
#[test]
fn actual_section_source_publication_retires_pre_mount_opening_without_changing_draft() {
    let (mut shell, token) = authenticated();
    publish(&mut shell, &token, "First section");
    let blocks = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[raw()]}]}),
    ];
    fixture::mentions::set_blocks(&mut shell, blocks.clone());
    let view = crate::profile_edit::sections::presentation(&shell).unwrap();
    let opening = opening::begin(&mut shell, view.visit, view.serial, "long", 73).unwrap();
    publish(&mut shell, &token, "New section");
    assert!(!opening::finish(&mut shell, opening));
    assert_eq!(fixture::mentions::snapshot(&shell)["blocks"], json!(blocks));
    let opening = opening::begin(&mut shell, view.visit, view.serial, "long", 74).unwrap();
    assert!(opening::finish(&mut shell, opening));
}
