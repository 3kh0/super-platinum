use super::*;
use serde_json::json;
fn setup(max: usize, optional: bool, initial: &[(&str, &str)]) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    let block = raw["blocks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["block_id"] == "home-choice")
        .unwrap();
    block["optional"] = json!(optional);
    block["element"] = json!({"type":"multi_external_select","action_id":"value","min_query_length":3,"max_selected_items":max,"initial_options":initial.iter().map(|(l,v)|fixture::option(l,v)).collect::<Vec<_>>()});
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    crate::app_home::inputs::hydrate(&mut s);
    crate::app_home::inputs::native::seed(&mut s);
    s
}
fn query(s: &mut ShellState) -> QueryOwner {
    let t = Ticket::capture(s, &fixture::key()).unwrap();
    let o = open_local(s, t).unwrap();
    let revision = o.query(s).unwrap().revision;
    assert!(shared::complete(
        s,
        &o,
        revision,
        fixture::response(fixture::Reply::Options)
    ));
    o
}
fn choose(s: &mut ShellState, index: usize) {
    let o = owner(s, &fixture::key()).unwrap();
    let rev = o.query(s).unwrap().revision;
    assert!(choose_local(s, o, rev, index));
}
fn values(s: &ShellState) -> Vec<String> {
    crate::app_home::inputs::form(s).unwrap().state().unwrap()["values"]["home-choice"]["value"]["selected_options"].as_array().unwrap().iter().map(|o|o["value"].as_str().unwrap().into()).collect()
}
#[test]
fn multi_visible_overmaximum_is_not_the_home_button_state_and_correction_emits() {
    let mut s = setup(2, true, &[("Installed", "installed")]);
    query(&mut s);
    choose(&mut s, 0);
    let rev = crate::app_home::inputs::revision(&s);
    let old = owner(&s, &fixture::key()).unwrap();
    choose(&mut s, 1);
    assert_eq!(visible(&s, &fixture::key()).unwrap().len(), 3);
    assert_eq!(values(&s), ["installed", "alpha,desktop"]);
    assert_eq!(crate::app_home::inputs::revision(&s), rev);
    assert!(!old.query(&s).is_some());
    assert!(super::super::validated(&s));
    assert!(snapshot(&s, &fixture::key())["invalid"].as_bool().unwrap());
    let ticket = Ticket::capture(&s, &fixture::key()).unwrap();
    assert!(remove_local(&mut s, ticket, "installed"));
    assert_eq!(values(&s), ["alpha,desktop", "beta"]);
    assert_eq!(snapshot(&s, &fixture::key())["invalid"], false);
}
#[test]
fn multi_popup_and_max_only_refresh_preserve_invalid_widget_until_real_edit() {
    let mut s = setup(1, true, &[("Installed", "installed")]);
    query(&mut s);
    choose(&mut s, 0);
    let before = snapshot(&s, &fixture::key());
    let visit = owner(&s, &fixture::key()).unwrap().id();
    close_local(&mut s, visit);
    query(&mut s);
    assert_eq!(snapshot(&s, &fixture::key()), before);
    multi_fixture::refresh(&mut s, 4, None, None).unwrap();
    let mut expected = before.clone();
    expected["presentation"] = json!({"maximum":4,"count":2,"parentError":false,"invalid":false,"warning":false,"text":"You can select up to 4 items."});
    assert_eq!(snapshot(&s, &fixture::key()), expected);
    assert_eq!(expected["invalid"], true);
    assert_eq!(values(&s), ["installed"]);
    let t = Ticket::capture(&s, &fixture::key()).unwrap();
    assert!(remove_local(&mut s, t, "installed"));
    assert_eq!(values(&s), ["alpha,desktop"]);
}
#[test]
fn multi_required_empty_and_initial_overmaximum_remain_committed() {
    let mut s = setup(1, false, &[("Installed", "installed"), ("Other", "other")]);
    assert_eq!(values(&s), ["installed", "other"]);
    assert_eq!(snapshot(&s, &fixture::key())["invalid"], false);
    assert_eq!(
        snapshot(&s, &fixture::key())["presentation"]["warning"],
        true
    );
    assert_eq!(
        snapshot(&s, &fixture::key())["presentation"]["invalid"],
        true
    );
    let t = Ticket::capture(&s, &fixture::key()).unwrap();
    assert!(remove_local(&mut s, t, "installed"));
    let t = Ticket::capture(&s, &fixture::key()).unwrap();
    assert!(remove_local(&mut s, t, "other"));
    assert!(values(&s).is_empty());
    assert_eq!(snapshot(&s, &fixture::key())["invalid"], false);
    assert!(
        crate::app_home::inputs::form(&s).unwrap().fields[&fixture::key()]
            .error
            .is_some()
    );
    assert_eq!(
        snapshot(&s, &fixture::key())["presentation"]["warning"],
        false
    );
    assert_eq!(
        snapshot(&s, &fixture::key())["presentation"]["invalid"],
        false
    );
    assert_eq!(
        snapshot(&s, &fixture::key())["presentation"]["parentError"],
        false
    );
    assert!(super::super::validated(&s));
}
#[test]
fn multi_same_value_new_rows_do_not_rewrite_old_raw_until_explicit_off_on() {
    let mut s = setup(2, true, &[]);
    query(&mut s);
    choose(&mut s, 0);
    let o = owner(&s, &fixture::key()).unwrap();
    let rev = o.query(&s).unwrap().revision;
    assert!(shared::complete(
        &mut s,
        &o,
        rev,
        fixture::response(fixture::Reply::Replacement)
    ));
    assert_eq!(
        snapshot(&s, &fixture::key())["accepted"][0]["text"]["text"],
        "Release Alpha"
    );
    choose(&mut s, 0);
    choose(&mut s, 0);
    assert_eq!(
        snapshot(&s, &fixture::key())["accepted"][0]["text"]["text"],
        "Release Gamma"
    );
    assert_eq!(
        snapshot(&s, &fixture::key())["accepted"][0]["opaque"]["nested"][1]["n"],
        8
    );
}
#[test]
fn multi_changed_initial_emits_but_saved_only_props_do_not_restore_parent() {
    let mut s = setup(2, true, &[("Installed", "installed")]);
    let before = snapshot(&s, &fixture::key());
    multi_fixture::refresh(
        &mut s,
        2,
        None,
        Some(json!([fixture::option("Server", "server")])),
    )
    .unwrap();
    assert_eq!(
        visible(&s, &fixture::key()).unwrap()[0].value(),
        "installed"
    );
    assert_eq!(values(&s), ["installed"]);
    assert_eq!(
        snapshot(&s, &fixture::key())["revision"],
        before["revision"]
    );
    assert_eq!(
        snapshot(&s, &fixture::key())["accepted"][0]["value"],
        "installed"
    );
    multi_fixture::refresh(
        &mut s,
        2,
        Some(json!([fixture::option("New", "new")])),
        None,
    )
    .unwrap();
    assert_eq!(visible(&s, &fixture::key()).unwrap()[0].value(), "new");
    assert_eq!(values(&s), ["new"]);
}
#[test]
fn multi_widget_edit_retires_loading_and_exact_old_ticket_without_losing_rows() {
    let mut s = setup(1, true, &[("Installed", "installed")]);
    let old = query(&mut s);
    let QueryOwner::Home(t) = &old else { panic!() };
    t.query_mut(&mut s).unwrap().loading = true;
    let rev = old.query(&s).unwrap().revision;
    choose(&mut s, 0);
    let o = owner(&s, &fixture::key()).unwrap();
    let q = o.query(&s).unwrap();
    assert!(!q.loading);
    assert_eq!(q.rows.len(), 2);
    assert_ne!(q.revision, rev);
    assert!(!shared::complete(
        &mut s,
        &old,
        rev,
        fixture::response(fixture::Reply::Replacement)
    ));
    assert_eq!(values(&s), ["installed"]);
}

#[test]
fn multi_dirty_values_cannot_recreate_lost_option_custody() {
    let mut s = setup(2, true, &[]);
    query(&mut s);
    choose(&mut s, 0);
    s.app_home.external.catalogs.clear();
    hydrate(&mut s);
    assert!(Ticket::capture(&s, &fixture::key()).is_none());
    assert!(!super::super::validated(&s));
    assert_eq!(values(&s), ["alpha,desktop"]);
}
