//! Original hGZA local state versus the independent committed Home form.
use super::super as users;
use super::*;
use crate::app_home::inputs;
use super_platinum_core::slack::models::User;

const A: &str = "UINITIAL1";
const B: &str = "USELECT02";
const C: &str = "USELECT03";
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
fn setup(optional: bool, maximum: usize) -> ShellState {
    setup_initial(optional, maximum, &[A])
}
fn setup_initial(optional: bool, maximum: usize, initial: &[&str]) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let team = s.core.active_team.clone().unwrap();
    for id in [A, B, C] {
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
        {"type":"input","block_id":"member","optional":optional,"label":{"type":"plain_text","text":"Reviewers"},"element":{"type":"multi_users_select","action_id":"pick","initial_users":initial,"max_selected_items":maximum}},
        {"type":"input","block_id":"notes","optional":true,"element":{"type":"plain_text_input","action_id":"edit","initial_value":"Retained draft"}},
        {"type":"actions","block_id":"entry","elements":[{"type":"button","action_id":"review","text":{"type":"plain_text","text":"Review"}}]}
    ]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    inputs::hydrate(&mut s);
    s
}
fn committed(s: &ShellState) -> Vec<String> {
    let InputValue::Users(ids) = &inputs::form(s).unwrap().fields[&key()].value else {
        panic!("multi-user field")
    };
    ids.clone()
}
fn shown(s: &ShellState) -> Vec<String> {
    users::display(s, &key())
        .unwrap()
        .into_iter()
        .map(|(id, _)| id)
        .collect()
}
fn widget(s: &ShellState) -> &Widget {
    &s.app_home.users.multiples[&(inputs::owner(s).unwrap(), key())]
}
fn refresh(s: &mut ShellState, edit: impl FnOnce(&mut Value)) {
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    edit(&mut raw);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
}
fn picker(s: &mut ShellState) -> (Ticket, u64, u64) {
    let t = Ticket::capture(s, &key()).unwrap();
    assert!(users::open_local(s, t.clone()));
    let p = s.app_home.users.picker.as_ref().unwrap();
    (t, p.serial, p.query_revision)
}
fn choose(s: &mut ShellState, id: &str) {
    let (t, serial, revision) = picker(s);
    assert!(users::choose_local(s, &t, serial, revision, id));
}

#[test]
fn initial_overmax_and_lowered_max_keep_constructor_flag_and_exact_button_array() {
    use super_platinum_core::slack::{SlackClient, client::RequestBody};
    let mut s = setup_initial(false, 2, &[A, B, C]);
    for maximum in [2, 1] {
        if maximum == 1 {
            refresh(&mut s, |raw| {
                raw["blocks"][0]["element"]["max_selected_items"] = json!(1)
            });
        }
        assert_eq!(shown(&s), vec![A, B, C]);
        assert_eq!(committed(&s), vec![A, B, C]);
        assert!(!widget(&s).invalid);
        assert!(inputs::form(&s).unwrap().fields[&key()].error.is_some());
        let presentation = presentation(&s, &key()).unwrap();
        assert_eq!(presentation.maximum, Some(maximum));
        assert!(presentation.invalid);
        let h = s.app_home.surface.as_ref().unwrap();
        let view = h.view.as_ref().unwrap();
        let button = view
            .button_with_form(
                "entry",
                "review",
                &h.owner.ctx.session.user_id,
                inputs::form(&s).unwrap(),
            )
            .unwrap();
        let request = button
            .prepare_with_context(
                &SlackClient::default(),
                &h.owner.ctx.session,
                view,
                "fixture-constructor",
                None,
            )
            .unwrap();
        let RequestBody::Form(fields) = request.body else {
            panic!("form request")
        };
        let fields: BTreeMap<_, _> = fields.into_iter().collect();
        let state: Value = serde_json::from_str(&fields["state"]).unwrap();
        assert_eq!(
            state["values"]["member"]["pick"],
            json!({
                "type":"multi_users_select", "selected_users":[A,B,C]
            })
        );
        let actions: Value = serde_json::from_str(&fields["actions"]).unwrap();
        assert_eq!(actions.as_array().unwrap().len(), 1);
        assert_eq!(actions[0]["type"], "button");
    }
    assert!(s.app_home.fixture_forms.is_empty());
}

#[test]
fn overmax_inline_choice_commits_ordered_array_and_required_removal_commits_empty() {
    let mut s = setup(false, 2);
    choose(&mut s, B);
    choose(&mut s, C);
    assert_eq!(shown(&s), vec![A, B, C]);
    assert_eq!(committed(&s), vec![A, B, C]);
    assert!(widget(&s).invalid);
    assert!(inputs::form(&s).unwrap().fields[&key()].error.is_some());
    assert_eq!(
        inputs::form(&s).unwrap().state().unwrap()["values"]["member"]["pick"],
        json!({"type":"multi_users_select","selected_users":[A,B,C]})
    );
    let h = s.app_home.surface.as_ref().unwrap();
    assert!(
        h.view
            .as_ref()
            .unwrap()
            .button_with_form(
                "entry",
                "review",
                &h.owner.ctx.session.user_id,
                inputs::form(&s).unwrap()
            )
            .is_ok()
    );
    for id in [B, A, C] {
        let ticket = Ticket::capture(&s, &key()).unwrap();
        assert!(users::remove_local(&mut s, &ticket, id));
    }
    assert!(shown(&s).is_empty());
    assert!(committed(&s).is_empty());
    assert!(!widget(&s).invalid);
    assert!(inputs::form(&s).unwrap().fields[&key()].error.is_some());
    assert_eq!(
        inputs::form(&s).unwrap().state().unwrap()["values"]["member"]["pick"]["selected_users"],
        json!([])
    );
    assert!(s.app_home.fixture_forms.is_empty());
}

#[test]
fn max_and_initial_prop_effects_do_not_emit_or_revalidate_internal_widget_state() {
    let mut s = setup(false, 2);
    choose(&mut s, B);
    choose(&mut s, C);
    let old = Ticket::capture(&s, &key()).unwrap();
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["max_selected_items"] = json!(4)
    });
    assert!(!old.current(&s));
    assert!(widget(&s).invalid);
    let p = presentation(&s, &key()).unwrap();
    assert_eq!(p.count, 3);
    assert!(!p.invalid);
    assert_eq!(p.text.as_deref(), Some("You can select up to 4 people."));
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_users"] = json!([B])
    });
    assert_eq!(shown(&s), vec![B]);
    assert_eq!(committed(&s), vec![A, B, C]);
    assert!(widget(&s).invalid);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_users"] = Value::Null
    });
    assert_eq!(shown(&s), vec![B]);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_users");
    });
    assert_eq!(shown(&s), vec![B]);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_users"] = json!([])
    });
    assert!(shown(&s).is_empty());
    assert_eq!(committed(&s), vec![A, B, C]);
    assert!(widget(&s).invalid);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["max_selected_items"] = json!(1)
    });
    let p = presentation(&s, &key()).unwrap();
    assert_eq!(p.count, 0);
    assert!(form(&s).unwrap().fields[&key()].error.is_some());
    assert!(!p.invalid);
    assert!(p.parent_error.is_none());
    assert_eq!(p.text.as_deref(), Some("You can select up to 1 people."));
    assert!(s.app_home.fixture_forms.is_empty());
}

#[test]
fn initialized_parent_survives_saved_only_refresh_and_revisit() {
    let mut s = setup(true, 2);
    refresh(
        &mut s,
        |raw| raw["state"] = json!({"values":{"member":{"pick":{"type":"multi_users_select","selected_users":[B,C]}}}}),
    );
    assert_eq!(shown(&s), vec![A]);
    assert_eq!(committed(&s), vec![A]);
    refresh(&mut s, |raw| {
        raw["state"]["values"]["member"]["pick"]["selected_users"] = json!([])
    });
    assert_eq!(shown(&s), vec![A]);
    assert_eq!(committed(&s), vec![A]);
    let owner = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = owner.home_visit;
    h.owner = owner;
    users::hydrate(&mut s);
    assert_eq!(shown(&s), vec![A]);
    assert!(!widget(&s).invalid);
    assert!(s.app_home.fixture_forms.is_empty());
}

#[test]
fn local_edit_retires_held_query_without_dropping_current_rows_or_query() {
    let mut s = setup(true, 2);
    let (t, serial, revision) = picker(&mut s);
    let (cancel, rx) = tokio::sync::watch::channel(false);
    let p = s.app_home.users.picker.as_mut().unwrap();
    p.cancel = Some(cancel);
    p.loading = true;
    p.query = "held query".into();
    let before: Vec<_> = p.candidates.iter().map(|c| c.id.clone()).collect();
    assert!(users::choose_local(&mut s, &t, serial, revision, B));
    assert!(*rx.borrow());
    assert!(!t.current(&s));
    let p = s.app_home.users.picker.as_ref().unwrap();
    assert_eq!(p.serial, serial);
    assert_ne!(p.query_revision, revision);
    assert_eq!(p.query, "held query");
    assert!(!p.loading);
    assert!(p.ticket.current(&s));
    assert_eq!(
        p.candidates
            .iter()
            .map(|c| c.id.clone())
            .collect::<Vec<_>>(),
        before
    );
    assert!(!users::search::complete(
        &mut s,
        serial,
        revision,
        Ok(vec![member(C)])
    ));
    assert!(!users::choose_local(&mut s, &t, serial, revision, C));
    assert_eq!(shown(&s), vec![A, B]);
    assert_eq!(committed(&s), vec![A, B]);
}

#[test]
fn context_or_account_retirement_refuses_old_callbacks_without_form_mutation() {
    for retire in ["context", "socket", "account"] {
        let mut s = setup(true, 2);
        let (t, serial, revision) = picker(&mut s);
        let team = s.core.active_team.clone().unwrap();
        match retire {
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
        assert!(!users::choose_local(&mut s, &t, serial, revision, B));
        assert!(!users::search::complete(
            &mut s,
            serial,
            revision,
            Ok(vec![member(B)])
        ));
        assert_eq!(
            s.app_home.forms[&t.form].fields[&key()].value,
            InputValue::Users(vec![A.into()])
        );
    }
}

#[test]
fn remote_selected_label_survives_picker_overlay_and_prefers_current_native_name() {
    let mut s = setup(true, 2);
    let (t, serial, revision) = picker(&mut s);
    let remote_id = "UREMOTE01";
    let mut remote = member(remote_id);
    remote.real_name = Some("Remote reviewer".into());
    assert!(users::search::complete(
        &mut s,
        serial,
        revision,
        Ok(vec![remote.clone()])
    ));
    let expected = s
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
    assert!(users::choose_local(&mut s, &t, serial, revision, remote_id));
    s.app_home.users.picker = None;
    s.overlay = Some(Overlay::AppModal);
    assert!(Ticket::capture(&s, &key()).is_none());
    assert_eq!(
        users::display(&s, &key()).unwrap().last(),
        Some(&(remote_id.into(), expected))
    );
    let team = s.core.active_team.clone().unwrap();
    remote.real_name = Some("Current reviewer".into());
    let w = s.core.workspaces.get_mut(&team).unwrap();
    w.users.insert(remote_id.into(), remote);
    let expected = w.display_name(remote_id);
    assert_eq!(
        users::display(&s, &key()).unwrap().last(),
        Some(&(remote_id.into(), expected))
    );
    assert_eq!(committed(&s), vec![A, remote_id]);
}

#[path = "dispatch_tests.rs"]
mod dispatch_tests;
