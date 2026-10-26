use super::*;
use serde_json::json;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::{
        Transport,
        events::{RawEvent, RtEvent},
    },
    state::Workspace,
};
const TEAM: &str = "TSINGLE01";

fn shell() -> ShellState {
    let mut shell = ShellState::login(crate::media::MediaRegistry::default());
    let owner = WorkspaceSession {
        team_id: TEAM.into(),
        user_id: "USINGLE01".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.invalid".into(),
        token: "synthetic".into(),
    };
    shell.core.active_account = Some("singleton-fixture".into());
    shell.core.active_team = Some(TEAM.into());
    shell.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        unified_user_client_enabled: BTreeMap::from([(TEAM.into(), true)]),
        workspaces: BTreeMap::from([(TEAM.into(), owner.clone())]),
    });
    shell.core.transport = Some(Arc::new(Transport::disconnected_fixture("synthetic")));
    shell
        .core
        .workspaces
        .insert(TEAM.into(), Workspace::from_session(&owner));
    let mut other = owner.clone();
    other.team_id = "TOTHER01".into();
    shell
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .insert(other.team_id.clone(), other.clone());
    shell
        .core
        .workspaces
        .insert(other.team_id.clone(), Workspace::from_session(&other));
    shell
}
fn envelope(id: &str, mode: &str, title: &str) -> Value {
    json!({"ok":true,"file":{"id":id,"mode":mode,"filetype":mode,"created":101,
        "title":title,"permalink":"https://fixture.invalid/canonical","opaque":{"retain":true}},
        "comments":{"opaque":[1,2]},"paging":{"page":1},"receipt":"actual singleton"})
}

fn publish(shell: &mut ShellState, id: &str, raw: &Value) {
    let read = ReferenceRead::capture(shell, TEAM, id).unwrap();
    let response =
        source::FileReferenceResponse::decode(id, &serde_json::to_vec(raw).unwrap()).unwrap();
    read.publish(shell, &response).unwrap();
}
fn event(shell: &mut ShellState, generation: u64, raw: Value) {
    let event = RtEvent::Unknown(serde_json::from_value::<RawEvent>(raw).unwrap());
    assert!(crate::file_events::prepare(shell, TEAM, generation, &event).is_none());
    crate::realtime::apply_in_view(&mut shell.core, TEAM, generation, event, true);
}
#[test]
fn event_reference_retains_actual_preimage_across_visits_and_sockets_not_opening_authority() {
    let mut s = shell();
    let raw = envelope("F1", "quip", "Before");
    publish(&mut s, "F1", &raw);
    let before = Reference::capture(&s, TEAM, "F1").unwrap();
    let frame = json!({"type":"file_deleted","file_id":"F1","opaque":{"keep":7}});
    event(&mut s, 0, frame.clone());
    assert!(!before.current(&s));
    let gone = Reference::capture(&s, TEAM, "F1").unwrap();
    assert!(gone.row().is_event_deletion());
    assert_eq!(gone.row().receipt(), &frame);
    let evidence = gone.row().deletion_event().unwrap();
    assert_eq!(evidence.preimage_source(), &raw["file"]);
    assert_eq!(evidence.preimage_receipt(), &raw);
    assert!(Snapshot::capture(&s, TEAM).unwrap().rows().is_empty());
    // Closing a Profile visit retires its local cache, not the shared source fact.
    s.profile_edit.sections.file_titles = None;
    s.profile_generation += 1;
    assert!(
        Reference::capture(&s, TEAM, "F1")
            .unwrap()
            .row()
            .is_event_deletion()
    );
    s.core.active_team = Some("TOTHER01".into());
    s.core.active_team = Some(TEAM.into());
    s.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1;
    assert!(!gone.current(&s));
    assert!(
        Reference::capture(&s, TEAM, "F1")
            .unwrap()
            .row()
            .is_event_deletion()
    );
    s.core.account_epoch += 1;
    assert!(Reference::capture(&s, TEAM, "F1").is_err());
}
#[test]
fn event_retires_held_positive_and_plural_precedence_masks_every_id_without_absent_source() {
    let mut s = shell();
    publish(&mut s, "F1", &envelope("F1", "quip", "One"));
    publish(&mut s, "F2", &envelope("F2", "quip", "Two"));
    let held = ReferenceRead::capture(&mut s, TEAM, "F1").unwrap();
    event(
        &mut s,
        0,
        json!({"type":"file_deleted","file_ids":[],"file_id":"F1"}),
    );
    assert!(held.current(&s));
    assert!(!s.core.workspaces[TEAM].deleted_files.contains("F1"));
    event(&mut s, 0, json!({"type":"file_deleted","file":{"id":"F1"}}));
    assert!(held.current(&s));
    let frame = json!({"type":"file_deleted","file_ids":["F1",null,false,"","F1","F2","FABSENT"],"file_id":"FIGNORED"});
    event(&mut s, 0, frame);
    assert!(!held.current(&s));
    let response = source::FileReferenceResponse::decode(
        "F1",
        &serde_json::to_vec(&envelope("F1", "quip", "Late resurrection")).unwrap(),
    )
    .unwrap();
    assert!(held.publish(&mut s, &response).is_err());
    for id in ["F1", "F2"] {
        assert!(
            Reference::capture(&s, TEAM, id)
                .unwrap()
                .row()
                .is_event_deletion()
        );
    }
    let w = &s.core.workspaces[TEAM];
    assert!(w.deleted_files.contains("FABSENT"));
    assert!(!w.deleted_files.contains("FIGNORED"));
    assert!(
        s.profile_edit.sections.file_search.slots[TEAM]
            .store
            .source("FABSENT")
            .is_none()
    );
    assert!(Reference::capture(&s, TEAM, "FABSENT").is_err());
}
#[test]
fn foreign_socket_local_bit_and_source_replacement_cannot_mint_event_presentation() {
    let mut s = shell();
    publish(&mut s, "F1", &envelope("F1", "quip", "One"));
    event(&mut s, 9, json!({"type":"file_deleted","file_id":"F1"}));
    assert!(!s.core.workspaces[TEAM].deleted_files.contains("F1"));
    assert!(
        !Reference::capture(&s, TEAM, "F1")
            .unwrap()
            .row()
            .is_event_deletion()
    );
    s.core.remove_file(TEAM, "F1");
    assert!(Reference::capture(&s, TEAM, "F1").is_err());
    assert!(ReferenceRead::capture(&mut s, TEAM, "F1").is_err());
    let parsed = FileDeletionEvent::decode(&json!({"type":"file_deleted","file_id":"F1"})).unwrap();
    let old = s.core.transport.clone();
    s.core.transport = Some(Arc::new(Transport::disconnected_fixture("other")));
    deletion(&mut s, TEAM, 0, &parsed);
    assert!(Reference::capture(&s, TEAM, "F1").is_err());
    s.core.transport = old;
    assert!(
        !s.profile_edit.sections.file_search.slots[TEAM]
            .store
            .reference("F1")
            .unwrap()
            .is_event_deletion()
    );
}
#[test]
fn shared_response_projection_is_authoritative_and_unrelated_publication_retires_read() {
    let mut s = shell();
    publish(&mut s, "F1", &envelope("F1", "quip", "One"));
    let read = ReferenceRead::capture(&mut s, TEAM, "F1").unwrap();
    publish(&mut s, "F2", &envelope("F2", "quip", "Two"));
    assert!(!read.current(&s));
    let deleted =
        source::FileReferenceResponse::decode("F1", br#"{"ok":false,"error":"file_deleted"}"#)
            .unwrap();
    ReferenceRead::capture(&mut s, TEAM, "F1")
        .unwrap()
        .publish(&mut s, &deleted)
        .unwrap();
    let api = Reference::capture(&s, TEAM, "F1").unwrap();
    assert!(!api.row().is_event_deletion());
    assert!(matches!(
        api.row().value(),
        source::ReferenceValue::DeletedCanvas
    ));
    s.core.remove_file(TEAM, "F1");
    assert!(!api.current(&s));
}
