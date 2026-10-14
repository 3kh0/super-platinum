use super::*;
use serde_json::json;
use std::sync::Arc;
use super_platinum_core::{
    config::{Session as Credentials, WorkspaceSession},
    slack::Transport,
};

fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "value".into(),
    }
}
fn setup() -> (ShellState, Ticket, u64, u64) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    super::super::users_fixture::install(&mut shell);
    let ticket = Ticket::capture(&shell, &key("reviewers")).unwrap();
    assert!(open_local(&mut shell, ticket.clone()));
    let p = shell.app_modal.as_ref().unwrap().users.as_ref().unwrap();
    let (serial, query_revision) = (p.serial, p.query_revision);
    (shell, ticket, serial, query_revision)
}
fn value(shell: &ShellState, block: &str) -> InputValue {
    shell
        .app_modal
        .as_ref()
        .unwrap()
        .current_form()
        .unwrap()
        .fields[&key(block)]
        .value
        .clone()
}
#[test]
fn inline_selection_is_local_full_state_and_over_limit_is_repairable() {
    let (mut shell, ticket, serial, revision) = setup();
    assert!(remove_local(&mut shell, &ticket, "U00000001"));
    assert!(
        shell
            .app_modal
            .as_ref()
            .unwrap()
            .current_form()
            .unwrap()
            .submission()
            .is_err()
    );
    for id in ["U00000001", "U00000002", "U00000003"] {
        assert!(choose_local(&mut shell, &ticket, serial, revision, id));
    }
    assert_eq!(
        value(&shell, "reviewers"),
        InputValue::Users(vec![
            "U00000001".into(),
            "U00000002".into(),
            "U00000003".into()
        ])
    );
    assert!(
        shell
            .app_modal
            .as_ref()
            .unwrap()
            .current_form()
            .unwrap()
            .submission()
            .is_err()
    );
    assert!(remove_local(&mut shell, &ticket, "U00000003"));
    let session = shell.app_modal.as_ref().unwrap();
    let wire = session.current_form().unwrap().submission().unwrap();
    assert_eq!(
        wire["values"]["reviewers"]["value"],
        json!({"type":"multi_users_select","selected_users":["U00000001","U00000002"]})
    );
    assert_eq!(
        wire["values"]["optional"]["value"]["selected_users"],
        json!([])
    );
    assert_eq!(
        wire["values"]["note"]["value"]["value"],
        "Retained sibling draft"
    );
    assert_eq!(session.fixture_attempts, 0);
    assert!(shell.core.transport.is_none());
    assert_eq!(shell.overlay, Some(Overlay::AppModal));
}
#[test]
fn stale_queries_candidates_and_hidden_or_replaced_forms_cannot_edit() {
    let changes: [fn(&mut ShellState); 7] = [
        |s| s.core.account_epoch += 1,
        |s| s.core.active_team = Some("other".into()),
        |s| s.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
        |s| s.app_modal.as_mut().unwrap().generation += 1,
        |s| s.app_modal.as_mut().unwrap().busy = true,
        |s| s.app_modal.as_mut().unwrap().uncertain = true,
        |s| s.overlay = None,
    ];
    for change in changes {
        let (mut shell, ticket, serial, revision) = setup();
        let before = value(&shell, "reviewers");
        change(&mut shell);
        assert!(!choose_local(
            &mut shell,
            &ticket,
            serial,
            revision,
            "U00000002"
        ));
        assert!(!remove_local(&mut shell, &ticket, "U00000001"));
        assert!(!complete(&mut shell, serial, revision, Ok(vec![])));
        assert_eq!(value(&shell, "reviewers"), before);
        assert!(poll(&mut shell));
    }
    let (mut shell, ticket, serial, revision) = setup();
    assert!(!choose_local(
        &mut shell,
        &ticket,
        serial,
        revision + 1,
        "U00000002"
    ));
    assert!(!choose_local(
        &mut shell,
        &ticket,
        serial,
        revision,
        "U99999999"
    ));
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .users
        .get_mut("U00000002")
        .unwrap()
        .deleted = true;
    assert!(!choose_local(
        &mut shell,
        &ticket,
        serial,
        revision,
        "U00000002"
    ));
}
#[test]
fn credential_and_transport_replacement_invalidate_same_view_search() {
    for change in 0..3 {
        let (mut shell, _, _, _) = setup();
        shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap()));
        shell.core.session = Some(Credentials {
            d_cookie: "synthetic-cookie".into(),
            workspaces: [(
                "T1".into(),
                WorkspaceSession {
                    team_id: "T1".into(),
                    enterprise_id: None,
                    user_id: "U00000001".into(),
                    name: "Fixture".into(),
                    url: "https://example.test".into(),
                    token: "synthetic-token".into(),
                },
            )]
            .into(),
        });
        let ticket = Ticket::capture(&shell, &key("reviewers")).unwrap();
        shell.app_modal.as_mut().unwrap().users = None;
        assert!(open_local(&mut shell, ticket.clone()));
        let p = shell.app_modal.as_ref().unwrap().users.as_ref().unwrap();
        let (serial, revision) = (p.serial, p.query_revision);
        match change {
            0 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .token = "rotated".into()
            }
            1 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .user_id = "U99999999".into()
            }
            _ => shell.core.transport = Some(Arc::new(Transport::new("different-cookie").unwrap())),
        }
        assert!(!ticket.current(&shell));
        assert!(!choose_local(
            &mut shell,
            &ticket,
            serial,
            revision,
            "U00000002"
        ));
        assert!(!complete(&mut shell, serial, revision, Ok(vec![])));
    }
}
#[test]
fn member_policy_keeps_self_bots_guests_and_remote_labels_but_filters_unavailable() {
    let (mut shell, ticket, serial, revision) = setup();
    let ids: Vec<_> = shell
        .app_modal
        .as_ref()
        .unwrap()
        .users
        .as_ref()
        .unwrap()
        .candidates
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    for id in ["U00000001", "U00000003", "U00000004", "U00000005"] {
        assert!(ids.contains(&id));
    }
    assert!(!ids.contains(&"U00000006"));
    assert!(!ids.contains(&"U00000007"));
    let remote =
        serde_json::from_value(json!({"id":"U00000099","name":"Remote Reviewer"})).unwrap();
    assert!(complete(&mut shell, serial, revision, Ok(vec![remote])));
    assert!(choose_local(
        &mut shell,
        &ticket,
        serial,
        revision,
        "U00000099"
    ));
    shell.app_modal.as_mut().unwrap().users = None;
    assert_eq!(
        labels(&shell, &ticket, &["U00000099".into()]),
        vec![("U00000099".into(), "Remote Reviewer".into())]
    );
}
#[test]
fn remote_revision_retires_search_preserves_dirty_users_and_pristine_state_refreshes() {
    let (mut shell, ticket, serial, revision) = setup();
    assert!(choose_local(
        &mut shell,
        &ticket,
        serial,
        revision,
        "U00000002"
    ));
    let mut updated = super::super::users_fixture::view();
    updated["blocks"][1]["element"]["initial_users"] = json!(["U00000004"]);
    updated["blocks"][1]["element"]["max_selected_items"] = json!(1);
    updated["blocks"][2]["element"]["initial_users"] = json!(["U00000004"]);
    let session = shell.app_modal.as_mut().unwrap();
    session
        .current_form_mut()
        .unwrap()
        .reconcile(&updated)
        .unwrap();
    session.generation = next_id();
    assert!(poll(&mut shell));
    assert!(!ticket.current(&shell));
    assert_eq!(
        value(&shell, "reviewers"),
        InputValue::Users(vec!["U00000001".into(), "U00000002".into()])
    );
    assert_eq!(
        value(&shell, "optional"),
        InputValue::Users(vec!["U00000004".into()])
    );
    assert!(session_form(&shell).submission().is_err());
    let fresh = Ticket::capture(&shell, &key("reviewers")).unwrap();
    assert!(remove_local(&mut shell, &fresh, "U00000002"));
    assert!(session_form(&shell).submission().is_ok());
}
fn session_form(shell: &ShellState) -> &ModalForm {
    shell.app_modal.as_ref().unwrap().current_form().unwrap()
}
#[test]
fn search_failure_cools_down_without_losing_selected_or_cached_users() {
    let (mut shell, ticket, serial, revision) = setup();
    let before = value(&shell, "reviewers");
    assert!(complete(
        &mut shell,
        serial,
        revision,
        Err(Error::RateLimited {
            retry_after_secs: Some(30)
        })
    ));
    assert!(cooling(&shell, &ticket));
    let p = shell.app_modal.as_ref().unwrap().users.as_ref().unwrap();
    assert!(!p.loading);
    assert!(p.error.is_some());
    assert!(!p.candidates.is_empty());
    assert_eq!(value(&shell, "reviewers"), before);
    assert!(!complete(&mut shell, serial, revision + 1, Ok(vec![])));
}
