use super::*;
use crate::message_buttons::{external::complete, external_fixture as fixture};
use serde_json::json;
use super_platinum_core::block_actions::external::{ExternalOption, ExternalSuggestions};

fn key() -> InputKey {
    InputKey {
        block_id: "choice".into(),
        action_id: "pick".into(),
    }
}
fn option(value: &str) -> ExternalOption {
    ExternalOption::parse(&fixture::option(value, value)).unwrap()
}
fn view(optional: bool, maximum: usize) -> Value {
    let mut view = fixture::view();
    view["blocks"][0]["optional"] = json!(optional);
    view["blocks"][0]["element"] = json!({
        "type":"multi_external_select", "action_id":"pick", "min_query_length":0,
        "max_selected_items":maximum, "initial_options":[fixture::option("initial", "initial")]
    });
    view
}
fn setup(view: Value) -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.transport = None;
    shell.app_modal = Some(super::super::Session::fixture(view));
    shell.overlay = Some(Overlay::AppModal);
    shell
}
fn open(shell: &mut ShellState) -> Owner {
    let ticket = Ticket::capture(shell, &key()).unwrap();
    open_local(shell, ticket).unwrap()
}
fn selected(shell: &ShellState) -> Vec<ExternalOption> {
    Ticket::capture(shell, &key())
        .unwrap()
        .selected(shell)
        .unwrap()
}
fn result(shell: &mut ShellState, owner: &Owner, options: Vec<ExternalOption>) {
    let revision = owner.query(shell).unwrap().revision;
    assert!(complete(
        shell,
        owner,
        revision,
        Ok(ExternalSuggestions::Options(options))
    ));
}
fn choose(shell: &mut ShellState, index: usize) {
    let owner = owner(shell, &key()).unwrap();
    let revision = owner.query(shell).unwrap().revision;
    assert!(choose_local(shell, owner, revision, index));
}
fn wire(shell: &ShellState) -> Value {
    requests::prepare_submission(shell).unwrap()
}

#[test]
fn inline_raw_order_and_sibling_state_survive_queries_without_action() {
    let mut s = setup(view(true, 5));
    let first = open(&mut s);
    result(&mut s, &first, vec![option("beta"), option("alpha")]);
    choose(&mut s, 0);
    assert!(s.app_modal.as_ref().unwrap().external_picker.is_some());
    let second = owner(&s, &key()).unwrap();
    assert_eq!(
        first.id(),
        second.id(),
        "local inline edit keeps mounted visit"
    );
    assert!(
        !first.query(&s).is_some(),
        "old selected-value callback retired"
    );
    choose(&mut s, 1);
    let current = owner(&s, &key()).unwrap();
    result(&mut s, &current, vec![]);
    let state = wire(&s);
    assert_eq!(
        state["values"]["choice"]["pick"]["selected_options"],
        json!([
            option("initial").raw(),
            option("beta").raw(),
            option("alpha").raw()
        ])
    );
    assert_eq!(
        state["values"]["note"]["value"]["value"],
        "Keep this sibling"
    );
    assert_eq!(s.app_modal.as_ref().unwrap().fixture_attempts, 0);
}

#[test]
fn over_maximum_and_required_empty_remain_editable_but_cannot_submit() {
    let mut s = setup(view(false, 1));
    let first = open(&mut s);
    result(&mut s, &first, vec![option("beta")]);
    choose(&mut s, 0);
    assert_eq!(selected(&s).len(), 2);
    assert!(requests::prepare_submission(&s).is_err());
    let current = owner(&s, &key()).unwrap();
    assert!(multi::remove_local(&mut s, current, "initial"));
    assert!(requests::prepare_submission(&s).is_ok());
    let current = owner(&s, &key()).unwrap();
    assert!(multi::remove_local(&mut s, current, "beta"));
    assert!(selected(&s).is_empty());
    assert!(requests::prepare_submission(&s).is_err());
    // Accepted rows survive removal, so the required field can be corrected.
    choose(&mut s, 0);
    assert_eq!(selected(&s), vec![option("beta")]);
    assert!(requests::prepare_submission(&s).is_ok());
}

#[test]
fn optional_clear_is_explicit_array_and_retired_remove_cannot_cross_aba() {
    let mut s = setup(view(true, 3));
    let first = open(&mut s);
    result(&mut s, &first, vec![option("initial")]);
    let stale = owner(&s, &key()).unwrap();
    assert!(multi::remove_local(&mut s, stale.clone(), "initial"));
    assert_eq!(
        wire(&s)["values"]["choice"]["pick"]["selected_options"],
        json!([])
    );
    choose(&mut s, 0);
    assert!(!multi::remove_local(&mut s, stale, "initial"));
    assert_eq!(selected(&s), vec![option("initial")]);
}

#[test]
fn removal_retires_inflight_result_without_losing_retained_options() {
    let mut s = setup(view(true, 3));
    let old = open(&mut s);
    let revision = old.query(&s).unwrap().revision;
    if let Owner::Modal(ticket) = &old {
        ticket.query_mut(&mut s).unwrap().loading = true;
    }
    assert!(multi::remove_local(&mut s, old.clone(), "initial"));
    let current = owner(&s, &key()).unwrap();
    assert!(!current.query(&s).unwrap().loading);
    assert_ne!(current.query(&s).unwrap().revision, revision);
    assert!(!complete(
        &mut s,
        &old,
        revision,
        Ok(ExternalSuggestions::Options(vec![option("late")]))
    ));
    assert!(current.query(&s).unwrap().rows.is_empty());
}

#[test]
fn conflicting_metadata_blocks_selected_vector_until_explicit_removal() {
    let mut s = setup(view(true, 5));
    let first = open(&mut s);
    let conflict = ExternalOption::parse(&fixture::option("Changed label", "initial")).unwrap();
    result(&mut s, &first, vec![conflict]);
    let revision = first.query(&s).unwrap().revision;
    assert!(!choose_local(&mut s, first.clone(), revision, 0));
    assert!(requests::prepare_submission(&s).is_err());
    assert!(multi::remove_local(&mut s, first, "initial"));
    assert_eq!(
        wire(&s)["values"]["choice"]["pick"]["selected_options"],
        json!([])
    );
}

#[test]
fn pristine_initial_arrays_reset_but_null_removal_and_saved_only_retain_inline() {
    let original = view(true, 5);
    let mut s = setup(original.clone());
    let old = open(&mut s);
    let mut replacement = original.clone();
    replacement["blocks"][0]["element"]["initial_options"] =
        json!([fixture::option("next", "next")]);
    s.app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap()
        .reconcile(&replacement)
        .unwrap();
    assert!(old.query(&s).is_none());
    assert_eq!(selected(&s), vec![option("next")]);
    replacement["blocks"][0]["element"]["initial_options"] = Value::Null;
    replacement["state"] = json!({"values":{"choice":{"pick":{"type":"multi_external_select","selected_options":[]}}}});
    s.app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap()
        .reconcile(&replacement)
        .unwrap();
    assert_eq!(selected(&s), vec![option("next")]);
    replacement["blocks"][0]["element"]
        .as_object_mut()
        .unwrap()
        .remove("initial_options");
    s.app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap()
        .reconcile(&replacement)
        .unwrap();
    assert_eq!(selected(&s), vec![option("next")]);
    replacement["blocks"][0]["element"]["initial_options"] = json!([]);
    s.app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap()
        .reconcile(&replacement)
        .unwrap();
    assert!(selected(&s).is_empty());
}

#[test]
fn dirty_reconcile_retains_array_but_retires_catalog_until_reaccepted_or_removed() {
    let mut s = setup(view(true, 5));
    let first = open(&mut s);
    result(&mut s, &first, vec![option("beta")]);
    choose(&mut s, 0);
    let mut replacement = view(true, 1);
    replacement["blocks"][0]["element"]["initial_options"] = json!([]);
    s.app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap()
        .reconcile(&replacement)
        .unwrap();
    assert_eq!(selected(&s), vec![option("initial"), option("beta")]);
    assert!(requests::prepare_submission(&s).is_err());
    poll(&mut s);
    let fresh = open(&mut s);
    assert!(
        requests::prepare_submission(&s).is_err(),
        "reopen cannot bless dirty values"
    );
    result(&mut s, &fresh, vec![option("beta")]);
    let current = owner(&s, &key()).unwrap();
    assert!(multi::remove_local(&mut s, current, "initial"));
    assert_eq!(
        wire(&s)["values"]["choice"]["pick"]["selected_options"],
        json!([option("beta").raw()])
    );
}

#[test]
fn collapsed_pristine_removal_retains_exact_saved_metadata_without_search() {
    let mut v = view(true, 5);
    let retained = fixture::option("Saved label", "saved");
    v["state"] = json!({"values":{"choice":{"pick":{"type":"multi_external_select",
        "selected_options":[fixture::option("initial", "initial"), retained.clone()]}}}});
    let mut s = setup(v);
    let ticket = Ticket::capture(&s, &key()).unwrap();
    assert!(multi::remove_local(&mut s, Owner::Modal(ticket), "initial"));
    assert!(s.app_modal.as_ref().unwrap().external_picker.is_none());
    assert_eq!(
        wire(&s)["values"]["choice"]["pick"]["selected_options"],
        json!([retained])
    );
    assert_eq!(s.app_modal.as_ref().unwrap().fixture_attempts, 0);
}

#[test]
fn credential_replacement_cannot_lend_dirty_multi_proofs_to_a_new_picker() {
    use super_platinum_core::{config::Session as Credentials, slack::Transport};
    let mut s = setup(view(true, 5));
    s.core.session = Some(Credentials {
        unified_user_client_enabled: Default::default(),
        d_cookie: "synthetic".into(),
        workspaces: [("T1".into(), fixture::workspace())].into(),
    });
    s.core.transport = Some(Arc::new(Transport::new("synthetic").unwrap()));
    let first = open(&mut s);
    result(&mut s, &first, vec![option("beta")]);
    choose(&mut s, 0);
    assert!(requests::prepare_submission(&s).is_ok());
    let stale = owner(&s, &key()).unwrap();
    s.core.transport = Some(Arc::new(Transport::new("synthetic").unwrap()));
    assert!(!multi::remove_local(&mut s, stale, "initial"));
    assert!(requests::prepare_submission(&s).is_err());
    poll(&mut s);
    let fresh = open(&mut s);
    assert!(requests::prepare_submission(&s).is_err());
    result(&mut s, &fresh, vec![option("beta")]);
    assert!(
        requests::prepare_submission(&s).is_err(),
        "one refreshed option does not prove its sibling"
    );
    let current = owner(&s, &key()).unwrap();
    assert!(multi::remove_local(&mut s, current, "initial"));
    assert_eq!(
        wire(&s)["values"]["choice"]["pick"]["selected_options"],
        json!([option("beta").raw()])
    );
}

#[test]
fn uncertain_submission_keeps_choices_locked_and_never_replays_inline_edits() {
    let mut s = setup(view(true, 5));
    let first = open(&mut s);
    result(&mut s, &first, vec![option("beta")]);
    choose(&mut s, 0);
    let stale = owner(&s, &key()).unwrap();
    let revision = stale.query(&s).unwrap().revision;
    let before = selected(&s);
    s.app_modal.as_mut().unwrap().uncertain = true;
    assert!(Ticket::capture(&s, &key()).is_none());
    assert!(!choose_local(&mut s, stale.clone(), revision, 0));
    assert!(!multi::remove_local(&mut s, stale, "initial"));
    assert_eq!(
        s.app_modal.as_ref().unwrap().current_form().unwrap().fields[&key()].value,
        InputValue::ExternalOptions(before)
    );
    assert_eq!(s.app_modal.as_ref().unwrap().fixture_attempts, 0);
}
