use super::*;
use crate::media::MediaRegistry;
use serde_json::json;
use std::collections::BTreeMap;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::{Transport, models::UserGroup},
    state::Workspace,
};
const TEAM: &str = "TGROUP001";
const A: &str = "SGROUP001";
const B: &str = "SGROUP002";
fn shell() -> ShellState {
    let mut shell = ShellState::login(MediaRegistry::default());
    let session = WorkspaceSession {
        team_id: TEAM.into(),
        user_id: "USELF0001".into(),
        enterprise_id: None,
        name: "Closed fixture".into(),
        url: "https://example.invalid".into(),
        token: "synthetic-token".into(),
    };
    shell.core.active_account = Some("group-query-fixture".into());
    shell.core.active_team = Some(TEAM.into());
    shell.core.session = Some(Session {
        d_cookie: "synthetic-cookie".into(),
        unified_user_client_enabled: BTreeMap::new(),
        workspaces: BTreeMap::from([(TEAM.into(), session.clone())]),
    });
    shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap()));
    let mut w = Workspace::from_session(&session);
    let boot = w.directory_usergroups.begin_boot(&w.usergroups).unwrap();
    let commit = w.directory_usergroups.observe_boot(
        boot, "group-query-fixture", &session, shell.core.account_epoch, &w.usergroups,
        &json!({"ok":true,"team":{"id":TEAM},"self":{"id":"USELF0001"},"subteams":{"self":[A]}}),
    ).unwrap();
    (w.directory_usergroups, w.usergroups) = commit.into_parts();
    shell.core.workspaces.insert(TEAM.into(), w);
    shell
}
fn pages(rows: Vec<Value>) -> groups::Pages {
    let mut pages = groups::Pages::default();
    pages
        .accept(json!({"ok":true,"results":rows}), None)
        .unwrap();
    assert!(pages.complete());
    pages
}
fn adopt(shell: &mut ShellState, rows: Vec<Value>, origin: intake::Origin) {
    let owner = Context::capture(shell, TEAM).unwrap();
    let w = shell.core.workspaces.get_mut(TEAM).unwrap();
    let ticket = w
        .directory_usergroups
        .enqueue(
            &owner.account,
            &owner.session,
            owner.epoch,
            "fixture-group-query",
            &w.usergroups,
            intake::Operation::Metadata {
                origin,
                rows,
                native_members: intake::NativeMembers::Preserve,
            },
        )
        .unwrap();
    let pending = w
        .directory_usergroups
        .next_for(
            &ticket,
            &owner.account,
            &owner.session,
            owner.epoch,
            &w.usergroups,
        )
        .unwrap()
        .unwrap();
    // Only ASCII fixture names: production normalization is the shared actual
    // locale renderer, never this test's deliberately restricted conversion.
    assert!(pending.fields().iter().all(|s| s.is_ascii()));
    let reply = source_core::RendererReply {
        generation: pending.generation().into(),
        fields: pending.fields().to_vec(),
        lowered: pending
            .fields()
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect(),
        locale: (!pending.fields().is_empty()).then(|| "en-US".into()),
    };
    let commit = w
        .directory_usergroups
        .commit(
            pending,
            reply,
            &owner.account,
            &owner.session,
            owner.epoch,
            &w.usergroups,
        )
        .unwrap();
    (w.directory_usergroups, w.usergroups) = commit.into_parts();
    assert!(ticket.finished(&w.directory_usergroups));
}
#[test]
fn accepted_raw_effective_pairs_reselect_duplicates_and_same_version_predecessor() {
    let mut shell = shell();
    adopt(
        &mut shell,
        vec![
            json!({"id":A,"name":" Old Name ","handle":"first","date_update":7,"opaque":{"keep":true}}),
            json!({"id":B,"name":"Second","handle":"second","date_update":2}),
        ],
        intake::Origin::Upsert,
    );
    let before = group_source::Snapshot::capture(&shell, TEAM).unwrap();
    let incoming = vec![
        json!({"id":B,"name":"Updated","date_update":3}),
        json!({"id":A,"name":"Ignored","date_update":7}),
        json!({"id":B,"handle":"last","date_update":4}),
    ];
    adopt(&mut shell, incoming.clone(), intake::Origin::Fetch);
    assert!(!before.current(&shell));
    let after = group_source::Snapshot::capture(&shell, TEAM).unwrap();
    let selected = after.reselect(&incoming).unwrap();
    assert_eq!(
        selected.iter().map(|r| r.id()).collect::<Vec<_>>(),
        vec![B, A, B]
    );
    assert_eq!(selected[1].effective()["name"], " Old Name ");
    assert_eq!(selected[1].effective()["_name_lc"], " old name ");
    assert_eq!(selected[1].source()["name"], " Old Name ");
    assert_eq!(selected[1].effective()["opaque"], json!({"keep":true}));
    assert_eq!(selected[0].effective(), selected[2].effective());
    assert_eq!(selected[0].effective()["handle"], "last");
    assert_eq!(after.rows().len(), 2);
    assert_eq!(after.membership(), Some([A.to_owned()].as_slice()));
    assert!(
        after
            .reselect(&[json!({"id":"SMISSING1"})])
            .unwrap()
            .is_empty()
    );
    let filtered = after
        .reselect(&[json!({"id":B}), json!({"id":"SMISSING1"}), json!({"id":B})])
        .unwrap();
    assert_eq!(
        filtered.iter().map(|r| r.id()).collect::<Vec<_>>(),
        vec![B, B]
    );
}
#[test]
fn original_socket_account_cookie_credentials_and_transport_retire_read_and_snapshot() {
    for kind in 0..6 {
        let mut shell = shell();
        let read = Read::capture(&shell, TEAM).unwrap();
        let snapshot = group_source::Snapshot::capture(&shell, TEAM).unwrap();
        match kind {
            0 => shell.core.active_account = Some("foreign".into()),
            1 => shell.core.account_epoch += 1,
            2 => shell.core.session.as_mut().unwrap().d_cookie = "different".into(),
            3 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(TEAM)
                    .unwrap()
                    .token = "different".into()
            }
            4 => shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap())),
            _ => shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1,
        }
        assert!(!read.current(&shell));
        assert!(!snapshot.current(&shell));
        assert!(group_source::Snapshot::under(&shell, read.owner.clone()).is_err());
        assert!(
            read.enqueue(&mut shell, &pages(vec![json!({"id":A,"handle":"late"})]))
                .is_err()
        );
    }
}
#[test]
fn native_preimage_drift_refuses_late_response_without_recapture() {
    let mut shell = shell();
    let read = Read::capture(&shell, TEAM).unwrap();
    shell
        .core
        .workspaces
        .get_mut(TEAM)
        .unwrap()
        .usergroups
        .insert(
            A.into(),
            UserGroup::from_observed(&json!({"id":A,"handle":"socket"})).unwrap(),
        );
    assert!(!read.current(&shell));
    assert!(
        read.enqueue(&mut shell, &pages(vec![json!({"id":A,"handle":"late"})]))
            .is_err()
    );
    assert_eq!(shell.core.workspaces[TEAM].usergroups[A].handle, "socket");
}
#[test]
fn queued_source_operation_retires_http_receipt_and_dropped_work_keeps_history_unknown() {
    let mut shell = shell();
    let native_before = serde_json::to_value(&shell.core.workspaces[TEAM].usergroups).unwrap();
    let read = Read::capture(&shell, TEAM).unwrap();
    let snapshot = group_source::Snapshot::capture(&shell, TEAM).unwrap();
    let work = read
        .enqueue(
            &mut shell,
            &pages(vec![json!({"id":A,"name":"Remote","handle":"remote"})]),
        )
        .unwrap();
    assert!(!read.current(&shell));
    assert!(!snapshot.current(&shell));
    assert!(Read::capture(&shell, TEAM).is_err());
    assert_eq!(
        serde_json::to_value(&shell.core.workspaces[TEAM].usergroups).unwrap(),
        native_before
    );
    drop(work);
    assert!(Read::capture(&shell, TEAM).is_err());
    assert!(group_source::Snapshot::capture(&shell, TEAM).is_err());
}
#[test]
fn incomplete_or_malformed_pages_never_reserve_a_source_write() {
    let mut shell = shell();
    let read = Read::capture(&shell, TEAM).unwrap();
    let mut pages = groups::Pages::default();
    pages
        .accept(
            json!({"ok":true,"results":[{"id":A}],"next_marker":"next"}),
            None,
        )
        .unwrap();
    assert!(!pages.complete());
    assert!(read.enqueue(&mut shell, &pages).is_err());
    assert!(
        pages
            .accept(json!({"ok":true,"results":false}), None)
            .is_err()
    );
    assert!(read.enqueue(&mut shell, &pages).is_err());
    assert!(read.current(&shell));
    assert!(
        !shell.core.workspaces[TEAM]
            .directory_usergroups
            .has_pending()
    );
}
#[test]
fn legacy_raw_label_rows_are_not_effective_group_history() {
    let mut shell = shell();
    let w = shell.core.workspaces.get_mut(TEAM).unwrap();
    w.directory_usergroups = source_core::Store::default();
    w.usergroups.insert(
        A.into(),
        UserGroup::from_observed(&json!({"id":A,"handle":"legacy"})).unwrap(),
    );
    assert!(Read::capture(&shell, TEAM).is_err());
    assert!(group_source::Snapshot::capture(&shell, TEAM).is_err());
    assert_eq!(shell.core.workspaces[TEAM].usergroups[A].handle, "legacy");
}
