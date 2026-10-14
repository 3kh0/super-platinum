use super::super::conversation_fixture as fixture;
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
    let picker = shell.app_modal.as_ref().unwrap().users.as_ref().unwrap();
    (ticket, picker.serial, picker.query_revision)
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
fn modal_origin_and_saved_clear_survive_navigation_and_full_state() {
    let mut shell = setup();
    shell.core.active_channel = Some("C00000002".into());
    assert_eq!(
        value(&shell, "reviewers"),
        InputValue::Conversation(Some(fixture::ORIGIN.into()))
    );
    assert_eq!(value(&shell, "optional"), InputValue::Conversation(None));
    let required = Ticket::capture(&shell, &key("reviewers")).unwrap();
    assert!(!remove_local(&mut shell, &required, fixture::ORIGIN));
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
            "reviewers":{"value":{"type":"conversations_select","selected_conversation":fixture::ORIGIN,"response_url_enabled":true}},
            "optional":{"value":{"type":"conversations_select","selected_conversation":null,"response_url_enabled":true}}
        }})
    );
    assert_eq!(shell.app_modal.as_ref().unwrap().fixture_attempts, 0);
    assert!(shell.core.transport.is_none());
}

#[test]
fn modal_conversation_filters_and_fresh_canonical_metadata_guard_choices() {
    let mut shell = setup();
    let (ticket, serial, revision) = open(&mut shell, "optional");
    let choices = &shell
        .app_modal
        .as_ref()
        .unwrap()
        .users
        .as_ref()
        .unwrap()
        .candidates;
    assert_eq!(
        choices.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["C00000002", "C00000001"]
    );
    for id in [
        "G00000003",
        "G00000004",
        "U00000002",
        "C00000005",
        "C00000006",
    ] {
        assert!(!choose_local(&mut shell, &ticket, serial, revision, id));
    }
    let channel = shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .get_mut("C00000002")
        .unwrap();
    channel.is_archived = true;
    channel.updated = Some(20);
    assert!(!choose_local(
        &mut shell,
        &ticket,
        serial,
        revision,
        "C00000002"
    ));
    assert_eq!(value(&shell, "optional"), InputValue::Conversation(None));
    let (ticket, serial, revision) = open(&mut shell, "reviewers");
    assert!(choose_local(
        &mut shell,
        &ticket,
        serial,
        revision,
        "U00000002"
    ));
    assert_eq!(
        value(&shell, "reviewers"),
        InputValue::Conversation(Some("U00000002".into()))
    );
    let (ticket, serial, revision) = open(&mut shell, "reviewers");
    assert!(choose_local(
        &mut shell,
        &ticket,
        serial,
        revision,
        "G00000004"
    ));
    assert!(
        labels(&shell, &ticket, &["G00000004".into()])[0]
            .1
            .contains("Blair")
    );
}

#[test]
fn modal_conversation_remote_updates_keep_dirty_clear_and_expire_tickets() {
    let mut shell = setup();
    let (ticket, serial, revision) = open(&mut shell, "reviewers");
    assert!(choose_local(
        &mut shell,
        &ticket,
        serial,
        revision,
        "G00000003"
    ));
    let (old, serial, revision) = open(&mut shell, "reviewers");
    let mut update = fixture::view();
    update.as_object_mut().unwrap().remove("state");
    update["blocks"][1]["element"]["initial_conversation"] = json!("C00000002");
    let s = shell.app_modal.as_mut().unwrap();
    s.current_form_mut().unwrap().reconcile(&update).unwrap();
    s.generation = next_id();
    assert!(poll(&mut shell));
    assert!(!choose_local(
        &mut shell,
        &old,
        serial,
        revision,
        "U00000002"
    ));
    assert!(!complete_entities(
        &mut shell,
        serial,
        revision,
        Ok((vec![], vec![]))
    ));
    assert_eq!(
        value(&shell, "reviewers"),
        InputValue::Conversation(Some("G00000003".into()))
    );
    assert_eq!(
        value(&shell, "optional"),
        InputValue::Conversation(Some("C00000002".into()))
    );
    let optional = Ticket::capture(&shell, &key("optional")).unwrap();
    assert!(remove_local(&mut shell, &optional, "C00000002"));
    shell
        .app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap()
        .reconcile(&update)
        .unwrap();
    assert_eq!(value(&shell, "optional"), InputValue::Conversation(None));
}

#[test]
fn modal_conversation_query_owner_and_pending_guards_reject_stale_work() {
    let changes: [fn(&mut ShellState); 8] = [
        |s| s.core.account_epoch += 1,
        |s| s.core.active_team = Some("other".into()),
        |s| s.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
        |s| s.core.workspaces.get_mut("T1").unwrap().self_user_id = "other".into(),
        |s| s.app_modal.as_mut().unwrap().generation += 1,
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
        assert_eq!(value(&shell, "optional"), InputValue::Conversation(None));
    }
    let mut shell = setup();
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
}

#[test]
fn modal_conversation_search_combines_remote_channels_without_overriding_newer_rejection() {
    let mut shell = setup();
    let (_, serial, revision) = open(&mut shell, "optional");
    let w = shell.core.workspaces.get_mut("T1").unwrap();
    let channel = w.channels.get_mut("C00000002").unwrap();
    channel.is_archived = true;
    channel.updated = Some(20);
    let remote: Channel = serde_json::from_value(
        json!({"id":"C00000002","is_channel":true,"name":"Old name","updated":10}),
    )
    .unwrap();
    let fresh: Channel = serde_json::from_value(
        json!({"id":"C00000009","is_channel":true,"name":"New result","updated":30}),
    )
    .unwrap();
    assert!(complete_entities(
        &mut shell,
        serial,
        revision,
        Ok((vec![], vec![remote, fresh]))
    ));
    let choices = &shell
        .app_modal
        .as_ref()
        .unwrap()
        .users
        .as_ref()
        .unwrap()
        .candidates;
    assert!(!choices.iter().any(|c| c.id == "C00000002"));
    assert!(choices.iter().any(|c| c.id == "C00000009"));
}
