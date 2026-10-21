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
const TEAM: &str = "TOBSERVED";
fn shell() -> ShellState {
    let mut shell = ShellState::login(crate::media::MediaRegistry::default());
    let owner = WorkspaceSession {
        team_id: TEAM.into(),
        user_id: "UOBSERVED".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.invalid".into(),
        token: "synthetic".into(),
    };
    shell.core.active_account = Some("file-observation-fixture".into());
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
    shell
}
fn envelope(id: &str, mode: &str) -> Value {
    json!({"ok":true,"file":{"id":id,"mode":mode,"filetype":mode,
        "created":101,"title":"Original &amp; title","name":"Different name",
        "permalink":"https://fixture.invalid/original","opaque":{"keep":[0,false,null]}},
        "originalEnvelope":{"keep":true}})
}
fn publish(shell: &mut ShellState, raw: &Value) -> Result<(), Error> {
    let read = FileInfoRead::capture(shell, TEAM, raw["file"]["id"].as_str().unwrap())?;
    let received = source::OrderedFileInfo::decode(raw.to_string().as_bytes())?;
    read.publish(shell, &received)
}
#[test]
fn observed_singleton_preserves_original_facts_without_token_or_query() {
    for kind in ["quip", "list"] {
        let mut shell = shell();
        assert!(FileObservation::capture(&shell, TEAM, "F1").is_err());
        assert!(shell.profile_edit.sections.file_search.slots.is_empty());
        let raw = envelope("F1", kind);
        publish(&mut shell, &raw).unwrap();
        let observed = FileObservation::capture(&shell, TEAM, "F1").unwrap();
        assert_eq!(observed.id(), "F1");
        assert_eq!(observed.source(), &raw["file"]);
        assert_eq!(observed.receipt(), &raw);
        assert_eq!(observed.effective()["title"], "Original &amp; title");
        assert_eq!(observed.effective()["is_deleted"], false);
        assert!(observed.source().get("is_deleted").is_none());
        assert!(observed.current(&shell));
        assert!(shell.profile_edit.sections.file_search.queries.is_empty());
    }
}
#[test]
fn sibling_publication_retires_collection_but_read_only_recapture_needs_no_fetch() {
    let mut shell = shell();
    publish(&mut shell, &envelope("F1", "list")).unwrap();
    let before = FileObservation::capture(&shell, TEAM, "F1").unwrap();
    publish(&mut shell, &envelope("F2", "quip")).unwrap();
    assert!(!before.current(&shell));
    let fresh = FileObservation::capture(&shell, TEAM, "F1").unwrap();
    assert_eq!(fresh.receipt(), before.receipt());
    assert!(fresh.current(&shell));
    // Equal value replacement still retires the original row witness.
    publish(&mut shell, &envelope("F1", "list")).unwrap();
    assert!(!fresh.current(&shell));
    assert!(shell.profile_edit.sections.file_search.queries.is_empty());
}
#[test]
fn original_same_team_account_socket_event_and_local_invalidation_guards_remain() {
    for change in 0..6 {
        let mut shell = shell();
        publish(&mut shell, &envelope("F1", "list")).unwrap();
        let observed = FileObservation::capture(&shell, TEAM, "F1").unwrap();
        match change {
            0 => shell.core.active_team = Some("TOTHER".into()),
            1 => shell.core.account_epoch += 1,
            2 => shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1,
            3 => invalidate(&mut shell, TEAM, "F1"),
            4 => {
                shell
                    .core
                    .workspaces
                    .get_mut(TEAM)
                    .unwrap()
                    .deleted_files
                    .insert("F1".into());
            }
            _ => {
                let generation = shell.core.workspaces[TEAM].rt_generation;
                let raw = RawEvent {
                    kind: "file_change".into(),
                    rest: BTreeMap::from([("file_id".into(), json!("F1"))]),
                };
                let _ = crate::file_events::prepare(
                    &mut shell,
                    TEAM,
                    generation,
                    &RtEvent::Unknown(raw),
                );
            }
        }
        assert!(!observed.current(&shell), "{change}");
        assert!(
            FileObservation::capture(&shell, TEAM, "F1").is_err(),
            "{change}"
        );
    }
}
