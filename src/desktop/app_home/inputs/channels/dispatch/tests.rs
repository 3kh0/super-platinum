//! cdYm/iGIl60 serial-shell regressions. No browser, timer or HTTP execution claim.
use super::*;
use crate::app_home::inputs;
use super_platinum_core::slack::models::Channel;
const A: &str = "CINITIAL1";
const B: &str = "CSELECT02";
const C: &str = "CNEWINIT1";
fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "pick".into(),
    }
}
fn setup(dispatch: bool, initial: bool) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let team = s.core.active_team.clone().unwrap();
    s.core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .enterprise_id = Some("EHOME0001".into());
    for id in [A, B, C] {
        let c: Channel = serde_json::from_value(json!({"id":id,"name":id,"is_channel":true,"is_member":true,"internal_team_ids":[team]})).unwrap();
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .channels
            .insert(id.into(), c);
    }
    let owner = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = owner.home_visit;
    h.owner = owner;
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
        {"type":"input","block_id":"destination","dispatch_action":dispatch,"optional":true,"label":{"type":"plain_text","text":"Destination"},"element":{"type":"channels_select","action_id":"pick","response_url_enabled":true}},
        {"type":"input","block_id":"many","optional":true,"label":{"type":"plain_text","text":"Channels"},"element":{"type":"multi_channels_select","action_id":"pick","initial_channels":[A]}},
        {"type":"input","block_id":"conversation","optional":true,"label":{"type":"plain_text","text":"Conversation"},"element":{"type":"conversations_select","action_id":"pick","initial_conversation":A}},
        {"type":"input","block_id":"conversations","optional":true,"label":{"type":"plain_text","text":"Conversations"},"element":{"type":"multi_conversations_select","action_id":"pick","initial_conversations":[A]}},
        {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Notes"},"element":{"type":"plain_text_input","action_id":"edit","initial_value":"Before"}}
    ]);
    if initial {
        raw["blocks"][0]["element"]["initial_channel"] = json!(A);
    }
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    inputs::hydrate(&mut s);
    assert!(inputs::supported(&s));
    assert!(widget(&s).dispatch.is_some());
    s
}
fn choose(s: &mut ShellState, block: &str, id: &str) -> Option<Event> {
    let ticket = Ticket::capture(s, &key(block)).unwrap();
    assert!(open_local(s, ticket.clone()));
    let p = s.app_home.channels.picker.as_ref().unwrap();
    let (serial, revision) = (p.serial, p.query_revision);
    let mut event = None;
    assert!(choose_impl(s, &ticket, serial, revision, id, &mut event));
    event
}
fn widget(s: &ShellState) -> &Widget {
    &s.app_home.channels.widgets[&(inputs::owner(s).unwrap(), key("destination"))]
}
fn state(s: &ShellState) -> Value {
    inputs::form(s).unwrap().state().unwrap()["values"]["destination"]["pick"].clone()
}
fn refresh(s: &mut ShellState, edit: impl FnOnce(&mut Value)) {
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    edit(&mut raw);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
    assert!(inputs::supported(s));
}
fn revisit(s: &mut ShellState) {
    let owner = Owner::capture(s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = owner.home_visit;
    h.owner = owner;
    hydrate(s);
}

#[test]
fn equal_choices_keep_instance_but_retire_picker_ticket_and_emit_again() {
    let mut s = setup(true, true);
    let old = Ticket::capture(&s, &key("destination")).unwrap();
    let first = choose(&mut s, "destination", B).unwrap();
    let revision = widget(&s).revision;
    assert!(!old.current(&s));
    let second = choose(&mut s, "destination", B).unwrap();
    assert_eq!(first.instance, second.instance);
    assert_ne!(revision, widget(&s).revision);
    assert!(instance_current(&s, &first.binding, first.instance));
    let owner = inputs::owner(&s).unwrap();
    s.app_home
        .forms
        .get_mut(&owner)
        .unwrap()
        .set(
            &InputKey {
                block_id: "notes".into(),
                action_id: "edit".into(),
            },
            InputValue::Text("Latest".into()),
        )
        .unwrap();
    inputs::bump(&mut s, &owner);
    assert!(instance_current(&s, &first.binding, first.instance));
}

#[test]
fn initialized_parent_and_response_bit_survive_display_only_props() {
    let mut s = setup(true, true);
    let old = binding(&s, &key("destination")).unwrap();
    let instance = widget(&s).dispatch.as_ref().unwrap().instance;
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_channel"] = json!(C);
        raw["state"] = json!({"values":{"destination":{"pick":{"type":"channels_select","selected_channel":B}}}});
    });
    assert_eq!(widget(&s).visible.as_deref(), Some(C));
    assert_eq!(
        state(&s),
        json!({"type":"channels_select","selected_channel":A})
    );
    assert!(!instance_current(&s, &old, instance));
    choose(&mut s, "destination", B).unwrap();
    assert_eq!(state(&s)["response_url_enabled"], true);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_channel");
        raw["blocks"][0]["element"]["response_url_enabled"] = json!(false);
    });
    assert_eq!(widget(&s).visible, None);
    assert_eq!(
        state(&s),
        json!({"type":"channels_select","selected_channel":B,"response_url_enabled":true})
    );
    choose(&mut s, "destination", C).unwrap();
    assert!(state(&s).get("response_url_enabled").is_none());
}

#[test]
fn fresh_parent_null_and_omitted_both_fall_back_initial_without_rewriting_parent() {
    let mut s = setup(true, false);
    assert!(state(&s).get("selected_channel").is_none());
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_channel"] = json!(C)
    });
    revisit(&mut s);
    assert_eq!(widget(&s).visible.as_deref(), Some(C));
    assert!(state(&s).get("selected_channel").is_none());
    let t = Ticket::capture(&s, &key("destination")).unwrap();
    let mut event = None;
    assert!(remove_impl(&mut s, &t, C, &mut event));
    assert!(event.is_some());
    assert_eq!(widget(&s).visible, None);
    assert_eq!(
        state(&s),
        json!({"type":"channels_select","selected_channel":null,"response_url_enabled":true})
    );
    revisit(&mut s);
    assert_eq!(widget(&s).visible.as_deref(), Some(C));
    assert_eq!(
        state(&s),
        json!({"type":"channels_select","selected_channel":null,"response_url_enabled":true})
    );
}

#[test]
fn quiet_channel_and_other_lookup_kinds_keep_no_dispatch_and_required_clear_is_absent() {
    let mut s = setup(false, true);
    for block in ["destination", "many", "conversation", "conversations"] {
        assert!(choose(&mut s, block, B).is_none());
    }
    assert_eq!(state(&s)["selected_channel"], B);
    refresh(&mut s, |raw| raw["blocks"][0]["optional"] = json!(false));
    let t = Ticket::capture(&s, &key("destination")).unwrap();
    assert!(!remove_impl(&mut s, &t, B, &mut None));
    assert_eq!(state(&s)["selected_channel"], B);
}

#[test]
fn reset_clears_owner_widgets_and_read_then_reseeds_all_four_kinds_only() {
    let mut s = setup(true, true);
    let old = choose(&mut s, "destination", B).unwrap();
    for block in ["many", "conversation", "conversations"] {
        assert!(choose(&mut s, block, B).is_none());
    }
    let owner = inputs::owner(&s).unwrap();
    let mut other = owner.clone();
    other.app = "AOTHER01".into();
    let other_widget = s
        .app_home
        .channels
        .widgets
        .remove(&(owner.clone(), key("conversation")))
        .unwrap();
    s.app_home
        .channels
        .widgets
        .insert((other.clone(), key("conversation")), other_widget);
    s.app_home
        .forms
        .insert(other.clone(), inputs::form(&s).unwrap().clone());
    let t = Ticket::capture(&s, &key("destination")).unwrap();
    assert!(open_local(&mut s, t.clone()));
    let (tx, rx) = tokio::sync::watch::channel(false);
    s.app_home.channels.picker.as_mut().unwrap().cancel = Some(tx);
    let raw = inputs::form(&s).unwrap().raw_view().clone();
    reset(&mut s, &owner);
    assert!(*rx.borrow());
    assert!(s.app_home.channels.picker.is_none());
    assert!(
        s.app_home
            .channels
            .widgets
            .contains_key(&(other.clone(), key("conversation")))
    );
    assert!(!s.app_home.channels.widgets.keys().any(|(o, _)| o == &owner));
    assert!(
        !s.app_home
            .channels
            .multiples
            .keys()
            .any(|(o, _)| o == &owner)
    );
    inputs::static_select::reset(&mut s, &owner);
    s.app_home
        .forms
        .insert(owner.clone(), HomeForm::parse(&raw).unwrap());
    inputs::bump(&mut s, &owner);
    hydrate(&mut s);
    assert!(!instance_current(&s, &old.binding, old.instance));
    assert!(!t.current(&s));
    for block in ["destination", "many", "conversation", "conversations"] {
        assert_eq!(
            display(&s, &key(block))
                .unwrap()
                .into_iter()
                .map(|v| v.0)
                .collect::<Vec<_>>(),
            vec![A]
        );
    }
    assert_eq!(
        s.app_home.channels.widgets[&(other, key("conversation"))]
            .visible
            .as_deref(),
        Some(B)
    );
}

#[test]
fn source_account_socket_and_configured_mode_changes_retire_instance_authority() {
    for change in ["account", "socket", "mode", "source"] {
        let mut s = setup(true, true);
        let e = choose(&mut s, "destination", B).unwrap();
        let ticket = Ticket::capture(&s, &key("destination")).unwrap();
        match change {
            "account" => s.core.account_epoch += 1,
            "socket" => {
                s.core
                    .workspaces
                    .get_mut(&s.core.active_team.clone().unwrap())
                    .unwrap()
                    .rt_generation += 1
            }
            "mode" => {
                s.core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&s.core.active_team.clone().unwrap())
                    .unwrap()
                    .enterprise_id = None
            }
            _ => refresh(&mut s, |raw| raw["hash"] = json!("new-source")),
        }
        assert!(!instance_current(&s, &e.binding, e.instance), "{change}");
        assert!(!ticket.current(&s), "{change}");
    }
}
