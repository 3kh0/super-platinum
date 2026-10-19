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
const TEAM: &str = "TFILES01";

fn ordered_shared_file() -> files::OrderedResponse {
    files::OrderedResponse::decode(br#"{"ok":true,"files":[{"id":"FORDER","mode":"quip","filetype":"quip","created":101,"title":"Shared Canvas","channels":["CTAIL"],"channel_map":{"z":{"id":"CZFIRST","team":"TFILES01"},"a":{"id":"CASECOND","team":"TFILES01"}}}]}"#).unwrap()
}

#[test]
fn same_byte_shared_file_order_survives_native_publication_and_retirement() {
    let mut shell = setup();
    let response = ordered_shared_file();
    let receipt = read(&mut shell);
    let published = receipt
        .publish_ordered(
            &mut shell,
            response.raw(),
            response.rows(),
            Some(response.channel_map_order()),
        )
        .unwrap();
    assert_eq!(
        published.rows()[0].effective()["channels"],
        json!(["CZFIRST", "CASECOND", "CTAIL"])
    );
    assert_eq!(published.rows()[0].source(), &response.rows()[0]);
    assert!(published.current(&shell));
    event(&mut shell, "FORDER");
    assert!(!published.current(&shell));
    assert!(
        receipt
            .publish_ordered(
                &mut shell,
                response.raw(),
                response.rows(),
                Some(response.channel_map_order())
            )
            .is_err()
    );
}

#[test]
fn ordered_file_response_cannot_borrow_foreign_carrier_or_successor_socket() {
    for socket in [false, true] {
        let mut shell = setup();
        let receipt = read(&mut shell);
        let response = ordered_shared_file();
        let mut carrier = response.raw().clone();
        if socket {
            shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1;
        } else {
            carrier["files"][0]["title"] = json!("Different response");
        }
        let rows = carrier["files"].as_array().unwrap();
        assert!(
            receipt
                .publish_ordered(
                    &mut shell,
                    &carrier,
                    rows,
                    Some(response.channel_map_order())
                )
                .is_err()
        );
        assert!(
            shell.profile_edit.sections.file_search.slots[TEAM]
                .store
                .source("FORDER")
                .is_none()
        );
    }
}
fn setup() -> ShellState {
    let mut shell = ShellState::login(crate::media::MediaRegistry::default());
    let owner = WorkspaceSession {
        team_id: TEAM.into(),
        user_id: "UFILES01".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.invalid".into(),
        token: "synthetic".into(),
    };
    shell.core.active_account = Some("A".into());
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
fn read(shell: &mut ShellState) -> Fence {
    let owner = Arc::new(Context::capture(shell, TEAM).unwrap());
    Fence::capture(shell, owner, TEAM.into(), 100).unwrap()
}
fn row(id: &str, title: &str) -> Value {
    json!({"id":id,"mode":"quip","filetype":"quip","created":101,"title":title,"opaque":{"retain":true}})
}
fn apply(shell: &mut ShellState, read: &Fence, rows: Vec<Value>) -> Result<Arc<Snapshot>, Error> {
    read.publish(
        shell,
        &json!({"ok":true,"files":rows,"opaqueCarrier":true}),
        &rows,
    )
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
fn exact_batch_title_pair_and_duplicate_predecessor_are_shared() {
    let mut shell = setup();
    let initial = read(&mut shell);
    let first = apply(&mut shell, &initial, vec![row("F1", "Before")]).unwrap();
    let next = read(&mut shell);
    let rows = vec![
        json!({"id":"F1","title":"First","actions":[{"ts":"30"}]}),
        json!({"id":"F1","title":"Final","actions":[{"ts":"10"}]}),
    ];
    let published = apply(&mut shell, &next, rows.clone()).unwrap();
    assert!(!first.current(&shell));
    assert!(published.current(&shell));
    assert_eq!(published.rows()[0].source(), &rows[1]);
    assert_eq!(published.rows()[0].effective()["title"], "Final");
    assert_eq!(published.rows()[0].effective()["actions"][0]["ts"], "10");
    assert_eq!(
        published.title("F1").unwrap().source(),
        published.rows()[0].source()
    );
    assert_eq!(published.title("F1").unwrap().effective()["title"], "Final");
    assert_eq!(published.rows()[0].receipt()["opaqueCarrier"], true);
}

#[test]
fn original_source_receipt_refuses_sibling_publication_and_malformed_batch() {
    let mut shell = setup();
    let a = read(&mut shell);
    let b = read(&mut shell);
    let current = apply(&mut shell, &b, vec![row("F1", "New")]).unwrap();
    assert!(apply(&mut shell, &a, vec![row("F2", "Old")]).is_err());
    let fresh = read(&mut shell);
    assert!(
        fresh
            .publish(
                &mut shell,
                &json!({"ok":true,"files":[]}),
                &[row("F2", "Foreign")]
            )
            .is_err()
    );
    assert!(current.current(&shell));
    assert!(current.source("F2").is_none());
}

#[test]
fn account_cookie_token_transport_epoch_and_socket_retire_original_read() {
    for mutation in 0..6 {
        let mut shell = setup();
        let original = read(&mut shell);
        match mutation {
            0 => shell.core.active_account = Some("B".into()),
            1 => shell.core.session.as_mut().unwrap().d_cookie = "changed".into(),
            2 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(TEAM)
                    .unwrap()
                    .token = "changed".into()
            }
            3 => {
                shell.core.transport = Some(Arc::new(Transport::disconnected_fixture("successor")))
            }
            4 => shell.core.account_epoch += 1,
            _ => shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1,
        }
        assert!(!original.current(&shell), "mutation {mutation}");
        assert!(apply(&mut shell, &original, vec![row("F1", "Late")]).is_err());
        assert!(
            shell.profile_edit.sections.file_search.slots[TEAM]
                .store
                .source("F1")
                .is_none()
        );
    }
}

#[test]
fn reconnect_keeps_predecessors_and_query_history_but_not_old_read_authority() {
    let mut shell = setup();
    let original = read(&mut shell);
    let old = apply(&mut shell, &original, vec![row("F1", "Before")]).unwrap();
    shell
        .profile_edit
        .sections
        .file_search
        .queries
        .get_mut(TEAM)
        .unwrap()
        .add("before", 100);
    shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1;
    assert!(!old.current(&shell));
    let fresh = read(&mut shell);
    let cold = Snapshot::capture(&shell, TEAM).unwrap();
    assert!(cold.rows().is_empty());
    assert_eq!(cold.unavailable_ids(), ["F1"]);
    assert_eq!(cold.source("F1").unwrap()["title"], "Before");
    assert!(
        shell
            .profile_edit
            .sections
            .file_search
            .queries
            .get_mut(TEAM)
            .unwrap()
            .contains("before", 110)
    );
    let current = apply(&mut shell, &fresh, vec![json!({"id":"F1","title":"After"})]).unwrap();
    assert_eq!(current.rows()[0].effective()["mode"], "quip");
    assert_eq!(current.rows()[0].effective()["title"], "After");
    assert!(current.unavailable_ids().is_empty());
}

#[test]
fn file_events_cover_previously_absent_and_embedded_ids_without_late_publication() {
    for embedded in [false, true] {
        let mut shell = setup();
        let original = read(&mut shell);
        event(&mut shell, "F2");
        let rows = if embedded {
            let mut parent = row("F1", "Parent");
            parent["canvas_metadata"] = json!({"items":[{"file":row("F2","Child")}]});
            vec![parent]
        } else {
            vec![row("F2", "Late")]
        };
        assert!(apply(&mut shell, &original, rows).is_err());
        assert!(
            shell.profile_edit.sections.file_search.slots[TEAM]
                .store
                .source("F1")
                .is_none()
        );
        assert!(
            shell.profile_edit.sections.file_search.slots[TEAM]
                .store
                .source("F2")
                .is_none()
        );
    }
}

#[test]
fn local_invalidation_keeps_preimage_and_requires_fresh_actual_response() {
    let mut shell = setup();
    let initial = read(&mut shell);
    let old = apply(&mut shell, &initial, vec![row("F1", "Before")]).unwrap();
    let pending = read(&mut shell);
    invalidate(&mut shell, TEAM, "F1");
    assert!(!old.current(&shell));
    assert!(!pending.current(&shell));
    let captured = Snapshot::capture(&shell, TEAM).unwrap();
    assert_eq!(captured.unavailable_ids(), ["F1"]);
    assert!(captured.title("F1").is_err());
    assert_eq!(captured.source("F1").unwrap()["title"], "Before");
    let fresh = read(&mut shell);
    let new = apply(&mut shell, &fresh, vec![json!({"id":"F1","title":"After"})]).unwrap();
    assert!(new.current(&shell));
    assert!(new.unavailable_ids().is_empty());
    assert_eq!(new.rows()[0].effective()["created"], 101);
}

#[test]
fn unsupported_or_out_of_category_rows_are_explicit_not_silently_filtered() {
    let mut shell = setup();
    let captured = read(&mut shell);
    let mut unsupported = row("F2", "Template");
    unsupported["canvas_template_mode"] = json!("template");
    let hosted = json!({"id":"F3","mode":"hosted","filetype":"pdf","title":"PDF","created":100});
    let snapshot = apply(
        &mut shell,
        &captured,
        vec![row("F1", "Canvas"), unsupported.clone(), hosted.clone()],
    )
    .unwrap();
    assert_eq!(
        snapshot.rows().iter().map(|r| r.id()).collect::<Vec<_>>(),
        ["F1"]
    );
    assert_eq!(snapshot.unavailable_ids(), ["F2"]);
    assert_eq!(snapshot.excluded_ids(), ["F3"]);
    assert_eq!(snapshot.source("F2"), Some(&unsupported));
    assert_eq!(snapshot.source("F3"), Some(&hosted));
    assert!(snapshot.title("F2").is_err());
    assert!(snapshot.title("F3").is_err());
}

#[test]
fn cache_scope_survives_profile_lifecycle_and_is_retired_on_credentials_change() {
    let mut shell = setup();
    let original = read(&mut shell);
    let snapshot = apply(&mut shell, &original, vec![row("F1", "Before")]).unwrap();
    shell
        .profile_edit
        .sections
        .file_search
        .queries
        .get_mut(TEAM)
        .unwrap()
        .add_as_prefix("can", 100);
    shell.profile_edit.draft = None;
    shell.profile_edit.sections.draft = None;
    assert!(snapshot.current(&shell));
    let _another = read(&mut shell);
    assert!(
        shell
            .profile_edit
            .sections
            .file_search
            .queries
            .get_mut(TEAM)
            .unwrap()
            .contains_prefix_of("canvas", 110)
    );
    shell.core.session.as_mut().unwrap().d_cookie = "replacement".into();
    let _replacement = read(&mut shell);
    assert!(!snapshot.current(&shell));
    let state = &mut shell.profile_edit.sections.file_search;
    assert!(state.slots[TEAM].store.source("F1").is_none());
    assert!(
        !state
            .queries
            .get_mut(TEAM)
            .unwrap()
            .contains_prefix_of("canvas", 120)
    );
}

#[test]
fn positive_prefix_precedes_failed_upsert_but_stale_ids_never_record_success() {
    let mut shell = setup();
    let initial = read(&mut shell);
    let full = (1..=128)
        .map(|n| row(&format!("F{n}"), "Existing"))
        .collect();
    apply(&mut shell, &initial, full).unwrap();
    let next = read(&mut shell);
    let rows = vec![row("F129", "Beyond native cache capacity")];
    let carrier = json!({"ok":true,"files":rows});
    next.record_response(&mut shell, "capacity", &carrier, &rows, None, 200)
        .unwrap();
    assert!(next.publish(&mut shell, &carrier, &rows).is_err());
    let cache = shell
        .profile_edit
        .sections
        .file_search
        .queries
        .get_mut(TEAM)
        .unwrap();
    assert!(cache.contains_prefix_of("capacity-more", 201));
    assert!(!cache.contains("capacity", 201));
    for embedded in [false, true] {
        let pending = read(&mut shell);
        event(&mut shell, "F130");
        let rows = if embedded {
            let mut parent = row("F1", "Parent");
            parent["canvas_metadata"] = json!({"items":[{"file":row("F130","Child")}]});
            vec![parent]
        } else {
            vec![row("F130", "Late")]
        };
        assert!(
            pending
                .record_response(
                    &mut shell,
                    "stale",
                    &json!({"ok":true,"files":rows}),
                    &rows,
                    None,
                    210
                )
                .is_err()
        );
        assert!(
            !shell
                .profile_edit
                .sections
                .file_search
                .queries
                .get_mut(TEAM)
                .unwrap()
                .contains_prefix_of("stale-more", 211)
        );
    }
}

#[test]
fn excluded_receipt_is_not_reauthorized_after_socket_or_file_mutation() {
    for socket in [false, true] {
        let mut shell = setup();
        let captured = read(&mut shell);
        let hosted = json!({"id":"F3","mode":"hosted","filetype":"pdf","created":1});
        let before = apply(&mut shell, &captured, vec![hosted.clone()]).unwrap();
        assert_eq!(before.excluded_ids(), ["F3"]);
        assert!(before.unavailable_ids().is_empty());
        if socket {
            shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1;
        } else {
            invalidate(&mut shell, TEAM, "F3");
        }
        assert!(!before.current(&shell));
        let current = Snapshot::capture(&shell, TEAM).unwrap();
        assert!(current.excluded_ids().is_empty());
        assert_eq!(current.unavailable_ids(), ["F3"]);
        assert_eq!(current.source("F3"), Some(&hosted));
    }
}
