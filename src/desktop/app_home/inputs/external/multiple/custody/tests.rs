use super::*;
use serde_json::json;
fn key() -> InputKey {
    fixture::key()
}
fn opt(label: &str, value: &str) -> ExternalOption {
    ExternalOption::parse(&fixture::option(label, value)).unwrap()
}
fn setup(optional: bool, quiet: bool) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([{"type":"input","block_id":"home-choice","optional":optional,"dispatch_action":!quiet,
        "label":{"type":"plain_text","text":"Options"},"element":{"type":"multi_external_select","action_id":"value","min_query_length":0,"max_selected_items":2,
        "initial_options":[opt("Installed","installed").raw()],"placeholder":{"type":"plain_text","text":"Choose"}}}]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    s.app_home.external.catalogs.clear();
    inputs::hydrate(&mut s);
    assert!(widget(&s, &key()).is_some());
    assert!(Ticket::capture(&s, &key()).is_some());
    s
}
fn parent(s: &ShellState) -> Value {
    inputs::form(s).unwrap().state().unwrap()["values"]["home-choice"]["value"].clone()
}
fn refresh(s: &mut ShellState, f: impl FnOnce(&mut Value)) {
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    f(&mut raw);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
}
fn edit(s: &mut ShellState, options: Vec<ExternalOption>) -> Option<dispatch::Event> {
    let mut e = None;
    assert!(apply(s, &key(), options, &mut e));
    e
}
#[test]
fn new_parent_full_initial_retained_mount_projection_and_reset_are_distinct() {
    let mut s = setup(true, false);
    let original = opt("Installed", "installed");
    let p = parent(&s);
    assert_eq!(visible(&s, &key()).unwrap(), vec![original.clone()]);
    inputs::hydrate(&mut s);
    assert_eq!(visible(&s, &key()).unwrap(), vec![original.clone()]);
    let old = widget(&s, &key()).unwrap().instance;
    crate::app_home::fixture::revisit(&mut s);
    inputs::hydrate(&mut s);
    let projected = visible(&s, &key()).unwrap();
    assert_ne!(widget(&s, &key()).unwrap().instance, old);
    assert_eq!(
        projected[0].raw(),
        &json!({"text":original.raw()["text"],"value":"installed"})
    );
    assert_eq!(parent(&s), p);
    let o = inputs::owner(&s).unwrap();
    dispatch::reset(&mut s, &o);
    let raw = inputs::form(&s).unwrap().raw_view().clone();
    s.app_home.forms.insert(o, HomeForm::parse(&raw).unwrap());
    super::super::super::hydrate_initialized(&mut s, &[key()]);
    assert_eq!(visible(&s, &key()).unwrap(), vec![original]);
    assert!(pending_revision(&s, &key()).is_none());
}
#[test]
fn props_callback_is_current_take_once_and_equal_selected_initial_does_not_emit() {
    let mut s = setup(true, false);
    let old = widget(&s, &key()).unwrap().binding.clone();
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]["initial_options"] = json!([opt("Beta", "beta").raw()])
    });
    assert_eq!(parent(&s)["selected_options"][0]["value"], "beta");
    let rev = pending_revision(&s, &key());
    let event = take_pending(&mut s, &key(), rev).unwrap();
    assert!(instance_current(&s, &event.binding, event.instance));
    assert!(!old.current(&s));
    assert!(take_pending(&mut s, &key(), rev).is_none());
    // Change selected locally to Gamma, then an initial prop equal to that selection.
    assert!(edit(&mut s, vec![opt("Gamma", "gamma")]).is_some());
    let before = widget(&s, &key()).unwrap().revision;
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]["initial_options"] = json!([opt("Gamma", "gamma").raw()])
    });
    assert!(pending_revision(&s, &key()).is_none());
    assert_eq!(widget(&s, &key()).unwrap().revision, before);
}
#[test]
fn local_overmax_preserves_parent_and_equal_selection_does_not_revalidate_after_max_change() {
    let mut s = setup(true, false);
    let p = parent(&s);
    let values = vec![opt("A", "a"), opt("B", "b"), opt("C", "c")];
    assert!(edit(&mut s, values.clone()).is_none());
    assert_eq!(parent(&s), p);
    assert!(widget(&s, &key()).unwrap().invalid);
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]["max_selected_items"] = json!(4)
    });
    assert!(!presentation(&s, &key()).unwrap().warning);
    let rev = widget(&s, &key()).unwrap().revision;
    assert!(edit(&mut s, values).is_none());
    assert!(widget(&s, &key()).unwrap().invalid);
    assert_eq!(widget(&s, &key()).unwrap().revision, rev);
    assert!(edit(&mut s, vec![opt("A", "a")]).is_some());
    assert!(!widget(&s, &key()).unwrap().invalid);
}
#[test]
fn required_empty_stores_parent_but_overmax_initial_stays_local_and_valid_empty_value_emits() {
    let mut s = setup(false, false);
    assert!(edit(&mut s, vec![]).is_none());
    assert_eq!(parent(&s)["selected_options"], json!([]));
    let p = parent(&s);
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]["initial_options"] = json!([
            opt("A", "a").raw(),
            opt("B", "b").raw(),
            opt("C", "c").raw()
        ])
    });
    assert_eq!(visible(&s, &key()).unwrap().len(), 3);
    assert_eq!(parent(&s), p);
    assert!(pending_revision(&s, &key()).is_none());
    assert!(edit(&mut s, vec![opt("Empty", "")]).is_some());
    assert_eq!(parent(&s)["selected_options"][0]["value"], "");
}
#[test]
fn stale_prop_handoff_and_reset_cannot_replay_while_quiet_props_still_update_parent() {
    let mut s = setup(true, false);
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]["initial_options"] = json!([opt("B", "b").raw()])
    });
    let rev = pending_revision(&s, &key());
    refresh(&mut s, |r| r["hash"] = json!("new-source"));
    assert!(take_pending(&mut s, &key(), rev).is_none());
    let old = widget(&s, &key()).unwrap().binding.clone();
    let instance = widget(&s, &key()).unwrap().instance;
    let o = inputs::owner(&s).unwrap();
    dispatch::reset(&mut s, &o);
    assert!(!instance_current(&s, &old, instance));
    let mut quiet = setup(true, true);
    refresh(&mut quiet, |r| {
        r["blocks"][0]["element"]["initial_options"] = json!([])
    });
    assert_eq!(parent(&quiet)["selected_options"], json!([]));
    assert!(pending_revision(&quiet, &key()).is_none());
}
#[test]
fn saved_only_and_missing_initial_do_not_replace_initialized_parent_or_display() {
    let mut s = setup(true, false);
    let p = parent(&s);
    let v = visible(&s, &key());
    refresh(
        &mut s,
        |r| r["state"] = json!({"values":{"home-choice":{"value":{"type":"multi_external_select","selected_options":[opt("Saved","saved").raw()]}}}}),
    );
    assert_eq!(parent(&s), p);
    assert_eq!(visible(&s, &key()), v);
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_options");
    });
    assert_eq!(parent(&s), p);
    assert_eq!(visible(&s, &key()), v);
    assert!(pending_revision(&s, &key()).is_none());
}
#[test]
fn descriptor_change_and_order_are_real_selection_but_equal_callback_has_no_effect() {
    let mut s = setup(true, false);
    let a = opt("Alpha", "same");
    let b = opt("Beta", "same");
    let first = edit(&mut s, vec![a.clone()]).unwrap();
    let rev = widget(&s, &key()).unwrap().revision;
    assert!(edit(&mut s, vec![a]).is_none());
    assert_eq!(widget(&s, &key()).unwrap().revision, rev);
    assert!(edit(&mut s, vec![b.clone()]).is_some());
    assert_eq!(parent(&s)["selected_options"][0], b.raw().clone());
    assert!(instance_current(&s, &first.binding, first.instance));
    s.core.account_epoch += 1;
    assert!(!instance_current(&s, &first.binding, first.instance));
}

#[test]
fn explicit_valid_callback_supersedes_pending_props_but_invalid_edit_preserves_it() {
    let mut s = setup(true, false);
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]["initial_options"] = json!([opt("B", "b").raw()])
    });
    assert!(pending_revision(&s, &key()).is_some());
    assert!(edit(&mut s, vec![opt("A", "a"), opt("B", "b"), opt("C", "c")]).is_none());
    let rev = pending_revision(&s, &key());
    assert!(rev.is_some());
    assert!(take_pending(&mut s, &key(), rev).is_some());
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]["initial_options"] = json!([opt("C", "c").raw()])
    });
    assert!(pending_revision(&s, &key()).is_some());
    assert!(edit(&mut s, vec![opt("D", "d")]).is_some());
    assert!(pending_revision(&s, &key()).is_none());
}
