//! Serial-shell regressions against retained HvtU42; no browser or HTTP claim.
use super::*;
use super_platinum_core::slack::models::User;
const A: &str = "UINITIAL1";
const B: &str = "USELECT02";
const C: &str = "UNEWINIT1";
fn key() -> InputKey {
    InputKey {
        block_id: "member".into(),
        action_id: "pick".into(),
    }
}
fn setup(dispatch: bool, initial: bool) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let team = s.core.active_team.clone().unwrap();
    for id in [A, B, C] {
        let user: User =
            serde_json::from_value(json!({"id":id,"name":id,"deleted":false,"is_bot":false}))
                .unwrap();
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .users
            .insert(id.into(), user);
    }
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
        {"type":"input","block_id":"member","dispatch_action":dispatch,"optional":true,"label":{"type":"plain_text","text":"Member"},"element":{"type":"users_select","action_id":"pick"}},
        {"type":"input","block_id":"many","optional":true,"label":{"type":"plain_text","text":"Members"},"element":{"type":"multi_users_select","action_id":"pick","initial_users":[A]}},
        {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Notes"},"element":{"type":"plain_text_input","action_id":"edit","initial_value":"Before"}}
    ]);
    if initial {
        raw["blocks"][0]["element"]["initial_user"] = json!(A);
    }
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    super::super::super::hydrate(&mut s);
    assert!(super::super::super::supported(&s));
    s
}
fn choose(s: &mut ShellState, id: &str) -> Option<Event> {
    let ticket = Ticket::capture(s, &key()).unwrap();
    assert!(open_local(s, ticket.clone()));
    let p = s.app_home.users.picker.as_ref().unwrap();
    let (serial, revision) = (p.serial, p.query_revision);
    let mut event = None;
    assert!(choose_impl(s, &ticket, serial, revision, id, &mut event));
    event
}
fn widget(s: &ShellState) -> &Widget {
    &s.app_home.users.widgets[&(super::super::super::owner(s).unwrap(), key())]
}
fn state(s: &ShellState) -> Value {
    super::super::super::form(s).unwrap().state().unwrap()["values"]["member"]["pick"].clone()
}
fn refresh(s: &mut ShellState, change: impl FnOnce(&mut Value)) {
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    change(&mut raw);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    super::super::super::hydrate(s);
    assert!(super::super::super::supported(s));
}
fn revisit(s: &mut ShellState) {
    let o = Owner::capture(s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = o.home_visit;
    h.owner = o;
    hydrate(s);
}
#[test]
fn callback_instance_survives_equal_choices_and_sibling_edits_but_ticket_does_not() {
    let mut s = setup(true, true);
    let before = Ticket::capture(&s, &key()).unwrap();
    let instance = widget(&s).instance;
    let first = choose(&mut s, B).unwrap();
    let revision = widget(&s).revision;
    assert!(!before.current(&s));
    assert!(instance_current(&s, &first.binding, instance));
    assert!(choose(&mut s, B).is_some());
    assert_ne!(widget(&s).revision, revision);
    assert_eq!(widget(&s).instance, instance);
    let o = super::super::super::owner(&s).unwrap();
    s.app_home
        .forms
        .get_mut(&o)
        .unwrap()
        .set(
            &InputKey {
                block_id: "notes".into(),
                action_id: "edit".into(),
            },
            InputValue::Text("After".into()),
        )
        .unwrap();
    super::super::super::bump(&mut s, &o);
    assert!(instance_current(&s, &first.binding, instance));
}
#[test]
fn initialized_parent_survives_initial_and_saved_props_while_display_changes() {
    let mut s = setup(true, true);
    let old = binding(&s, &key()).unwrap();
    let instance = widget(&s).instance;
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_user"] = json!(C);
        raw["state"] =
            json!({"values":{"member":{"pick":{"type":"users_select","selected_user":B}}}});
    });
    assert_eq!(widget(&s).visible.as_deref(), Some(C));
    assert_eq!(state(&s)["selected_user"], A);
    assert_eq!(widget(&s).instance, instance);
    assert!(!instance_current(&s, &old, instance));
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_user");
    });
    assert_eq!(widget(&s).visible, None);
    assert_eq!(state(&s)["selected_user"], A);
}
#[test]
fn fresh_seed_distinguishes_omitted_parent_from_explicit_null() {
    let mut s = setup(true, false);
    assert!(state(&s).get("selected_user").is_none());
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_user"] = json!(C)
    });
    assert!(state(&s).get("selected_user").is_none());
    revisit(&mut s);
    assert_eq!(widget(&s).visible.as_deref(), Some(C));
    let ticket = Ticket::capture(&s, &key()).unwrap();
    let mut event = None;
    assert!(remove_impl(&mut s, &ticket, C, &mut event));
    assert!(event.is_some());
    assert!(state(&s)["selected_user"].is_null());
    revisit(&mut s);
    assert_eq!(widget(&s).visible, None);
    assert!(state(&s)["selected_user"].is_null());
}
#[test]
fn owner_wide_reset_reseeds_single_and_multi_and_spends_old_picker_instance() {
    let mut s = setup(true, true);
    let old = choose(&mut s, B).unwrap();
    let many = InputKey {
        block_id: "many".into(),
        action_id: "pick".into(),
    };
    let ticket = Ticket::capture(&s, &many).unwrap();
    assert!(open_local(&mut s, ticket.clone()));
    let p = s.app_home.users.picker.as_ref().unwrap();
    let (serial, rev) = (p.serial, p.query_revision);
    assert!(choose_impl(&mut s, &ticket, serial, rev, B, &mut None));
    let o = super::super::super::owner(&s).unwrap();
    let raw = super::super::super::form(&s).unwrap().raw_view().clone();
    let fresh = HomeForm::parse(&raw).unwrap();
    reset(&mut s, &o);
    assert!(s.app_home.users.picker.is_none());
    assert!(s.app_home.users.multiples.is_empty());
    super::super::super::static_select::reset(&mut s, &o);
    s.app_home.forms.insert(o.clone(), fresh);
    super::super::super::bump(&mut s, &o);
    hydrate(&mut s);
    assert_eq!(widget(&s).visible.as_deref(), Some(A));
    assert!(!instance_current(&s, &old.binding, old.instance));
    assert_eq!(s.app_home.users.multiples[&(o, many)].visible, vec![A]);
    assert!(!ticket.current(&s));
}
#[test]
fn quiet_callbacks_emit_nothing_and_required_remove_remains_unreachable() {
    let mut s = setup(false, true);
    assert!(choose(&mut s, B).is_none());
    assert_eq!(state(&s)["selected_user"], B);
    refresh(&mut s, |raw| raw["blocks"][0]["optional"] = json!(false));
    let ticket = Ticket::capture(&s, &key()).unwrap();
    let mut event = None;
    assert!(!remove_impl(&mut s, &ticket, B, &mut event));
    assert!(event.is_none());
    assert_eq!(state(&s)["selected_user"], B);
}
#[test]
fn foreign_socket_and_account_retire_callback_and_instance_custody() {
    for mode in 0..2 {
        let mut s = setup(true, true);
        let old = choose(&mut s, B).unwrap();
        let ticket = Ticket::capture(&s, &key()).unwrap();
        if mode == 0 {
            s.core.account_epoch += 1;
        } else {
            let team = s.core.active_team.clone().unwrap();
            s.core.workspaces.get_mut(&team).unwrap().rt_generation += 1;
        }
        assert!(!instance_current(&s, &old.binding, old.instance));
        assert!(!ticket.current(&s));
    }
}
