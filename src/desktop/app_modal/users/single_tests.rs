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
    super::super::users_fixture::install_single(&mut shell);
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
fn single_selection_replaces_collapses_and_rejects_old_search_results() {
    let mut shell = setup();
    let (ticket, serial, revision) = open(&mut shell, "reviewers");
    assert!(!choose_local(
        &mut shell,
        &ticket,
        serial,
        revision + 1,
        "U00000002"
    ));
    assert!(choose_local(
        &mut shell,
        &ticket,
        serial,
        revision,
        "U00000002"
    ));
    assert_eq!(
        value(&shell, "reviewers"),
        InputValue::User(Some("U00000002".into()))
    );
    assert!(shell.app_modal.as_ref().unwrap().users.is_none());
    assert!(!choose_local(
        &mut shell,
        &ticket,
        serial,
        revision,
        "U00000001"
    ));
    assert!(!complete(&mut shell, serial, revision, Ok(vec![])));
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
        InputValue::User(Some("U00000002".into()))
    );
    assert_eq!(shell.app_modal.as_ref().unwrap().fixture_attempts, 0);
}

#[test]
fn only_optional_single_can_clear_and_full_state_uses_explicit_null() {
    let mut shell = setup();
    let required = Ticket::capture(&shell, &key("reviewers")).unwrap();
    assert!(!remove_local(&mut shell, &required, "U00000001"));
    assert_eq!(value(&shell, "optional"), InputValue::User(None));
    let (ticket, serial, revision) = open(&mut shell, "optional");
    assert!(choose_local(
        &mut shell,
        &ticket,
        serial,
        revision,
        "U00000001"
    ));
    assert!(remove_local(&mut shell, &ticket, "U00000001"));
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
            "reviewers":{"value":{"type":"users_select","selected_user":"U00000001"}},
            "optional":{"value":{"type":"users_select","selected_user":null}},
            "note":{"value":{"type":"plain_text_input","value":"Local sibling draft"}}
        }})
    );
    assert!(shell.core.transport.is_none());
}

#[test]
fn remote_single_update_preserves_dirty_refreshes_pristine_and_retires_old_tickets() {
    let mut shell = setup();
    let (ticket, serial, revision) = open(&mut shell, "reviewers");
    assert!(choose_local(
        &mut shell,
        &ticket,
        serial,
        revision,
        "U00000002"
    ));
    let (old, serial, revision) = open(&mut shell, "reviewers");
    let mut update = super::super::users_fixture::single_view();
    update["blocks"][1]["element"]["initial_user"] = json!("U00000004");
    update["blocks"][2]["element"]["initial_user"] = json!("U00000004");
    let s = shell.app_modal.as_mut().unwrap();
    s.current_form_mut().unwrap().reconcile(&update).unwrap();
    s.generation = next_id();
    assert!(poll(&mut shell));
    assert!(!choose_local(
        &mut shell,
        &old,
        serial,
        revision,
        "U00000001"
    ));
    assert_eq!(
        value(&shell, "reviewers"),
        InputValue::User(Some("U00000002".into()))
    );
    assert_eq!(
        value(&shell, "optional"),
        InputValue::User(Some("U00000004".into()))
    );
    let optional = Ticket::capture(&shell, &key("optional")).unwrap();
    assert!(remove_local(&mut shell, &optional, "U00000004"));
    shell
        .app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap()
        .reconcile(&update)
        .unwrap();
    assert_eq!(value(&shell, "optional"), InputValue::User(None));
}

#[test]
fn single_picker_guards_owner_view_pending_and_member_policy() {
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
            "U00000002"
        ));
        assert!(!complete(&mut shell, serial, revision, Ok(vec![])));
        assert_eq!(value(&shell, "optional"), InputValue::User(None));
    }
    let mut shell = setup();
    let (ticket, serial, revision) = open(&mut shell, "reviewers");
    for id in ["U00000006", "U00000007", "U99999999"] {
        assert!(!choose_local(&mut shell, &ticket, serial, revision, id));
    }
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
