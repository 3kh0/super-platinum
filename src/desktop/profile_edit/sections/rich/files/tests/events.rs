use super::*;
use super_platinum_core::{
    config::Session,
    slack::{
        Transport,
        events::{RawEvent, RtEvent},
    },
};
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
    (shell, token)
}
fn delete(shell: &mut ShellState, token: &Token) {
    let event = RtEvent::Unknown(
        serde_json::from_value::<RawEvent>(json!({"type":"file_deleted","file_id":"FFILE0001"}))
            .unwrap(),
    );
    let generation = socket(shell, token).unwrap();
    crate::file_events::prepare(shell, &token.owner.team, generation, &event);
    crate::realtime::apply_in_view(&mut shell.core, &token.owner.team, generation, event, true);
}
#[test]
fn shared_event_metadata_retires_opening_but_preserves_mounted_catalog_and_raw_capture() {
    let (mut shell, token) = authenticated();
    let id = "FFILE0001";
    put(&mut shell, &token, &response(id, "Known canvas", "quip"));
    let raw = json!({"type":"canvas","file_id":id,"text":"https://fixture.invalid/docs/FFILE0001","opaque":{"keep":7},"style":{"bold":true}});
    fixture::mentions::set_blocks(
        &mut shell,
        vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[raw]}]}),
        ],
    );
    let view = crate::profile_edit::sections::presentation(&shell).unwrap();
    let opening = opening::begin(&mut shell, view.visit, view.serial, "long", 71).unwrap();
    assert!(opening::finish(&mut shell, opening));
    let before = shell
        .profile_edit
        .sections
        .draft
        .as_ref()
        .unwrap()
        .rich
        .as_ref()
        .unwrap()
        .catalog
        .clone();
    delete(&mut shell, &token);
    let mounted = shell
        .profile_edit
        .sections
        .draft
        .as_ref()
        .unwrap()
        .rich
        .as_ref()
        .unwrap();
    let witness = [catalog::Witness {
        label_units: None,
        value: raw.clone(),
        label: "Known canvas".into(),
        presentation: None,
    }];
    assert!(before.context(&witness, &token.owner).is_ok());
    assert!(mounted.catalog.context(&witness, &token.owner).is_ok());
    assert_eq!(mounted.catalog.presentations(), before.presentations());
    assert!(metadata_current(&shell, &token, id));
    assert!(reuse_metadata(&shell, &token, id, true));
    let gone = Observation::capture(&shell, &token, id).unwrap();
    assert!(gone.deleted_canvas());
    assert_eq!(gone.label(&raw).unwrap(), "Deleted canvas");
    let opening = opening::begin(&mut shell, view.visit, view.serial, "long", 72).unwrap();
    delete(&mut shell, &token);
    assert!(!opening::finish(&mut shell, opening));
    let current = opening::begin(&mut shell, view.visit, view.serial, "long", 73).unwrap();
    assert!(opening::finish(&mut shell, current));
    let mounted = shell
        .profile_edit
        .sections
        .draft
        .as_ref()
        .unwrap()
        .rich
        .as_ref()
        .unwrap();
    assert_eq!(
        mounted.catalog.presentations()[raw.to_string()]["kind"],
        "deleted-canvas"
    );
}
#[test]
fn shared_event_source_survives_visit_cache_drop_and_rejects_positive_resurrection() {
    let (mut shell, token) = authenticated();
    let id = "FFILE0001";
    put(&mut shell, &token, &response(id, "Known canvas", "quip"));
    delete(&mut shell, &token);
    shell.profile_edit.sections.file_titles = None;
    let old = Observation::capture(&shell, &token, id).unwrap();
    assert!(old.deleted_canvas());
    shell
        .core
        .workspaces
        .get_mut(&token.owner.team)
        .unwrap()
        .rt_generation += 1;
    assert!(!old.current(&shell));
    assert!(
        Observation::capture(&shell, &token, id)
            .unwrap()
            .deleted_canvas()
    );
    let raw = response(id, "Resurrection", "quip");
    assert!(install(&mut shell, &token, id, old.socket + 1, old.revision, &raw).is_err());
    assert!(
        Observation::capture(&shell, &token, id)
            .unwrap()
            .deleted_canvas()
    );
    shell.core.account_epoch += 1;
    assert!(!old.current(&shell));
    assert!(Observation::capture(&shell, &token, id).is_err());
}
