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
    shell
}
fn envelope(id: &str, mode: &str, title: &str) -> Value {
    json!({"ok":true,"file":{"id":id,"mode":mode,"filetype":mode,"created":101,
        "title":title,"permalink":"https://fixture.invalid/canonical","opaque":{"retain":true}},
        "comments":{"opaque":[1,2]},"paging":{"page":1},"receipt":"actual singleton"})
}
fn capture(shell: &mut ShellState, id: &str) -> FileInfoRead {
    FileInfoRead::capture(shell, TEAM, id).unwrap()
}
fn snapshot(shell: &ShellState) -> Arc<Snapshot> {
    Snapshot::capture(shell, TEAM).unwrap()
}
fn token(id: &str, mode: &str) -> Value {
    json!({"type":if mode=="quip" {"canvas"} else {"file"},"file_id":id,
        "text":"Untrusted fallback","url":"https://untrusted.invalid/token","style":{"bold":true},"opaque":{"keep":1}})
}
fn event(shell: &mut ShellState, id: &str) {
    let generation = shell.core.workspaces[TEAM].rt_generation;
    let raw = RawEvent {
        kind: "file_change".into(),
        rest: BTreeMap::from([("file_id".into(), json!(id))]),
    };
    let _ = crate::file_events::prepare(shell, TEAM, generation, &RtEvent::Unknown(raw));
}

#[test]
fn actual_singleton_receipt_enables_existing_target_without_query_or_raw_rewrite() {
    for mode in ["quip", "list"] {
        let mut shell = shell();
        let original = envelope("FSINGLE01", mode, "Canonical title");
        let read = capture(&mut shell, "FSINGLE01");
        assert!(read.current(&shell));
        assert!(shell.profile_edit.sections.file_search.queries.is_empty());
        read.publish_value(&mut shell, &original).unwrap();
        let source = snapshot(&shell);
        assert_eq!(source.rows().len(), 1);
        assert_eq!(source.rows()[0].receipt(), &original);
        assert_eq!(source.rows()[0].source(), &original["file"]);
        assert_eq!(source.title("FSINGLE01").unwrap().receipt(), &original);
        let raw = token("FSINGLE01", mode);
        let target =
            crate::profile_edit::sections::rich::search::FileTarget::capture(&shell, TEAM, &raw)
                .unwrap();
        assert_eq!(target.raw(), &raw);
        assert_eq!(target.file().title.as_deref(), Some("Canonical title"));
        assert!(target.current(&shell));
        assert!(shell.profile_edit.sections.file_search.queries.is_empty());
    }
}

#[test]
fn malformed_error_raw_object_batch_and_wrong_id_do_not_mutate_previous_source() {
    let good = envelope("F1", "quip", "Before");
    for bad in [
        json!(null),
        json!({"ok":false,"error":"not_found"}),
        json!({"ok":true,"file":[]}),
        json!({"ok":true,"files":[good["file"]]}),
        good["file"].clone(),
        envelope("FOTHER", "quip", "Other"),
        json!({"ok":true,"error":"bad","file":good["file"]}),
    ] {
        let mut shell = shell();
        capture(&mut shell, "F1")
            .publish_value(&mut shell, &good)
            .unwrap();
        let before = snapshot(&shell);
        assert!(
            capture(&mut shell, "F1")
                .publish_value(&mut shell, &bad)
                .is_err()
        );
        assert!(before.current(&shell), "{bad}");
        assert_eq!(snapshot(&shell).source("F1"), Some(&good["file"]));
    }
}

#[test]
fn original_account_credentials_transport_epoch_socket_and_source_fence_refuse_late_write() {
    for mutation in 0..8 {
        let mut shell = shell();
        let read = capture(&mut shell, "F1");
        match mutation {
            0 => shell.core.active_account = Some("successor".into()),
            1 => shell.core.session.as_mut().unwrap().d_cookie = "successor".into(),
            2 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(TEAM)
                    .unwrap()
                    .token = "successor".into()
            }
            3 => {
                shell.core.transport = Some(Arc::new(Transport::disconnected_fixture("successor")))
            }
            4 => shell.core.account_epoch += 1,
            5 => shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1,
            6 => shell.profile_edit.sections.file_search = State::default(),
            _ => {
                capture(&mut shell, "F2")
                    .publish_value(&mut shell, &envelope("F2", "list", "Other"))
                    .unwrap();
            }
        }
        assert!(!read.current(&shell), "{mutation}");
        assert!(
            read.publish_value(&mut shell, &envelope("F1", "quip", "Late"))
                .is_err()
        );
        assert!(
            shell
                .profile_edit
                .sections
                .file_search
                .slots
                .get(TEAM)
                .is_none_or(|s| s.store.source("F1").is_none())
        );
    }
}

#[test]
fn initially_absent_local_invalidation_retires_read_and_fresh_request_can_publish() {
    let mut shell = shell();
    let old = capture(&mut shell, "F1");
    invalidate(&mut shell, TEAM, "F1");
    assert!(!old.current(&shell));
    assert!(
        old.publish_value(&mut shell, &envelope("F1", "quip", "Old"))
            .is_err()
    );
    assert!(snapshot(&shell).source("F1").is_none());
    capture(&mut shell, "F1")
        .publish_value(&mut shell, &envelope("F1", "quip", "Fresh"))
        .unwrap();
    assert_eq!(
        snapshot(&shell).title("F1").unwrap().effective()["title"],
        "Fresh"
    );
}

#[test]
fn initially_absent_socket_event_or_tombstone_blocks_original_response() {
    for deleted in [false, true] {
        let mut shell = shell();
        let old = capture(&mut shell, "F1");
        if deleted {
            shell
                .core
                .workspaces
                .get_mut(TEAM)
                .unwrap()
                .deleted_files
                .insert("F1".into());
        } else {
            event(&mut shell, "F1");
        }
        assert!(!old.current(&shell));
        assert!(
            old.publish_value(&mut shell, &envelope("F1", "quip", "Old"))
                .is_err()
        );
        assert!(snapshot(&shell).source("F1").is_none());
    }
}

#[test]
fn unsupported_update_installs_original_poison_and_cannot_revive_old_title() {
    let mut shell = shell();
    capture(&mut shell, "F1")
        .publish_value(&mut shell, &envelope("F1", "quip", "Old"))
        .unwrap();
    let before = snapshot(&shell);
    let unsupported = json!({"ok":true,"file":{"id":"F1","title":"Hidden","is_hidden_by_limit":true},"receipt":"unsupported"});
    assert!(
        capture(&mut shell, "F1")
            .publish_value(&mut shell, &unsupported)
            .is_err()
    );
    assert!(!before.current(&shell));
    let after = snapshot(&shell);
    assert_eq!(after.source("F1"), Some(&unsupported["file"]));
    assert!(after.title("F1").is_err());
    assert!(after.rows().is_empty());
    assert!(
        capture(&mut shell, "F1")
            .publish_value(&mut shell, &envelope("F1", "quip", "Guess"))
            .is_err()
    );
    assert!(snapshot(&shell).title("F1").is_err());
}

#[test]
fn sequential_ids_and_partial_rename_preserve_predecessors_and_search_cache() {
    let mut shell = shell();
    capture(&mut shell, "F1")
        .publish_value(&mut shell, &envelope("F1", "quip", "Before"))
        .unwrap();
    shell
        .profile_edit
        .sections
        .file_search
        .queries
        .insert(TEAM.into(), files::QueryCache::new(100));
    shell
        .profile_edit
        .sections
        .file_search
        .queries
        .get_mut(TEAM)
        .unwrap()
        .add("existing", 100);
    capture(&mut shell, "F2")
        .publish_value(&mut shell, &envelope("F2", "list", "Other"))
        .unwrap();
    let old = snapshot(&shell);
    let old_target = crate::profile_edit::sections::rich::search::FileTarget::capture(
        &shell,
        TEAM,
        &token("F1", "quip"),
    )
    .unwrap();
    let rename = json!({"ok":true,"file":{"id":"F1","title":"After"},"receipt":"rename"});
    capture(&mut shell, "F1")
        .publish_value(&mut shell, &rename)
        .unwrap();
    let fresh = snapshot(&shell);
    assert!(!old.current(&shell));
    assert!(!old_target.current(&shell));
    assert_eq!(
        fresh.rows().iter().map(|r| r.id()).collect::<Vec<_>>(),
        ["F1", "F2"]
    );
    assert_eq!(fresh.title("F1").unwrap().effective()["mode"], "quip");
    assert_eq!(fresh.title("F1").unwrap().receipt(), &rename);
    assert_eq!(fresh.title("F2").unwrap().effective()["title"], "Other");
    assert!(
        shell
            .profile_edit
            .sections
            .file_search
            .queries
            .get_mut(TEAM)
            .unwrap()
            .contains("existing", 110)
    );
}

#[test]
fn invalid_requested_identity_and_native_kind_mismatch_never_grant_target() {
    let mut shell = shell();
    for id in ["", "X1", "F bad", "F", "F/1"] {
        assert!(FileInfoRead::capture(&mut shell, TEAM, id).is_err());
    }
    assert!(shell.profile_edit.sections.file_search.slots.is_empty());
    capture(&mut shell, "F1")
        .publish_value(&mut shell, &envelope("F1", "quip", "Canvas"))
        .unwrap();
    assert!(
        crate::profile_edit::sections::rich::search::FileTarget::capture(
            &shell,
            TEAM,
            &token("F1", "list")
        )
        .is_err()
    );
}

// Generated semantic fixtures have no channel_map. Original order evidence uses
// literal bytes in the separate shared-channel test below.
impl FileInfoRead {
    fn publish_value(self, shell: &mut ShellState, value: &Value) -> Result<(), Error> {
        let received = source::OrderedFileInfo::decode(&serde_json::to_vec(value).unwrap())?;
        self.publish(shell, &received)
    }
}
#[test]
fn actual_ordered_singleton_shared_channels_enable_file_target() {
    let mut shell = shell();
    let received=source::OrderedFileInfo::decode(br#"{"ok":true,"file":{"id":"FSHARED","mode":"quip","filetype":"quip","created":101,"title":"Shared Canvas","permalink":"https://fixture.invalid/shared","channels":["CTAIL"],"channel_map":{"z":{"id":"CZ","team":"TSINGLE01"},"a":{"id":"CA","team":"TSINGLE01"},"foreign":{"id":"COTHER","team":"TOTHER"}}}}"#).unwrap();
    capture(&mut shell, "FSHARED")
        .publish(&mut shell, &received)
        .unwrap();
    let snapshot = snapshot(&shell);
    let row = &snapshot.rows()[0];
    assert_eq!(row.channel_map_keys().unwrap(), ["z", "a", "foreign"]);
    assert_eq!(row.effective()["channels"], json!(["CZ", "CA", "CTAIL"]));
    assert_eq!(row.receipt(), received.raw());
    let target = crate::profile_edit::sections::rich::search::FileTarget::capture(
        &shell,
        TEAM,
        &token("FSHARED", "quip"),
    )
    .unwrap();
    assert!(target.current(&shell));
    assert_eq!(target.file().title.as_deref(), Some("Shared Canvas"));
    assert!(shell.profile_edit.sections.file_search.queries.is_empty());
}
