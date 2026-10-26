//! HvtU's displayed choice is distinct from the parent committed Home state.
//! These tests use actual hydration and picker publication, not a simulated UI.
use super::*;
use super_platinum_core::slack::models::User;

const INITIAL: &str = "UINITIAL1";
const CHOSEN: &str = "USELECT02";
const REFRESHED: &str = "UNEWINIT1";
fn key() -> InputKey {
    InputKey {
        block_id: "member".into(),
        action_id: "pick".into(),
    }
}
fn member(id: &str) -> User {
    serde_json::from_value(json!({"id":id,"name":id,"real_name":id,"deleted":false,"is_bot":false}))
        .unwrap()
}
fn setup(optional: bool) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let team = s.core.active_team.clone().unwrap();
    for id in [INITIAL, CHOSEN, REFRESHED] {
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .users
            .insert(id.into(), member(id));
    }
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
        {"type":"input","block_id":"member","optional":optional,"label":{"type":"plain_text","text":"Reviewer"},"element":{"type":"users_select","action_id":"pick","initial_user":INITIAL}},
        {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Notes"},"element":{"type":"plain_text_input","action_id":"edit","initial_value":"Retained draft"}},
        {"type":"actions","block_id":"entry","elements":[{"type":"button","action_id":"review","text":{"type":"plain_text","text":"Review"}}]}
    ]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    // Fixture-only setup replaces an unrelated pristine form before the test.
    s.app_home.forms.clear();
    super::super::hydrate(&mut s);
    s
}
fn committed(s: &ShellState) -> Option<String> {
    let InputValue::User(value) = &super::super::form(s).unwrap().fields[&key()].value else {
        panic!("user field")
    };
    value.clone()
}
fn displayed(s: &ShellState) -> Vec<String> {
    selected(s, &Ticket::capture(s, &key()).unwrap())
}
fn refresh(s: &mut ShellState, edit: impl FnOnce(&mut Value)) {
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    edit(&mut raw);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    super::super::hydrate(s);
}
fn picker(s: &mut ShellState) -> (Ticket, u64, u64) {
    let t = Ticket::capture(s, &key()).unwrap();
    assert!(open_local(s, t.clone()));
    let p = s.app_home.users.picker.as_ref().unwrap();
    (t, p.serial, p.query_revision)
}
fn choose(s: &mut ShellState, id: &str) {
    let (t, serial, revision) = picker(s);
    assert!(choose_local(s, &t, serial, revision, id));
}

#[test]
fn changed_initial_is_display_only_including_removal_and_does_not_emit_parent_action() {
    let mut s = setup(true);
    choose(&mut s, CHOSEN);
    let h = s.app_home.surface.as_ref().unwrap();
    let button = h
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
    let old = Ticket::capture(&s, &key()).unwrap();
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_user"] = json!(REFRESHED)
    });
    assert!(!old.current(&s));
    assert_eq!(displayed(&s), vec![REFRESHED]);
    assert_eq!(committed(&s).as_deref(), Some(CHOSEN));
    assert!(!button.same_form(super::super::form(&s).unwrap()));
    assert!(s.app_home.fixture_forms.is_empty());
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_user");
    });
    assert!(displayed(&s).is_empty());
    assert_eq!(committed(&s).as_deref(), Some(CHOSEN));
    assert!(!button.same_form(super::super::form(&s).unwrap()));
}

#[test]
fn saved_only_refresh_preserves_initialized_parent_and_display_on_revisit() {
    let mut s = setup(true);
    refresh(
        &mut s,
        |raw| {
            raw["state"] =
                json!({"values":{"member":{"pick":{"type":"users_select","selected_user":CHOSEN}}}})
        },
    );
    assert_eq!(displayed(&s), vec![INITIAL]);
    assert_eq!(committed(&s).as_deref(), Some(INITIAL));
    refresh(&mut s, |raw| {
        raw["state"]["values"]["member"]["pick"]["selected_user"] = Value::Null
    });
    assert_eq!(displayed(&s), vec![INITIAL]);
    assert_eq!(committed(&s).as_deref(), Some(INITIAL));
    assert!(s.app_home.fixture_forms.is_empty());
    // Revisit uses the initialized parent, not unadmitted saved-only refresh.
    let owner = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = owner.home_visit;
    h.owner = owner;
    hydrate(&mut s);
    assert_eq!(displayed(&s), vec![INITIAL]);
}

#[test]
fn only_optional_current_displayed_member_can_clear_and_required_draft_is_retained() {
    let mut s = setup(true);
    let old = Ticket::capture(&s, &key()).unwrap();
    assert!(!remove_local(&mut s, &old, CHOSEN));
    assert!(remove_local(&mut s, &old, INITIAL));
    assert_eq!(committed(&s), None);
    assert!(displayed(&s).is_empty());
    assert!(!remove_local(&mut s, &old, INITIAL));
    let mut s = setup(false);
    let t = Ticket::capture(&s, &key()).unwrap();
    assert!(!remove_local(&mut s, &t, INITIAL));
    assert_eq!(committed(&s).as_deref(), Some(INITIAL));
    assert_eq!(displayed(&s), vec![INITIAL]);
}

#[test]
fn held_query_and_row_cannot_mutate_replacement_query_or_sibling_edited_form() {
    let mut s = setup(true);
    let (t, serial, old) = picker(&mut s);
    let p = s.app_home.users.picker.as_mut().unwrap();
    p.query_revision = next();
    p.query = "new".into();
    p.loading = true;
    let current = p.query_revision;
    assert!(!search::complete(
        &mut s,
        serial,
        old,
        Ok(vec![member(REFRESHED)])
    ));
    assert!(!choose_local(&mut s, &t, serial, old, CHOSEN));
    assert!(s.app_home.users.picker.as_ref().unwrap().loading);
    assert!(search::complete(
        &mut s,
        serial,
        current,
        Ok(vec![member(CHOSEN)])
    ));
    assert!(super::super::set_local(
        &mut s,
        t.surface,
        &InputKey {
            block_id: "notes".into(),
            action_id: "edit".into()
        },
        InputValue::Text("Changed independently".into())
    ));
    assert!(!choose_local(&mut s, &t, serial, current, CHOSEN));
    assert!(!search::complete(
        &mut s,
        serial,
        current,
        Ok(vec![member(REFRESHED)])
    ));
    assert!(poll(&mut s));
    assert_eq!(committed(&s).as_deref(), Some(INITIAL));
}

#[test]
fn context_socket_account_and_current_member_policy_retire_callbacks_without_losing_choice() {
    for retirement in ["context", "socket", "account"] {
        let mut s = setup(true);
        let (t, serial, revision) = picker(&mut s);
        let team = s.core.active_team.clone().unwrap();
        match retirement {
            "context" => s
                .core
                .session
                .as_mut()
                .unwrap()
                .unified_user_client_enabled
                .clear(),
            "socket" => s.core.workspaces.get_mut(&team).unwrap().rt_generation += 1,
            "account" => s.core.account_epoch += 1,
            _ => unreachable!(),
        }
        assert!(!t.current(&s));
        assert!(!search::complete(
            &mut s,
            serial,
            revision,
            Ok(vec![member(REFRESHED)])
        ));
        assert!(!choose_local(&mut s, &t, serial, revision, CHOSEN));
        assert!(poll(&mut s));
        // Retired form remains inert; do not read through a newly captured owner.
        assert_eq!(
            s.app_home.forms[&t.form].fields[&key()].value,
            InputValue::User(Some(INITIAL.into()))
        );
    }
    let mut s = setup(true);
    let (t, serial, revision) = picker(&mut s);
    let team = s.core.active_team.clone().unwrap();
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .get_mut(CHOSEN)
        .unwrap()
        .deleted = true;
    assert!(!choose_local(&mut s, &t, serial, revision, CHOSEN));
    assert_eq!(committed(&s).as_deref(), Some(INITIAL));
}

#[test]
fn remote_selected_label_survives_picker_close_but_current_native_name_wins() {
    let mut s = setup(true);
    let (ticket, serial, revision) = picker(&mut s);
    let remote_id = "UREMOTE01";
    let mut remote = member(remote_id);
    remote.name = Some("remote_reviewer".into());
    remote.real_name = Some("Remote Reviewer".into());
    assert!(search::complete(
        &mut s,
        serial,
        revision,
        Ok(vec![remote.clone()])
    ));
    let shown = s
        .app_home
        .users
        .picker
        .as_ref()
        .unwrap()
        .candidates
        .iter()
        .find(|c| c.id == remote_id)
        .unwrap()
        .label
        .clone();
    assert!(choose_local(&mut s, &ticket, serial, revision, remote_id));
    assert!(s.app_home.users.picker.is_none());
    let ticket = Ticket::capture(&s, &key()).unwrap();
    let ids = vec![remote_id.to_owned()];
    assert_eq!(labels(&s, &ticket, &ids), vec![(remote_id.into(), shown)]);
    assert_eq!(committed(&s).as_deref(), Some(remote_id));
    let team = s.core.active_team.clone().unwrap();
    remote.real_name = Some("Current native reviewer".into());
    let w = s.core.workspaces.get_mut(&team).unwrap();
    w.users.insert(remote_id.into(), remote);
    let expected = w.display_name(remote_id);
    assert_eq!(
        labels(&s, &ticket, &ids),
        vec![(remote_id.into(), expected)]
    );
}

#[test]
fn widget_cache_prunes_evicted_form_owners_without_discarding_retained_forms() {
    let mut s = setup(true);
    let current = super::super::owner(&s).unwrap();
    let mut retained = current.clone();
    retained.view = "VRETAINED".into();
    let mut evicted = current.clone();
    evicted.view = "VEVICTED".into();
    let form = s.app_home.forms[&current].clone();
    s.app_home.forms.insert(retained.clone(), form);
    let parent = super::super::form(&s).unwrap().user_parent(&key()).unwrap();
    let binding = dispatch::binding(&s, &key()).unwrap();
    for owner in [&retained, &evicted] {
        s.app_home.users.widgets.insert(
            (owner.clone(), key()),
            Widget {
                visit: next(),
                instance: next(),
                parent: parent.clone(),
                binding: binding.clone(),
                revision: next(),
                initial: Some(json!(INITIAL)),
                visible: Some(CHOSEN.into()),
                retained_label: Some((CHOSEN.into(), "Retained reviewer".into())),
            },
        );
    }
    hydrate(&mut s);
    assert!(
        s.app_home
            .users
            .widgets
            .contains_key(&(current.clone(), key()))
    );
    assert!(
        s.app_home
            .users
            .widgets
            .contains_key(&(retained.clone(), key()))
    );
    assert!(!s.app_home.users.widgets.contains_key(&(evicted, key())));
    assert_eq!(
        s.app_home.users.widgets[&(retained, key())]
            .visible
            .as_deref(),
        Some(CHOSEN)
    );
    assert_eq!(displayed(&s), vec![INITIAL]);
}

#[test]
fn display_only_empty_initial_stays_empty_under_modal_and_unavailable_context() {
    let mut s = setup(true);
    choose(&mut s, CHOSEN);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_user");
    });
    assert_eq!(display(&s, &key()), Some(vec![]));
    assert_eq!(committed(&s).as_deref(), Some(CHOSEN));
    let ticket = Ticket::capture(&s, &key()).unwrap();
    s.overlay = Some(Overlay::AppModal);
    assert!(Ticket::capture(&s, &key()).is_none());
    assert_eq!(display(&s, &key()), Some(vec![]));
    assert!(!remove_local(&mut s, &ticket, CHOSEN));
    s.overlay = None;
    s.core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled
        .clear();
    assert!(Ticket::capture(&s, &key()).is_none());
    assert_eq!(display(&s, &key()), Some(vec![]));
    assert!(!remove_local(&mut s, &ticket, CHOSEN));
    assert_eq!(committed(&s).as_deref(), Some(CHOSEN));
}

#[test]
fn display_retains_remote_label_when_interaction_lease_is_unavailable() {
    let mut s = setup(true);
    let (ticket, serial, revision) = picker(&mut s);
    let remote_id = "UREMOTE01";
    let mut remote = member(remote_id);
    remote.real_name = Some("Remote Reviewer".into());
    assert!(search::complete(&mut s, serial, revision, Ok(vec![remote])));
    let label = s
        .app_home
        .users
        .picker
        .as_ref()
        .unwrap()
        .candidates
        .iter()
        .find(|c| c.id == remote_id)
        .unwrap()
        .label
        .clone();
    assert!(choose_local(&mut s, &ticket, serial, revision, remote_id));
    let expected = Some(vec![(remote_id.to_owned(), label)]);
    s.overlay = Some(Overlay::AppModal);
    assert_eq!(display(&s, &key()), expected);
    assert!(Ticket::capture(&s, &key()).is_none());
    s.overlay = None;
    s.core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled
        .clear();
    assert_eq!(display(&s, &key()), expected);
    assert!(Ticket::capture(&s, &key()).is_none());
    assert_eq!(committed(&s).as_deref(), Some(remote_id));
}

#[test]
fn display_refuses_foreign_account_socket_visit_and_unreconciled_raw_view() {
    for retirement in ["account", "socket", "visit", "raw-view"] {
        let mut s = setup(true);
        assert!(display(&s, &key()).is_some());
        match retirement {
            "account" => s.core.account_epoch += 1,
            "socket" => {
                let team = s.core.active_team.clone().unwrap();
                s.core.workspaces.get_mut(&team).unwrap().rt_generation += 1;
            }
            "visit" => {
                // New valid Home owner, but the old widget has not mounted there.
                let owner = Owner::capture(&s).unwrap();
                let h = s.app_home.surface.as_mut().unwrap();
                h.id = owner.home_visit;
                h.owner = owner;
            }
            "raw-view" => {
                let h = s.app_home.surface.as_mut().unwrap();
                let mut raw = h.view.as_ref().unwrap().raw().clone();
                raw["blocks"][0]["element"]["initial_user"] = json!(REFRESHED);
                h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
            }
            _ => unreachable!(),
        }
        assert!(display(&s, &key()).is_none(), "{retirement}");
    }
}
