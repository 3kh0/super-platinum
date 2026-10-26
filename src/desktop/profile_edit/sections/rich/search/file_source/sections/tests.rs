use super::*;
use serde_json::json;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::Transport,
    state::Workspace,
};
const TEAM: &str = "TSECTION01";
fn shell() -> ShellState {
    let mut shell = ShellState::login(crate::media::MediaRegistry::default());
    let session = WorkspaceSession {
        team_id: TEAM.into(),
        user_id: "USECTION01".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.invalid".into(),
        token: "synthetic".into(),
    };
    shell.core.active_account = Some("section-fixture".into());
    shell.core.active_team = Some(TEAM.into());
    shell.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        unified_user_client_enabled: BTreeMap::from([(TEAM.into(), true)]),
        workspaces: BTreeMap::from([(TEAM.into(), session.clone())]),
    });
    shell.core.transport = Some(Arc::new(Transport::disconnected_fixture("synthetic")));
    shell
        .core
        .workspaces
        .insert(TEAM.into(), Workspace::from_session(&session));
    shell
}
fn read(shell: &mut ShellState, file: &str) -> Read {
    Read::capture(
        shell,
        TEAM,
        CanvasSectionsRequest::new(file, &["s1".into()]).unwrap(),
    )
    .unwrap()
}
fn publish(shell: &mut ShellState, file: &str, text: &str) {
    let read = read(shell, file);
    let response = read.request().decode(json!({"ok":true,"sections":[{"id":"s1","rich_text_blocks":[
        {"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":text}]}]}
    ]}]}));
    read.publish(shell, &response).unwrap();
}
#[test]
fn section_source_preserves_omission_and_undefined_and_current_failure_clears_only_file() {
    let mut shell = shell();
    let empty = read(&mut shell, "FSECTION1");
    let absent = Observation::capture(&shell, TEAM, "FSECTION1", "s1").unwrap();
    assert!(!absent.row().has_entry());
    let omitted = empty.request().decode(json!({"ok":true,"sections":[]}));
    empty.publish(&mut shell, &omitted).unwrap();
    assert!(absent.current(&shell));
    publish(&mut shell, "FSECTION1", "One");
    publish(&mut shell, "FSECTION2", "Two");
    let first = Observation::capture(&shell, TEAM, "FSECTION1", "s1").unwrap();
    let other = Observation::capture(&shell, TEAM, "FSECTION2", "s1").unwrap();
    let blank = read(&mut shell, "FSECTION1");
    let response = blank
        .request()
        .decode(json!({"ok":true,"sections":[{"id":"s1","rich_text_blocks":[]}]}));
    blank.publish(&mut shell, &response).unwrap();
    assert!(!first.current(&shell));
    let undefined = Observation::capture(&shell, TEAM, "FSECTION1", "s1").unwrap();
    assert!(undefined.row().has_entry());
    assert!(undefined.row().block().is_none());
    let failed = read(&mut shell, "FSECTION1");
    let response = failed.request().failed();
    failed.publish(&mut shell, &response).unwrap();
    assert!(!undefined.current(&shell));
    assert!(
        !Observation::capture(&shell, TEAM, "FSECTION1", "s1")
            .unwrap()
            .row()
            .has_entry()
    );
    assert!(other.current(&shell));
    assert!(other.row().block().is_some());
}
#[test]
fn stale_socket_account_event_and_successor_response_cannot_clear_section_source() {
    for boundary in ["socket", "account", "event", "successor"] {
        let mut shell = shell();
        publish(&mut shell, "FSECTION1", "Original");
        let held = read(&mut shell, "FSECTION1");
        let response = held.request().failed();
        match boundary {
            "socket" => shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1,
            "account" => shell.core.account_epoch += 1,
            "event" => crate::file_events::invalidate(&mut shell, TEAM, "FSECTION1"),
            "successor" => publish(&mut shell, "FSECTION1", "Successor"),
            _ => unreachable!(),
        }
        let before = shell.profile_edit.sections.file_search.slots[TEAM]
            .section_titles
            .observe("FSECTION1", "s1")
            .unwrap();
        assert!(!held.current(&shell), "{boundary}");
        assert!(held.publish(&mut shell, &response).is_err(), "{boundary}");
        assert!(
            before.current(&shell.profile_edit.sections.file_search.slots[TEAM].section_titles),
            "{boundary}"
        );
        assert!(before.block().is_some(), "{boundary}");
    }
}
#[test]
fn section_publication_requires_exact_request_and_metadata_predecessor() {
    let mut shell = shell();
    let held = read(&mut shell, "FSECTION1");
    let foreign = CanvasSectionsRequest::new("FSECTION1", &["s1".into()])
        .unwrap()
        .failed();
    assert!(held.publish(&mut shell, &foreign).is_err());
    let held = read(&mut shell, "FSECTION1");
    let response = held.request().failed();
    let metadata = reference::ReferenceRead::capture(&mut shell, TEAM, "FOTHER1").unwrap();
    let raw = source::FileReferenceResponse::decode(
        "FOTHER1",
        br#"{"ok":true,"file":{"id":"FOTHER1","mode":"quip","title":"Other"}}"#,
    )
    .unwrap();
    metadata.publish(&mut shell, &raw).unwrap();
    assert!(!held.current(&shell));
    assert!(held.publish(&mut shell, &response).is_err());
}
