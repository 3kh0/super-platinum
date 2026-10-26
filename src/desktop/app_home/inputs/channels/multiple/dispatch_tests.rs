//! Frozen xjrl/z+fW62: serial-shell callback custody, not browser/timer execution.
use super::*;
use crate::app_home::inputs;
use super_platinum_core::slack::models::Channel;
const A: &str = "CINITIAL1";
const B: &str = "CSELECT02";
const C: &str = "CSELECT03";
fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "pick".into(),
    }
}
fn setup(initial: Option<Vec<&str>>, optional: bool) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let team = s.core.active_team.clone().unwrap();
    for id in [A, B, C] {
        let channel: Channel = serde_json::from_value(json!({"id":id,"name":id,"is_channel":true,"is_member":true,"internal_team_ids":["TOTHER001"]})).unwrap();
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .channels
            .insert(id.into(), channel);
    }
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
        {"type":"input","block_id":"many","dispatch_action":true,"optional":optional,"label":{"type":"plain_text","text":"Channels"},"element":{"type":"multi_channels_select","action_id":"pick","max_selected_items":2,"response_url_enabled":true}},
        {"type":"input","block_id":"conversations","optional":true,"label":{"type":"plain_text","text":"Quiet conversations"},"element":{"type":"multi_conversations_select","action_id":"pick","initial_conversations":[A],"filter":{"include":["public"]}}}
    ]);
    if let Some(initial) = initial {
        raw["blocks"][0]["element"]["initial_channels"] = json!(initial);
    }
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    inputs::hydrate(&mut s);
    assert!(inputs::supported(&s));
    let w = widget(&s);
    assert!(dispatch::instance_current(
        &s,
        &w.dispatch.as_ref().unwrap().binding,
        w.dispatch.as_ref().unwrap().instance
    ));
    s
}
fn widget(s: &ShellState) -> &Widget {
    &s.app_home.channels.multiples[&(inputs::owner(s).unwrap(), key("many"))]
}
fn wire(s: &ShellState) -> Value {
    inputs::form(s).unwrap().state().unwrap()["values"]["many"]["pick"].clone()
}
fn choose(s: &mut ShellState, block: &str, id: &str) -> Option<dispatch::Event> {
    let t = Ticket::capture(s, &key(block)).unwrap();
    assert!(super::super::open_local(s, t.clone()));
    let p = s.app_home.channels.picker.as_ref().unwrap();
    let (serial, revision) = (p.serial, p.query_revision);
    let mut event = None;
    assert!(super::super::choose_impl(
        s, &t, serial, revision, id, &mut event
    ));
    event
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
    let current = Owner::capture(s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = current.home_visit;
    h.owner = current;
    super::super::hydrate(s);
}

#[test]
fn ordered_edits_keep_event_instance_but_retire_held_query_and_ticket() {
    let mut s = setup(Some(vec![A]), true);
    let t = Ticket::capture(&s, &key("many")).unwrap();
    assert!(t.mode.is_none());
    assert!(super::super::open_local(&mut s, t.clone()));
    let (tx, rx) = tokio::sync::watch::channel(false);
    let p = s.app_home.channels.picker.as_mut().unwrap();
    p.cancel = Some(tx);
    p.loading = true;
    p.query = "held".into();
    let (serial, revision) = (p.serial, p.query_revision);
    let rows: Vec<_> = p.candidates.iter().map(|c| c.id.clone()).collect();
    let mut first = None;
    assert!(super::super::choose_impl(
        &mut s, &t, serial, revision, B, &mut first
    ));
    let first = first.unwrap();
    assert!(*rx.borrow());
    assert!(!t.current(&s));
    let p = s.app_home.channels.picker.as_ref().unwrap();
    assert_eq!(p.serial, serial);
    assert_ne!(p.query_revision, revision);
    assert_eq!(p.query, "held");
    assert!(!p.loading);
    assert!(p.ticket.current(&s));
    assert_eq!(
        p.candidates
            .iter()
            .map(|c| c.id.clone())
            .collect::<Vec<_>>(),
        rows
    );
    let removed = choose(&mut s, "many", A).unwrap();
    let readded = choose(&mut s, "many", A).unwrap();
    assert_eq!(widget(&s).visible, vec![B, A]);
    assert_eq!(
        wire(&s),
        json!({"type":"multi_channels_select","selected_channels":[B,A]})
    );
    assert_eq!(first.instance, removed.instance);
    assert_eq!(first.instance, readded.instance);
    assert!(dispatch::instance_current(
        &s,
        &first.binding,
        first.instance
    ));
}

#[test]
fn overmax_and_required_empty_commit_without_new_event_and_max_props_do_not_revalidate_local_flag()
{
    let mut s = setup(Some(vec![A]), false);
    let first = choose(&mut s, "many", B).unwrap();
    assert!(choose(&mut s, "many", C).is_none());
    assert_eq!(wire(&s)["selected_channels"], json!([A, B, C]));
    assert!(widget(&s).invalid);
    assert!(dispatch::instance_current(
        &s,
        &first.binding,
        first.instance
    ));
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["max_selected_items"] = json!(4)
    });
    assert!(widget(&s).invalid);
    assert!(!presentation(&s, &key("many")).unwrap().invalid);
    for id in [A, B] {
        assert!(choose(&mut s, "many", id).is_some());
    }
    assert!(choose(&mut s, "many", C).is_none());
    assert_eq!(wire(&s)["selected_channels"], json!([]));
    assert!(!widget(&s).invalid);
    assert!(
        inputs::form(&s).unwrap().fields[&key("many")]
            .error
            .is_some()
    );
}

#[test]
fn initialized_omission_and_explicit_empty_have_distinct_fresh_seeds() {
    let mut s = setup(None, true);
    assert_eq!(wire(&s), json!({"type":"multi_channels_select"}));
    let old = widget(&s).dispatch.as_ref().unwrap().instance;
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_channels"] = json!([C])
    });
    assert_eq!(widget(&s).visible, vec![C]);
    assert_eq!(wire(&s), json!({"type":"multi_channels_select"}));
    assert_eq!(widget(&s).dispatch.as_ref().unwrap().instance, old);
    revisit(&mut s);
    assert_ne!(widget(&s).dispatch.as_ref().unwrap().instance, old);
    assert_eq!(widget(&s).visible, vec![C]);
    assert_eq!(wire(&s), json!({"type":"multi_channels_select"}));
    choose(&mut s, "many", C).unwrap();
    assert_eq!(
        wire(&s),
        json!({"type":"multi_channels_select","selected_channels":[]})
    );
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_channels"] = json!([B])
    });
    assert_eq!(widget(&s).visible, vec![B]);
    revisit(&mut s);
    assert!(widget(&s).visible.is_empty());
    assert_eq!(wire(&s)["selected_channels"], json!([]));
}

#[test]
fn initialized_parent_survives_saved_and_initial_props_while_quiet_conversation_keeps_own_path() {
    let mut s = setup(Some(vec![A]), true);
    refresh(&mut s, |raw| {
        raw["state"] = json!({"values":{"many":{"pick":{"type":"multi_channels_select","selected_channels":[B]}}}});
        raw["blocks"][0]["element"]["initial_channels"] = json!([C]);
    });
    assert_eq!(wire(&s)["selected_channels"], json!([A]));
    assert_eq!(widget(&s).visible, vec![C]);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_channels");
    });
    assert_eq!(widget(&s).visible, vec![C]);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_channels"] = json!([])
    });
    assert!(widget(&s).visible.is_empty());
    assert_eq!(wire(&s)["selected_channels"], json!([A]));
    assert!(choose(&mut s, "conversations", B).is_none());
    let o = inputs::owner(&s).unwrap();
    assert!(
        s.app_home.channels.multiples[&(o, key("conversations"))]
            .dispatch
            .is_none()
    );
    assert!(!dispatch::disabled(&s, &key("conversations")));
    assert_eq!(dispatch::parents(&s).len(), 1);
}

#[test]
fn multi_requires_no_single_enterprise_mode_and_reset_retires_its_instance() {
    let mut s = setup(Some(vec![A]), true);
    let team = s.core.active_team.clone().unwrap();
    s.core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .enterprise_id = Some("".into());
    let current = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = current.home_visit;
    h.owner = current;
    inputs::hydrate(&mut s);
    assert!(
        super::super::mode::Observation::capture(&s, &s.app_home.surface.as_ref().unwrap().owner)
            .is_none()
    );
    let e = choose(&mut s, "many", B).unwrap();
    assert!(dispatch::instance_current(&s, &e.binding, e.instance));
    assert!(Ticket::capture(&s, &key("many")).unwrap().mode.is_none());
    let raw = inputs::form(&s).unwrap().raw_view().clone();
    dispatch::reset(&mut s, &e.binding.form);
    assert!(!dispatch::instance_current(&s, &e.binding, e.instance));
    s.app_home
        .forms
        .insert(e.binding.form.clone(), HomeForm::parse(&raw).unwrap());
    super::super::hydrate(&mut s);
    assert_eq!(widget(&s).visible, vec![A]);
    assert_ne!(widget(&s).dispatch.as_ref().unwrap().instance, e.instance);
    assert!(!dispatch::instance_current(&s, &e.binding, e.instance));
}
