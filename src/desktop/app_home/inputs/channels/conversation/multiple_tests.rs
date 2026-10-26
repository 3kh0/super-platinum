//! Original QjSI local state versus the independent committed Home form.
use super::super as channels;
use super::super::multiple::{Widget, presentation};
use super::super::*;
use crate::app_home::inputs;
use super_platinum_core::slack::models::Channel;

const A: &str = "CINITIAL1";
const B: &str = "CSELECT02";
const C: &str = "CSELECT03";
fn key() -> InputKey {
    InputKey {
        block_id: "destination".into(),
        action_id: "pick".into(),
    }
}
fn channel(id: &str) -> Channel {
    serde_json::from_value(json!({"id":id,"name":id,"is_channel":true,"is_member":true,"updated":1,"internal_team_ids":["TOTHER001"]}))
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
            .channels
            .insert(id.into(), channel(id));
    }
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
        {"type":"input","block_id":"destination","optional":optional,"label":{"type":"plain_text","text":"Channels"},"element":{"type":"multi_conversations_select","action_id":"pick","initial_conversations":initial,"max_selected_items":maximum,"response_url_enabled":true}},
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
    let InputValue::Conversations(ids) = &inputs::form(s).unwrap().fields[&key()].value else {
        panic!("multi-conversation field")
    };
    ids.clone()
}
fn shown(s: &ShellState) -> Vec<String> {
    channels::display(s, &key())
        .unwrap()
        .into_iter()
        .map(|(id, _)| id)
        .collect()
}
fn widget(s: &ShellState) -> &Widget {
    &s.app_home.channels.multiples[&(inputs::owner(s).unwrap(), key())]
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
    assert!(channels::open_local(s, t.clone()));
    let p = s.app_home.channels.picker.as_ref().unwrap();
    (t, p.serial, p.query_revision)
}
fn choose(s: &mut ShellState, id: &str) {
    let (t, serial, revision) = picker(s);
    assert!(channels::choose_local(s, &t, serial, revision, id));
}

fn dispatch_setup(initial: Option<Vec<&str>>, optional: bool) -> ShellState {
    let mut s = setup_initial(optional, 2, &[]);
    let o = inputs::owner(&s).unwrap();
    channels::dispatch::reset(&mut s, &o);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"][0]["dispatch_action"] = json!(true);
    if let Some(initial) = initial {
        raw["blocks"][0]["element"]["initial_conversations"] = json!(initial);
    } else {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_conversations");
    }
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    inputs::hydrate(&mut s);
    assert!(inputs::supported(&s));
    let c = widget(&s).conversation_dispatch.as_ref().unwrap();
    assert!(channels::dispatch::instance_current(
        &s, &c.binding, c.instance
    ));
    s
}
fn choose_event(s: &mut ShellState, id: &str) -> Option<channels::dispatch::Event> {
    let (t, serial, revision) = picker(s);
    assert!(t.mode.is_none());
    let mut event = None;
    assert!(channels::choose_impl(
        s, &t, serial, revision, id, &mut event
    ));
    event
}

#[test]
fn typed_ordered_edits_reticket_without_retiring_stable_conversation_instance() {
    let mut s = dispatch_setup(Some(vec![A]), true);
    let (t, serial, revision) = picker(&mut s);
    let (cancel, rx) = tokio::sync::watch::channel(false);
    s.app_home.channels.picker.as_mut().unwrap().cancel = Some(cancel);
    let mut event = None;
    assert!(channels::choose_impl(
        &mut s, &t, serial, revision, B, &mut event
    ));
    let first = event.unwrap();
    assert!(*rx.borrow());
    assert!(!t.current(&s));
    assert!(!channels::choose_impl(
        &mut s, &t, serial, revision, C, &mut None
    ));
    let removed = choose_event(&mut s, A).unwrap();
    let readded = choose_event(&mut s, A).unwrap();
    assert_eq!(shown(&s), vec![B, A]);
    assert_eq!(committed(&s), vec![B, A]);
    assert_eq!(first.instance, removed.instance);
    assert_eq!(first.instance, readded.instance);
    assert!(channels::dispatch::instance_current(
        &s,
        &first.binding,
        first.instance
    ));
    assert!(widget(&s).dispatch.is_none());
}

#[test]
fn typed_invalid_callbacks_commit_and_reset_retires_current_instance() {
    let mut s = dispatch_setup(Some(vec![A]), false);
    let event = choose_event(&mut s, B).unwrap();
    assert!(choose_event(&mut s, C).is_none());
    assert_eq!(committed(&s), vec![A, B, C]);
    assert!(widget(&s).invalid);
    assert!(channels::dispatch::instance_current(
        &s,
        &event.binding,
        event.instance
    ));
    for id in [B, A] {
        assert!(choose_event(&mut s, id).is_some());
    }
    assert!(choose_event(&mut s, C).is_none());
    assert!(committed(&s).is_empty());
    assert!(!widget(&s).invalid);
    let raw = inputs::form(&s).unwrap().raw_view().clone();
    channels::dispatch::reset(&mut s, &event.binding.form);
    assert!(!channels::dispatch::instance_current(
        &s,
        &event.binding,
        event.instance
    ));
    s.app_home
        .forms
        .insert(event.binding.form.clone(), HomeForm::parse(&raw).unwrap());
    channels::hydrate(&mut s);
    assert_eq!(shown(&s), vec![A]);
    assert_ne!(
        widget(&s).conversation_dispatch.as_ref().unwrap().instance,
        event.instance
    );
    assert!(!channels::dispatch::instance_current(
        &s,
        &event.binding,
        event.instance
    ));
}

#[test]
fn omitted_parent_falls_back_but_callback_empty_wins_on_fresh_visit() {
    let mut s = dispatch_setup(None, true);
    let initial_instance = widget(&s).conversation_dispatch.as_ref().unwrap().instance;
    let wire = |s: &ShellState| {
        inputs::form(s).unwrap().state().unwrap()["values"]["destination"]["pick"].clone()
    };
    assert_eq!(wire(&s), json!({"type":"multi_conversations_select"}));
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_conversations"] = json!([B])
    });
    assert_eq!(shown(&s), vec![B]);
    assert_eq!(wire(&s), json!({"type":"multi_conversations_select"}));
    let current = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = current.home_visit;
    h.owner = current;
    channels::hydrate(&mut s);
    assert_eq!(shown(&s), vec![B]);
    assert_ne!(
        widget(&s).conversation_dispatch.as_ref().unwrap().instance,
        initial_instance
    );
    choose_event(&mut s, B).unwrap();
    assert_eq!(wire(&s)["selected_conversations"], json!([]));
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_conversations"] = json!([C])
    });
    assert_eq!(shown(&s), vec![C]);
    let current = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = current.home_visit;
    h.owner = current;
    channels::hydrate(&mut s);
    assert!(shown(&s).is_empty());
    assert_eq!(wire(&s)["selected_conversations"], json!([]));
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
            state["values"]["destination"]["pick"],
            json!({
                "type":"multi_conversations_select", "selected_conversations":[A,B,C]
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
        inputs::form(&s).unwrap().state().unwrap()["values"]["destination"]["pick"],
        json!({"type":"multi_conversations_select","selected_conversations":[A,B,C]})
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
        assert!(channels::remove_local(&mut s, &ticket, id));
    }
    assert!(shown(&s).is_empty());
    assert!(committed(&s).is_empty());
    assert!(!widget(&s).invalid);
    assert!(inputs::form(&s).unwrap().fields[&key()].error.is_some());
    assert_eq!(
        inputs::form(&s).unwrap().state().unwrap()["values"]["destination"]["pick"]["selected_conversations"],
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
    assert_eq!(
        p.text.as_deref(),
        Some("You can select up to 4 conversations.")
    );
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_conversations"] = json!([B])
    });
    assert_eq!(shown(&s), vec![B]);
    assert_eq!(committed(&s), vec![A, B, C]);
    assert!(widget(&s).invalid);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_conversations"] = Value::Null
    });
    assert_eq!(shown(&s), vec![B]);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_conversations");
    });
    assert_eq!(shown(&s), vec![B]);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_conversations"] = json!([])
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
    assert_eq!(
        p.text.as_deref(),
        Some("You can select up to 1 conversations.")
    );
    assert!(s.app_home.fixture_forms.is_empty());
}

#[test]
fn initialized_parent_ignores_saved_only_props_and_seeds_fresh_mount() {
    let mut s = setup(true, 2);
    refresh(
        &mut s,
        |raw| raw["state"] = json!({"values":{"destination":{"pick":{"type":"multi_conversations_select","selected_conversations":[B,C]}}}}),
    );
    assert_eq!(shown(&s), vec![A]);
    assert_eq!(committed(&s), vec![A]);
    refresh(&mut s, |raw| {
        raw["state"]["values"]["destination"]["pick"]["selected_conversations"] = json!([])
    });
    assert_eq!(shown(&s), vec![A]);
    assert_eq!(committed(&s), vec![A]);
    let owner = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = owner.home_visit;
    h.owner = owner;
    channels::hydrate(&mut s);
    assert_eq!(shown(&s), vec![A]);
    assert!(!widget(&s).invalid);
    assert!(s.app_home.fixture_forms.is_empty());
}

#[test]
fn local_edit_retires_held_query_without_dropping_current_rows_or_query() {
    let mut s = setup(true, 2);
    let (t, serial, revision) = picker(&mut s);
    let (cancel, rx) = tokio::sync::watch::channel(false);
    let p = s.app_home.channels.picker.as_mut().unwrap();
    p.cancel = Some(cancel);
    p.loading = true;
    p.query = "held query".into();
    let before: Vec<_> = p.candidates.iter().map(|c| c.id.clone()).collect();
    assert!(channels::choose_local(&mut s, &t, serial, revision, B));
    assert!(*rx.borrow());
    assert!(!t.current(&s));
    let p = s.app_home.channels.picker.as_ref().unwrap();
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
    assert!(!channels::search::complete(
        &mut s,
        serial,
        revision,
        Ok(vec![channel(C)])
    ));
    assert!(!channels::choose_local(&mut s, &t, serial, revision, C));
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
        assert!(!channels::choose_local(&mut s, &t, serial, revision, B));
        assert!(!channels::search::complete(
            &mut s,
            serial,
            revision,
            Ok(vec![channel(B)])
        ));
        assert_eq!(
            s.app_home.forms[&t.form].fields[&key()].value,
            InputValue::Conversations(vec![A.into()])
        );
    }
}

fn remote_user() -> super_platinum_core::slack::models::User {
    serde_json::from_value(json!({"id":"UREMOTE01","name":"remote-member","profile":{"display_name":"Remote member"},"updated":1})).unwrap()
}
fn remote_mpim() -> Channel {
    serde_json::from_value(json!({"id":"GREMOTE01","name":"Remote group","is_mpim":true,"is_private":true,"is_member":true,"updated":1})).unwrap()
}

#[test]
fn mixed_remote_choices_keep_order_and_cached_labels_without_dm_or_query_authority() {
    let mut s = setup(true, 2);
    let active = s.core.active_channel.clone();
    let generation = s.channel_generation;
    let (t, serial, revision) = picker(&mut s);
    assert!(t.mode.is_none());
    assert!(t.conversation.is_some());
    assert!(search::complete_entities(
        &mut s,
        serial,
        revision,
        Ok((vec![remote_user()], vec![remote_mpim()]))
    ));
    assert!(choose_local(&mut s, &t, serial, revision, "UREMOTE01"));
    let p = s.app_home.channels.picker.as_ref().unwrap();
    let (t, n, r) = (p.ticket.clone(), p.serial, p.query_revision);
    assert!(choose_local(&mut s, &t, n, r, "GREMOTE01"));
    assert_eq!(committed(&s), vec![A, "UREMOTE01", "GREMOTE01"]);
    assert!(widget(&s).invalid);
    assert_eq!(s.core.active_channel, active);
    assert_eq!(s.channel_generation, generation);
    let expected = display(&s, &key()).unwrap();
    assert_eq!(expected[1].1, "Remote member");
    assert_eq!(expected[2].1, "Remote group");
    s.app_home.channels.picker = None;
    s.overlay = Some(Overlay::AppModal);
    assert!(Ticket::capture(&s, &key()).is_none());
    assert_eq!(display(&s, &key()).unwrap(), expected);
    s.overlay = None;
    for initial in [json!([B]), json!([])] {
        refresh(&mut s, |raw| {
            raw["blocks"][0]["element"]["initial_conversations"] = initial.clone()
        });
        assert_eq!(committed(&s), vec![A, "UREMOTE01", "GREMOTE01"]);
    }
    assert!(shown(&s).is_empty());
    let owner = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = owner.home_visit;
    h.owner = owner;
    channels::hydrate(&mut s);
    assert_eq!(display(&s, &key()).unwrap(), expected);
    let (t, n, r) = picker(&mut s);
    assert!(!choose_local(&mut s, &t, n, r, "UREMOTE01"));
    assert!(!choose_local(&mut s, &t, n, r, "GREMOTE01"));
    let team = s.core.active_team.clone().unwrap();
    let mut user = remote_user();
    user.profile.as_mut().unwrap().display_name = Some("Current member".into());
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .insert(user.id.clone(), user);
    assert_eq!(display(&s, &key()).unwrap()[1].1, "Current member");
    assert!(s.app_home.fixture_forms.is_empty());
}

#[test]
fn ignored_default_has_no_home_origin_and_filter_is_conversation_specific() {
    let mut s = setup_initial(true, 3, &[]);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_conversations");
        raw["blocks"][0]["element"]["default_to_current_conversation"] = json!(true);
    });
    let t = Ticket::capture(&s, &key()).unwrap();
    assert!(t.mode.is_none());
    assert!(shown(&s).is_empty());
    assert!(committed(&s).is_empty());
    let mut archived = channel("CARCHIVE1");
    archived.is_archived = true;
    let group = remote_mpim();
    let rows = super::candidates(&s, &t, &[], &[archived.clone(), group.clone()], "");
    assert!(!rows.iter().any(|c| c.id == archived.id));
    assert!(rows.iter().any(|c| c.id == group.id));
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["filter"] = json!({"exclude_bot_users":true})
    });
    let t = Ticket::capture(&s, &key()).unwrap();
    let rows = super::candidates(&s, &t, &[], &[archived.clone(), group.clone()], "");
    assert!(rows.iter().any(|c| c.id == archived.id));
    assert!(!rows.iter().any(|c| c.id == group.id));
}

#[test]
fn channel_only_refresh_cannot_rewrite_conversation_retained_label_or_proof() {
    let mut s = setup(true, 3);
    let (t, n, r) = picker(&mut s);
    let remote = remote_mpim();
    assert!(search::complete_entities(
        &mut s,
        n,
        r,
        Ok((vec![], vec![remote.clone()]))
    ));
    assert!(choose_local(&mut s, &t, n, r, &remote.id));
    let t = Ticket::capture(&s, &key()).unwrap();
    let before = widget(&s).retained_choices[&remote.id].clone();
    let mut renamed = remote;
    renamed.name = Some("Renamed group".into());
    renamed.updated = Some(2);
    policy::refresh(&mut s, &t, &[renamed.clone()]);
    let unchanged = &widget(&s).retained_choices[&renamed.id];
    assert_eq!(unchanged.label, before.label);
    assert_eq!(unchanged.canonical, before.canonical);
    super::refresh(&mut s, &t, &[], &[renamed.clone()]);
    let changed = &widget(&s).retained_choices[&renamed.id];
    assert_eq!(changed.label, "Renamed group");
    assert_eq!(
        changed.canonical,
        Some(serde_json::to_value(renamed).unwrap())
    );
    assert_eq!(committed(&s), vec![A, "GREMOTE01"]);
}
