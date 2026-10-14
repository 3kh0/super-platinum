use super::super::channel_fixture as fixture;
use super::*;
use serde_json::json;

fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "value".into(),
    }
}
fn setup() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut shell);
    shell
}
fn open(shell: &mut ShellState, block: &str) -> (Ticket, u64, u64) {
    let ticket = Ticket::capture(shell, &key(block)).unwrap();
    assert!(open_local(shell, ticket.clone()));
    let p = shell.app_modal.as_ref().unwrap().users.as_ref().unwrap();
    (ticket, p.serial, p.query_revision)
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
fn modal_channel_initial_and_empty_state_ignore_origin_and_clear_uses_selected_channel_null() {
    let mut shell = setup();
    assert_eq!(
        value(&shell, "reviewers"),
        InputValue::Channel(Some("C00000006".into()))
    );
    assert_eq!(value(&shell, "optional"), InputValue::Channel(None));
    let required = Ticket::capture(&shell, &key("reviewers")).unwrap();
    assert!(!remove_local(&mut shell, &required, "C00000006"));
    let (ticket, serial, revision) = open(&mut shell, "optional");
    assert!(choose_local(
        &mut shell,
        &ticket,
        serial,
        revision,
        "C00000002"
    ));
    assert!(shell.app_modal.as_ref().unwrap().users.is_none());
    assert!(remove_local(&mut shell, &ticket, "C00000002"));
    let form = shell
        .app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap();
    form.set_value(&key("note"), InputValue::Text("Local sibling draft".into()))
        .unwrap();
    assert_eq!(
        form.submission().unwrap(),
        json!({"values":{
            "note":{"value":{"type":"plain_text_input","value":"Local sibling draft"}},
            "reviewers":{"value":{"type":"channels_select","selected_channel":"C00000006","response_url_enabled":true}},
            "optional":{"value":{"type":"channels_select","selected_channel":null,"response_url_enabled":true}}
        }})
    );
    assert_eq!(shell.app_modal.as_ref().unwrap().fixture_attempts, 0);
}
#[test]
fn modal_channel_picker_only_allows_public_nonarchived_channels_and_rechecks_canonical_changes() {
    let mut shell = setup();
    let (ticket, serial, revision) = open(&mut shell, "optional");
    let p = shell.app_modal.as_ref().unwrap().users.as_ref().unwrap();
    assert_eq!(
        p.candidates
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        vec!["C00000002", "C00000006", "C00000001"]
    );
    for id in [
        "G00000003",
        "G00000004",
        "D00000008",
        "U00000002",
        "C00000005",
    ] {
        assert!(!choose_local(&mut shell, &ticket, serial, revision, id));
    }
    let c = shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .get_mut("C00000002")
        .unwrap();
    c.is_private = true;
    c.updated = Some(20);
    assert!(!choose_local(
        &mut shell,
        &ticket,
        serial,
        revision,
        "C00000002"
    ));
    assert_eq!(value(&shell, "optional"), InputValue::Channel(None));
    assert!(choose_local(
        &mut shell,
        &ticket,
        serial,
        revision,
        "C00000006"
    ));
    assert_eq!(
        value(&shell, "optional"),
        InputValue::Channel(Some("C00000006".into()))
    );
}

#[test]
fn eligible_remote_public_nonmember_can_be_selected_without_joining_or_cache_insertion() {
    let mut shell = setup();
    let (ticket, serial, revision) = open(&mut shell, "optional");
    let remote: Channel = serde_json::from_value(json!({"id":"C00000009","is_channel":true,
        "name":"Discoverable public channel","is_member":false,"updated":30}))
    .unwrap();
    assert!(complete_entities(
        &mut shell,
        serial,
        revision,
        Ok((vec![], vec![remote]))
    ));
    assert!(choose_local(
        &mut shell,
        &ticket,
        serial,
        revision,
        "C00000009"
    ));
    assert_eq!(
        value(&shell, "optional"),
        InputValue::Channel(Some("C00000009".into()))
    );
    assert!(
        !shell.core.workspaces["T1"]
            .channels
            .contains_key("C00000009")
    );
    assert_eq!(shell.app_modal.as_ref().unwrap().fixture_attempts, 0);
    assert!(shell.core.transport.is_none());
}
#[test]
fn modal_channel_updates_preserve_dirty_value_refresh_pristine_and_reject_stale_work() {
    let mut shell = setup();
    let (ticket, serial, revision) = open(&mut shell, "reviewers");
    assert!(choose_local(
        &mut shell,
        &ticket,
        serial,
        revision,
        "C00000002"
    ));
    let (old, serial, revision) = open(&mut shell, "reviewers");
    let mut update = fixture::view();
    update["blocks"][1]["element"]["initial_channel"] = json!("C00000001");
    update["blocks"][2]["element"]["initial_channel"] = json!("C00000006");
    let s = shell.app_modal.as_mut().unwrap();
    s.current_form_mut().unwrap().reconcile(&update).unwrap();
    s.generation = next_id();
    assert!(poll(&mut shell));
    assert!(!choose_local(
        &mut shell,
        &old,
        serial,
        revision,
        "C00000001"
    ));
    assert!(!complete_entities(
        &mut shell,
        serial,
        revision,
        Ok((vec![], vec![]))
    ));
    assert_eq!(
        value(&shell, "reviewers"),
        InputValue::Channel(Some("C00000002".into()))
    );
    assert_eq!(
        value(&shell, "optional"),
        InputValue::Channel(Some("C00000006".into()))
    );
    let (ticket, serial, revision) = open(&mut shell, "optional");
    shell
        .app_modal
        .as_mut()
        .unwrap()
        .users
        .as_mut()
        .unwrap()
        .query_revision += 1;
    assert!(!choose_local(
        &mut shell,
        &ticket,
        serial,
        revision,
        "C00000002"
    ));
    assert!(!complete_entities(
        &mut shell,
        serial,
        revision,
        Ok((vec![], vec![]))
    ));
    assert!(remove_local(&mut shell, &ticket, "C00000006"));
    shell
        .app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap()
        .reconcile(&update)
        .unwrap();
    assert_eq!(value(&shell, "optional"), InputValue::Channel(None));
}
#[test]
fn modal_channel_pending_and_changed_owner_block_selection_without_dispatch() {
    let changes: [fn(&mut ShellState); 5] = [
        |s| s.core.account_epoch += 1,
        |s| s.core.workspaces.get_mut("T1").unwrap().self_user_id = "other".into(),
        |s| s.app_modal.as_mut().unwrap().busy = true,
        |s| s.app_modal.as_mut().unwrap().uncertain = true,
        |s| s.overlay = None,
    ];
    for change in changes {
        let mut shell = setup();
        let (ticket, serial, revision) = open(&mut shell, "optional");
        change(&mut shell);
        assert!(!choose_local(
            &mut shell,
            &ticket,
            serial,
            revision,
            "C00000002"
        ));
        assert!(!complete_entities(
            &mut shell,
            serial,
            revision,
            Ok((vec![], vec![]))
        ));
        assert_eq!(value(&shell, "optional"), InputValue::Channel(None));
        assert_eq!(shell.app_modal.as_ref().unwrap().fixture_attempts, 0);
    }
}

fn enterprise(shell: &mut ShellState) {
    use super_platinum_core::config::{Session as Credentials, WorkspaceSession};
    shell.core.session = Some(Credentials {
        d_cookie: "synthetic-cookie".into(),
        workspaces: [(
            "T1".into(),
            WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: Some("E00000001".into()),
                user_id: "U00000001".into(),
                name: "Fixture".into(),
                url: "https://fixture.slack.com".into(),
                token: "synthetic-token".into(),
            },
        )]
        .into(),
    });
}

#[test]
fn enterprise_context_blocks_channel_and_restricted_conversation_submission_not_only_picker() {
    for conversation in [false, true] {
        let mut shell = setup();
        if conversation {
            let mut view = super::super::conversation_fixture::view();
            view["blocks"][1]["element"]["filter"] = json!({"only_from_team_id":"T00000002"});
            shell.app_modal = Some(Session::fixture_with_origin(view, Some("C00000001".into())));
        }
        assert!(super::super::requests::prepare_submission(&shell).is_ok());
        let old = Ticket::capture(&shell, &key("reviewers")).unwrap();
        assert!(open_local(&mut shell, old.clone()));
        enterprise(&mut shell);
        assert!(!old.current(&shell));
        assert!(poll(&mut shell));
        assert!(Ticket::capture(&shell, &key("reviewers")).is_none());
        assert!(!open_local(&mut shell, old));
        let before = shell
            .app_modal
            .as_ref()
            .unwrap()
            .current_form()
            .unwrap()
            .submission()
            .unwrap();
        assert!(matches!(
            super::super::requests::prepare_submission(&shell),
            Err(FormError::Unsupported(_))
        ));
        let p = super::super::presentation(&shell).unwrap();
        assert!(p.locked && p.view_error.as_deref().unwrap().contains("Enterprise Grid"));
        // Even a preserved initial value plus an unrelated edited sibling cannot prepare a request.
        shell
            .app_modal
            .as_mut()
            .unwrap()
            .current_form_mut()
            .unwrap()
            .set_value(
                &key("note"),
                InputValue::Text("Retained local draft".into()),
            )
            .unwrap();
        assert!(matches!(
            super::super::requests::prepare_submission(&shell),
            Err(FormError::Unsupported(_))
        ));
        let after = shell
            .app_modal
            .as_ref()
            .unwrap()
            .current_form()
            .unwrap()
            .submission()
            .unwrap();
        assert_eq!(before["values"]["reviewers"], after["values"]["reviewers"]);
        assert!(shell.app_modal.as_ref().unwrap().users.is_none());
        assert_eq!(shell.app_modal.as_ref().unwrap().fixture_attempts, 0);
        assert!(shell.core.transport.is_none());
        shell
            .core
            .session
            .as_mut()
            .unwrap()
            .workspaces
            .get_mut("T1")
            .unwrap()
            .enterprise_id = None;
        assert!(super::super::requests::prepare_submission(&shell).is_ok());
        shell.core.active_team = Some("other-team".into());
        assert!(matches!(
            super::super::requests::prepare_submission(&shell),
            Err(FormError::ConflictingUpdate)
        ));
    }
}
