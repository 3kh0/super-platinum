//! Actual serial-shell edits against hGZA47; no native/HTTP claim.
use super::*;

fn dispatch_setup(optional: bool) -> ShellState {
    let mut s = setup(optional, 2);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["dispatch_action"] = json!(true)
    });
    assert!(inputs::supported(&s));
    s
}
fn callback(s: &mut ShellState, id: &str) -> Option<users::dispatch::Event> {
    let (ticket, serial, revision) = picker(s);
    let mut event = None;
    assert!(users::choose_impl(
        s, &ticket, serial, revision, id, &mut event
    ));
    event
}
fn revisit(s: &mut ShellState) {
    let owner = Owner::capture(s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = owner.home_visit;
    h.owner = owner;
    users::hydrate(s);
}

#[test]
fn ordered_edits_preserve_dispatch_instance_but_retire_picker_callback_and_query() {
    let mut s = dispatch_setup(true);
    let (old, serial, revision) = picker(&mut s);
    let (cancel, read) = tokio::sync::watch::channel(false);
    let p = s.app_home.users.picker.as_mut().unwrap();
    p.cancel = Some(cancel);
    p.loading = true;
    p.query = "reviewer".into();
    let rows: Vec<_> = p.candidates.iter().map(|c| c.id.clone()).collect();
    let instance = widget(&s).instance;
    let mut event = None;
    assert!(users::choose_impl(
        &mut s, &old, serial, revision, B, &mut event
    ));
    let event = event.unwrap();
    assert!(*read.borrow());
    assert!(!old.current(&s));
    assert!(users::dispatch::instance_current(
        &s,
        &event.binding,
        instance
    ));
    let p = s.app_home.users.picker.as_ref().unwrap();
    assert_eq!(p.serial, serial);
    assert_ne!(p.query_revision, revision);
    assert_eq!(p.query, "reviewer");
    assert!(!p.loading);
    assert_eq!(
        p.candidates
            .iter()
            .map(|c| c.id.clone())
            .collect::<Vec<_>>(),
        rows
    );
    assert!(p.ticket.current(&s));
    assert_eq!(committed(&s), vec![A, B]);
    assert!(callback(&mut s, A).is_some());
    assert!(callback(&mut s, A).is_some());
    assert_eq!(shown(&s), vec![B, A]);
    assert_eq!(committed(&s), vec![B, A]);
    assert_eq!(widget(&s).instance, instance);
    assert!(users::dispatch::instance_current(
        &s,
        &event.binding,
        instance
    ));
    assert!(!users::choose_impl(
        &mut s, &old, serial, revision, C, &mut None
    ));
}

#[test]
fn invalid_callbacks_commit_visible_parent_and_return_no_new_dispatch() {
    let mut s = dispatch_setup(false);
    let event = callback(&mut s, B).unwrap();
    assert!(callback(&mut s, C).is_none());
    assert_eq!(committed(&s), vec![A, B, C]);
    assert_eq!(shown(&s), committed(&s));
    assert!(widget(&s).invalid);
    assert!(users::dispatch::instance_current(
        &s,
        &event.binding,
        event.instance
    ));
    for (id, emits) in [(A, true), (B, true), (C, false)] {
        let t = Ticket::capture(&s, &key()).unwrap();
        let mut event = None;
        assert!(users::remove_impl(&mut s, &t, id, &mut event));
        assert_eq!(event.is_some(), emits);
    }
    assert!(shown(&s).is_empty());
    assert_eq!(
        inputs::form(&s).unwrap().state().unwrap()["values"]["member"]["pick"]["selected_users"],
        json!([])
    );
    assert!(!widget(&s).invalid);
    let mut quiet = setup(true, 2);
    assert!(callback(&mut quiet, B).is_none());
    assert_eq!(committed(&quiet), vec![A, B]);
}

#[test]
fn omitted_parent_and_explicit_empty_have_distinct_revisit_seeds() {
    let mut s = dispatch_setup(true);
    let o = inputs::owner(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"][0]["element"]
        .as_object_mut()
        .unwrap()
        .remove("initial_users");
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    users::dispatch::reset(&mut s, &o);
    s.app_home.forms.clear();
    inputs::hydrate(&mut s);
    assert!(
        inputs::form(&s).unwrap().state().unwrap()["values"]["member"]["pick"]
            .get("selected_users")
            .is_none()
    );
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_users"] = json!([C, B])
    });
    assert_eq!(shown(&s), vec![C, B]);
    assert!(committed(&s).is_empty());
    revisit(&mut s);
    assert_eq!(shown(&s), vec![C, B]);
    for id in [C, B] {
        let t = Ticket::capture(&s, &key()).unwrap();
        assert!(users::remove_impl(&mut s, &t, id, &mut None));
    }
    assert_eq!(
        inputs::form(&s).unwrap().state().unwrap()["values"]["member"]["pick"]["selected_users"],
        json!([])
    );
    revisit(&mut s);
    assert!(shown(&s).is_empty());
}

#[test]
fn initialized_parent_and_stable_instance_survive_props_but_reset_spends_instance() {
    let mut s = dispatch_setup(true);
    let instance = widget(&s).instance;
    let old = widget(&s).binding.clone();
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_users"] = json!([C]);
        raw["state"] = json!({"values":{"member":{"pick":{"type":"multi_users_select","selected_users":[B]}}}});
    });
    assert_eq!(shown(&s), vec![C]);
    assert_eq!(committed(&s), vec![A]);
    assert_eq!(widget(&s).instance, instance);
    assert!(!users::dispatch::instance_current(&s, &old, instance));
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_users"] = json!([])
    });
    assert!(shown(&s).is_empty());
    assert_eq!(committed(&s), vec![A]);
    let o = inputs::owner(&s).unwrap();
    let fresh = HomeForm::parse(inputs::form(&s).unwrap().raw_view()).unwrap();
    let binding = widget(&s).binding.clone();
    users::dispatch::reset(&mut s, &o);
    inputs::bump(&mut s, &o);
    s.app_home.forms.insert(o, fresh);
    users::hydrate(&mut s);
    assert_ne!(widget(&s).instance, instance);
    assert!(!users::dispatch::instance_current(&s, &binding, instance));
    assert_eq!(shown(&s), vec![B]);
    assert_eq!(committed(&s), vec![B]);
}
