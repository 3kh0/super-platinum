use super::*;
use crate::message_buttons::{external::complete, external_fixture as fixture};
use serde_json::json;
use super_platinum_core::block_actions::external::{ExternalOption, ExternalSuggestions};
fn setup() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.transport = None;
    install_fixture(&mut shell);
    shell
}
fn key() -> InputKey {
    InputKey {
        block_id: "choice".into(),
        action_id: "pick".into(),
    }
}
fn option(value: &str) -> ExternalOption {
    ExternalOption::parse(&fixture::option(value, value)).unwrap()
}
fn open(shell: &mut ShellState) -> Owner {
    let ticket = Ticket::capture(shell, &key()).unwrap();
    open_local(shell, ticket).unwrap()
}
fn result(shell: &mut ShellState, owner: &Owner, value: &str) {
    let revision = owner.query(shell).unwrap().revision;
    assert!(complete(
        shell,
        owner,
        revision,
        Ok(ExternalSuggestions::Options(vec![option(value)]))
    ));
}
fn choose(shell: &mut ShellState, owner: Owner) {
    let revision = owner.query(shell).unwrap().revision;
    assert!(choose_local(shell, owner, revision, 0));
}
#[test]
fn selection_survives_new_empty_query_and_clear_is_exact_local_value_owned() {
    let mut s = setup();
    let owner = open(&mut s);
    result(&mut s, &owner, "alpha");
    choose(&mut s, owner);
    let stale_clear = Ticket::capture(&s, &key()).unwrap();
    let owner = open(&mut s);
    let revision = owner.query(&s).unwrap().revision;
    assert!(complete(
        &mut s,
        &owner,
        revision,
        Ok(ExternalSuggestions::Options(vec![]))
    ));
    assert_eq!(
        requests::prepare_submission(&s).unwrap()["values"]["choice"]["pick"]["selected_option"],
        *option("alpha").raw()
    );
    result(&mut s, &owner, "beta");
    choose(&mut s, owner);
    assert!(!clear_local(&mut s, stale_clear));
    let current = Ticket::capture(&s, &key()).unwrap();
    assert!(clear_local(&mut s, current));
    let wire = requests::prepare_submission(&s).unwrap();
    assert!(wire["values"]["choice"]["pick"]["selected_option"].is_null());
    assert_eq!(
        wire["values"]["note"]["value"]["value"],
        "Keep this sibling"
    );
    assert_eq!(s.app_modal.as_ref().unwrap().fixture_attempts, 0);
}
#[test]
fn stale_query_reopen_and_same_route_field_replacement_never_lend_proofs() {
    let mut s = setup();
    let old = open(&mut s);
    let old_revision = old.query(&s).unwrap().revision;
    s.app_modal.as_mut().unwrap().external_picker = None;
    let fresh = open(&mut s);
    assert_ne!(fresh.id(), old.id());
    assert!(!complete(
        &mut s,
        &old,
        old_revision,
        Ok(ExternalSuggestions::Options(vec![option("stale")]))
    ));
    assert!(fresh.query(&s).unwrap().rows.is_empty());
    let mut replacement = fixture::view();
    replacement["blocks"][0]["element"]["initial_option"] = fixture::option("New initial", "new");
    let session = s.app_modal.as_mut().unwrap();
    session
        .current_form_mut()
        .unwrap()
        .reconcile(&replacement)
        .unwrap();
    // Same raw view routing remains; the field's replacement revision still retires the ticket.
    assert!(!match &fresh {
        Owner::Modal(t) => t.current(&s),
        _ => false,
    });
    assert_eq!(
        requests::prepare_submission(&s).unwrap()["values"]["choice"]["pick"]["selected_option"]["value"],
        "new"
    );
    let corrected = open(&mut s);
    result(&mut s, &corrected, "replacement");
    choose(&mut s, corrected);
    assert_eq!(
        requests::prepare_submission(&s).unwrap()["values"]["choice"]["pick"]["selected_option"]["value"],
        "replacement"
    );
}
#[test]
fn dirty_old_credential_proof_blocks_submit_but_required_field_can_correct() {
    use super_platinum_core::{config::Session, slack::Transport};
    let mut s = setup();
    let mut view = fixture::view();
    view["blocks"][0]["optional"] = json!(false);
    s.app_modal = Some(super::super::Session::fixture(view));
    s.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        workspaces: [("T1".into(), fixture::workspace())].into(),
    });
    s.core.transport = Some(Arc::new(Transport::new("synthetic").unwrap()));
    let old = open(&mut s);
    result(&mut s, &old, "alpha");
    choose(&mut s, old);
    assert!(requests::prepare_submission(&s).is_ok());
    s.core.transport = Some(Arc::new(Transport::new("synthetic").unwrap()));
    assert!(requests::prepare_submission(&s).is_err());
    let fresh = open(&mut s);
    assert!(requests::prepare_submission(&s).is_err());
    assert!(fresh.query(&s).unwrap().rows.is_empty());
    result(&mut s, &fresh, "beta");
    choose(&mut s, fresh);
    assert_eq!(
        requests::prepare_submission(&s).unwrap()["values"]["choice"]["pick"]["selected_option"]["value"],
        "beta"
    );
}
#[test]
fn initial_conflicting_raw_metadata_cannot_authorize_submission() {
    let mut s = setup();
    let owner = open(&mut s);
    let conflicting =
        ExternalOption::parse(&fixture::option("Changed metadata", "installed")).unwrap();
    let revision = owner.query(&s).unwrap().revision;
    assert!(complete(
        &mut s,
        &owner,
        revision,
        Ok(ExternalSuggestions::Options(vec![conflicting]))
    ));
    assert!(!choose_local(&mut s, owner, revision, 0));
    assert!(requests::prepare_submission(&s).is_err());
}
