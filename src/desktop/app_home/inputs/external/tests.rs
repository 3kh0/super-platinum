use super::*;
use serde_json::json;
fn setup() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s);
    super::super::native::seed(&mut s);
    s
}
fn selected(s: &ShellState) -> Option<ExternalOption> {
    let InputValue::External(option) =
        &super::super::form(s).unwrap().fields[&fixture::key()].value
    else {
        panic!()
    };
    option.clone()
}
fn options(s: &mut ShellState) -> QueryOwner {
    let t = Ticket::capture(s, &fixture::key()).unwrap();
    let owner = open_local(s, t).unwrap();
    let revision = owner.query(s).unwrap().revision;
    assert!(shared::complete(
        s,
        &owner,
        revision,
        fixture::response(fixture::Reply::Options)
    ));
    owner
}
#[test]
fn choice_preserves_full_raw_option_until_explicit_home_action_and_compatible_refresh() {
    let mut s = setup();
    let owner = options(&mut s);
    let revision = owner.query(&s).unwrap().revision;
    assert!(choose_local(&mut s, owner, revision, 0));
    assert_eq!(
        selected(&s).unwrap().raw(),
        &fixture::option("Release Alpha", "alpha,desktop")
    );
    assert!(s.app_home.fixture_forms.is_empty());
    assert!(validated(&s));
    assert!(fixture::refresh(&mut s));
    assert!(validated(&s));
    assert_eq!(
        super::super::form(&s).unwrap().state().unwrap()["values"]["home-choice"]["value"]["selected_option"],
        fixture::option("Release Alpha", "alpha,desktop")
    );
    let h = s.app_home.surface.as_ref().unwrap();
    let b = h
        .view
        .as_ref()
        .unwrap()
        .button_with_form(
            "entry",
            "review",
            &h.owner.ctx.session.user_id,
            super::super::form(&s).unwrap(),
        )
        .unwrap();
    assert!(b.same_form(super::super::form(&s).unwrap()));
    let t = Ticket::capture(&s, &fixture::key()).unwrap();
    assert!(clear_local(&mut s, t));
    assert!(!b.same_form(super::super::form(&s).unwrap()));
    assert!(selected(&s).is_none());
}
#[test]
fn whole_form_revision_query_revision_and_full_option_proof_reject_stale_or_forged_choices() {
    let mut s = setup();
    let owner = options(&mut s);
    let revision = owner.query(&s).unwrap().revision;
    assert!(!choose_local(&mut s, owner.clone(), revision + 1, 0));
    let h = s.app_home.surface.as_ref().unwrap();
    let visit = actions::Key {
        visit: h.id,
        revision: h.revision,
    };
    assert!(super::super::set_local(
        &mut s,
        visit,
        &InputKey {
            block_id: "home-name".into(),
            action_id: "value".into()
        },
        InputValue::Text("sibling changed".into())
    ));
    assert!(!choose_local(&mut s, owner, revision, 0));
    assert!(poll(&mut s));
    let owner = options(&mut s);
    let revision = owner.query(&s).unwrap().revision;
    let QueryOwner::Home(t) = &owner else {
        panic!()
    };
    t.query_mut(&mut s).unwrap().rows[0].option =
        ExternalOption::parse(&fixture::option("Forged label", "alpha,desktop")).unwrap();
    assert!(!choose_local(&mut s, owner, revision, 0));
    let forged = ExternalOption::parse(&fixture::option("Invented", "invented")).unwrap();
    super::super::form(&s).unwrap();
    assert!(super::super::set_local(
        &mut s,
        visit,
        &fixture::key(),
        InputValue::External(Some(forged))
    ));
    assert!(!validated(&s));
    assert!(!super::super::ready(&s));
}
#[test]
fn source_refresh_socket_account_and_unknown_or_global_unified_mode_retire_home_queries() {
    let mut s = setup();
    let owner = options(&mut s);
    let revision = owner.query(&s).unwrap().revision;
    assert!(fixture::refresh(&mut s));
    assert!(!shared::complete(
        &mut s,
        &owner,
        revision,
        fixture::response(fixture::Reply::Empty)
    ));
    let owner = options(&mut s);
    let team = s.core.active_team.clone().unwrap();
    s.core.workspaces.get_mut(&team).unwrap().rt_generation += 1;
    assert!(owner.query(&s).is_none());
    assert!(poll(&mut s));
    let mut s = setup();
    let t = Ticket::capture(&s, &fixture::key()).unwrap();
    s.core.account_epoch += 1;
    assert!(!t.current(&s));
    let mut s = setup();
    s.core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled
        .clear();
    assert!(Ticket::capture(&s, &fixture::key()).is_none());
    let mut s = setup();
    let a = s.core.session.as_mut().unwrap();
    let mut other = a.workspaces.values().next().unwrap().clone();
    other.team_id = "T_OTHER".into();
    a.workspaces.insert(other.team_id.clone(), other);
    a.unified_user_client_enabled.insert("T_OTHER".into(), true);
    assert!(Ticket::capture(&s, &fixture::key()).is_none());
}
#[test]
fn single_dispatch_admitted_without_bypassing_native_input_readiness() {
    let mut s = setup();
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["block_id"] == "home-choice")
        .unwrap()["dispatch_action"] = json!(true);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    super::super::hydrate(&mut s);
    assert!(Ticket::capture(&s, &fixture::key()).is_some());
    assert!(super::super::supported(&s));
    s.app_home.native = super::super::native::State::default();
    assert!(!super::super::ready(&s));
    assert!(Ticket::capture(&s, &fixture::key()).is_some());
}

#[test]
fn retained_home_rows_remain_selectable_while_loading_but_replacements_retire_click_identity() {
    let mut s = setup();
    let owner = options(&mut s);
    let QueryOwner::Home(ticket) = &owner else {
        panic!()
    };
    let before = ticket.query(&s).unwrap().revision;
    ticket.query_mut(&mut s).unwrap().loading = true;
    assert!(choose_local(&mut s, owner, before, 1));
    assert_eq!(selected(&s).unwrap().value(), "beta");
    let owner = options(&mut s);
    let old_revision = owner.query(&s).unwrap().revision;
    assert!(shared::complete(
        &mut s,
        &owner,
        old_revision,
        fixture::response(fixture::Reply::Replacement)
    ));
    assert_ne!(owner.query(&s).unwrap().revision, old_revision);
    assert!(!choose_local(&mut s, owner.clone(), old_revision, 0));
    let current = owner.query(&s).unwrap().revision;
    assert!(choose_local(&mut s, owner, current, 0));
    assert_eq!(selected(&s).unwrap().value(), "alpha,desktop");
}

#[test]
fn renamed_current_option_replaces_rows_without_mutating_same_value_dirty_selection() {
    let mut s = setup();
    let owner = options(&mut s);
    let rev = owner.query(&s).unwrap().revision;
    assert!(choose_local(&mut s, owner, rev, 0));
    let prior = selected(&s).unwrap();
    let owner = options(&mut s);
    let old_revision = owner.query(&s).unwrap().revision;
    assert!(shared::complete(
        &mut s,
        &owner,
        old_revision,
        fixture::response(fixture::Reply::Replacement)
    ));
    assert_eq!(selected(&s), Some(prior.clone()));
    assert!(validated(&s));
    assert!(!choose_local(&mut s, owner.clone(), old_revision, 0));
    let revision = owner.query(&s).unwrap().revision;
    let replacement = owner.query(&s).unwrap().rows[0].option.clone();
    assert_eq!(prior.value(), replacement.value());
    assert_ne!(prior.raw(), replacement.raw());
    assert!(choose_local(&mut s, owner, revision, 0));
    assert_eq!(selected(&s), Some(replacement));
    assert!(validated(&s));
}
