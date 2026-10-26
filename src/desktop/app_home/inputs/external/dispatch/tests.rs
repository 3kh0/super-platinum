use super::*;
use crate::app_home::inputs::external;

fn key() -> InputKey {
    external::fixture::key()
}
fn option(label: &str, value: &str) -> ExternalOption {
    ExternalOption::parse(&external::fixture::option(label, value)).unwrap()
}
fn setup(optional: bool, quiet: bool) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    external::fixture::install(&mut s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
        {"type":"input","block_id":"home-choice","dispatch_action":!quiet,"optional":optional,
         "label":{"type":"plain_text","text":"Option"},"element":{"type":"external_select","action_id":"value","min_query_length":0,
          "initial_option":option("Installed","installed").raw(),"placeholder":{"type":"plain_text","text":"Find release"}}},
        {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Notes"},
         "element":{"type":"plain_text_input","action_id":"value","initial_value":"Before"}}
    ]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    s.app_home.external.catalogs.clear();
    inputs::hydrate(&mut s);
    assert!(widget(&s, &key()).is_some());
    assert!(Ticket::capture(&s, &key()).is_some());
    s
}
fn refresh(s: &mut ShellState, edit: impl FnOnce(&mut Value)) {
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    edit(&mut raw);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
}
fn parent(s: &ShellState) -> Value {
    inputs::form(s).unwrap().state().unwrap()["values"]["home-choice"]["value"].clone()
}
fn query(s: &mut ShellState, reply: external::fixture::Reply) -> QueryOwner {
    let ticket = Ticket::capture(s, &key()).unwrap();
    let owner = external::open_local(s, ticket).unwrap();
    let revision = owner.query(s).unwrap().revision;
    assert!(shared::complete(
        s,
        &owner,
        revision,
        external::fixture::response(reply)
    ));
    owner
}
fn choose(s: &mut ShellState, owner: QueryOwner, index: usize) -> Option<Event> {
    let rev = owner.query(s).unwrap().revision;
    let mut event = None;
    assert!(external::choose_impl(s, owner, rev, index, &mut event));
    event
}
#[test]
fn first_parent_initialization_keeps_full_initial_and_mounted_props_do_not_rewrite_parent() {
    let mut s = setup(true, false);
    let original = parent(&s);
    let w = widget(&s, &key()).unwrap();
    let instance = w.instance;
    let revision = w.revision;
    assert_eq!(
        w.visible.as_ref().unwrap().raw(),
        option("Installed", "installed").raw()
    );
    // Hydration after the initializer has stored its parent is saved-only:
    // it must not normalize the already-mounted full descriptor.
    inputs::hydrate(&mut s);
    refresh(&mut s, |raw| raw["hash"] = json!("equal-initial"));
    assert_eq!(widget(&s, &key()).unwrap().revision, revision);
    assert_eq!(
        display(&s, &key()).unwrap().unwrap().raw(),
        option("Installed", "installed").raw()
    );
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_option"]["description"]["text"] =
            json!("Changed description")
    });
    assert_eq!(widget(&s, &key()).unwrap().instance, instance);
    assert_eq!(
        display(&s, &key()).unwrap().unwrap().description(),
        Some("Changed description")
    );
    assert_eq!(parent(&s), original);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_option");
    });
    assert_eq!(display(&s, &key()), Some(None));
    assert_eq!(parent(&s), original);
    refresh(
        &mut s,
        |raw| raw["state"] = json!({"values":{"home-choice":{"value":{"type":"external_select","selected_option":option("Saved only","saved").raw()}}}}),
    );
    assert_eq!(display(&s, &key()), Some(None));
    assert_eq!(parent(&s), original);
}
#[test]
fn current_result_descriptor_replaces_only_on_choice_with_stable_instance() {
    let mut s = setup(true, false);
    let instance = widget(&s, &key()).unwrap().instance;
    let owner = query(&mut s, external::fixture::Reply::Options);
    let first = choose(&mut s, owner, 0).unwrap();
    let full = parent(&s)["selected_option"].clone();
    assert_eq!(display(&s, &key()).unwrap().unwrap().raw(), &full);
    let owner = query(&mut s, external::fixture::Reply::Options);
    let old_rev = owner.query(&s).unwrap().revision;
    assert!(shared::complete(
        &mut s,
        &owner,
        old_rev,
        external::fixture::response(external::fixture::Reply::Replacement)
    ));
    assert_eq!(parent(&s)["selected_option"], full);
    assert_eq!(display(&s, &key()).unwrap().unwrap().raw(), &full);
    assert!(!external::choose_impl(
        &mut s,
        owner.clone(),
        old_rev,
        0,
        &mut None
    ));
    let renamed = choose(&mut s, owner, 0).unwrap();
    assert_ne!(parent(&s)["selected_option"], full);
    assert_eq!(parent(&s)["selected_option"]["value"], full["value"]);
    assert_eq!(first.instance, instance);
    assert_eq!(renamed.instance, instance);
    assert!(instance_current(&s, &first.binding, instance));
    assert!(external::validated(&s));
}
#[test]
fn clear_remount_falls_back_initial_while_full_parent_stays_null() {
    let mut s = setup(true, false);
    let t = Ticket::capture(&s, &key()).unwrap();
    let old_instance = widget(&s, &key()).unwrap().instance;
    let mut event = None;
    assert!(external::clear_impl(&mut s, t, &mut event));
    assert!(event.is_some());
    assert_eq!(display(&s, &key()), Some(None));
    assert!(parent(&s)["selected_option"].is_null());
    let source = external::fixture::depart(&mut s);
    external::fixture::return_home(&mut s, source);
    assert_ne!(widget(&s, &key()).unwrap().instance, old_instance);
    assert_eq!(
        display(&s, &key()).unwrap().unwrap().raw(),
        option("Installed", "installed").raw()
    );
    assert!(parent(&s)["selected_option"].is_null());
}
#[test]
fn omitted_parent_revisit_and_dirty_descriptor_projection_are_distinct() {
    let mut s = setup(true, true);
    s.app_home.forms.clear();
    let owner = inputs::owner(&s).unwrap();
    reset(&mut s, &owner);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_option");
    });
    assert!(parent(&s).get("selected_option").is_none());
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_option"] = option("Later", "later").raw().clone()
    });
    assert!(parent(&s).get("selected_option").is_none());
    let source = external::fixture::depart(&mut s);
    external::fixture::return_home(&mut s, source);
    assert_eq!(
        display(&s, &key()).unwrap().unwrap().raw(),
        option("Later", "later").raw()
    );
    let owner = query(&mut s, external::fixture::Reply::Options);
    assert!(choose(&mut s, owner, 0).is_none());
    let full = parent(&s);
    let source = external::fixture::depart(&mut s);
    external::fixture::return_home(&mut s, source);
    assert_eq!(parent(&s), full);
    let visible = display(&s, &key()).unwrap().unwrap();
    assert!(visible.raw().get("description").is_none());
    assert!(visible.raw().get("opaque").is_none());
    assert_eq!(visible.value(), "alpha,desktop");
    assert!(external::validated(&s));
}
#[test]
fn required_empty_value_is_real_choice_but_direct_null_is_not_native_clear() {
    let mut s = setup(false, false);
    let t = Ticket::capture(&s, &key()).unwrap();
    assert!(!external::clear_impl(&mut s, t.clone(), &mut None));
    let mut action = None;
    assert!(edit(
        &mut s,
        &t,
        Some(option("Empty value", "")),
        &mut action
    ));
    assert!(action.is_some());
    assert_eq!(parent(&s)["selected_option"]["value"], "");
    let t = Ticket::capture(&s, &key()).unwrap();
    action = None;
    assert!(edit(&mut s, &t, None, &mut action));
    assert!(action.is_none());
    assert!(parent(&s)["selected_option"].is_null());
    assert!(inputs::form(&s).unwrap().fields[&key()].error.is_some());
    assert!(inputs::presentation_error(&s, &key()).is_none());
}
#[test]
fn source_and_socket_retire_old_instances_without_reusing_selected_as_query_authority() {
    let mut s = setup(true, false);
    let owner = query(&mut s, external::fixture::Reply::Options);
    let action = choose(&mut s, owner, 0).unwrap();
    refresh(&mut s, |raw| raw["hash"] = json!("changed"));
    assert!(!instance_current(&s, &action.binding, action.instance));
    let owner = query(&mut s, external::fixture::Reply::Empty);
    let rev = owner.query(&s).unwrap().revision;
    assert!(!external::choose_impl(&mut s, owner, rev, 0, &mut None));
    assert!(parent(&s)["selected_option"].is_object());
    let w = widget(&s, &key()).unwrap();
    let b = w.binding.clone();
    let instance = w.instance;
    s.core
        .workspaces
        .get_mut(&b.owner.ctx.session.team_id)
        .unwrap()
        .rt_generation += 1;
    assert!(!instance_current(&s, &b, instance));
    assert!(display(&s, &key()).is_none());
}
#[test]
fn owner_reset_clears_single_and_quiet_multi_display_and_old_query() {
    let mut s = setup(true, false);
    refresh(&mut s, |raw| {
        raw["blocks"].as_array_mut().unwrap().push(json!({
        "type":"input","block_id":"multi","optional":true,"label":{"type":"plain_text","text":"Quiet"},
        "element":{"type":"multi_external_select","action_id":"value","min_query_length":0,"initial_options":[option("Multi initial","mi").raw()]}
    }))
    });
    let multi = InputKey {
        block_id: "multi".into(),
        action_id: "value".into(),
    };
    let t = Ticket::capture(&s, &multi).unwrap();
    assert!(multiple::remove_local(&mut s, t, "mi"));
    assert!(multiple::visible(&s, &multi).unwrap().is_empty());
    let owner = query(&mut s, external::fixture::Reply::Options);
    let old_instance = widget(&s, &key()).unwrap().instance;
    let form_owner = inputs::owner(&s).unwrap();
    let raw = inputs::form(&s).unwrap().raw_view().clone();
    let replacement =
        super_platinum_core::block_actions::form::home::HomeForm::parse(&raw).unwrap();
    reset(&mut s, &form_owner);
    assert!(owner.query(&s).is_none());
    assert!(s.app_home.external.catalogs.is_empty());
    s.app_home.forms.insert(form_owner, replacement);
    s.app_home.surface.as_mut().unwrap().revision = next();
    let initialized = inputs::external_keys(&s);
    external::hydrate_initialized(&mut s, &initialized);
    assert_ne!(widget(&s, &key()).unwrap().instance, old_instance);
    assert_eq!(
        display(&s, &key()).unwrap().unwrap().raw(),
        option("Installed", "installed").raw()
    );
    assert_eq!(multiple::visible(&s, &multi).unwrap()[0].value(), "mi");
    assert_eq!(
        parent(&s)["selected_option"],
        option("Installed", "installed").raw().clone()
    );
    assert!(s.app_home.external.picker.is_none());
}

#[test]
fn pristine_single_retyped_to_quiet_multi_does_not_retain_a_single_parent_token() {
    let mut s = setup(true, true);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"] = json!({"type":"multi_external_select","action_id":"value",
            "min_query_length":0,"initial_options":[option("Multi", "multi").raw()]});
    });
    assert!(inputs::supported(&s));
    assert!(display(&s, &key()).is_none());
    assert!(proof(&s)["externalWidgets"].as_array().unwrap().is_empty());
    assert_eq!(multiple::visible(&s, &key()).unwrap()[0].value(), "multi");
    assert!(Ticket::capture(&s, &key()).is_some());

    let mut dirty = setup(true, true);
    let query_owner = query(&mut dirty, external::fixture::Reply::Options);
    assert!(choose(&mut dirty, query_owner, 0).is_none());
    let old_raw = inputs::form(&dirty).unwrap().raw_view().clone();
    let old_state = inputs::form(&dirty).unwrap().state().unwrap();
    refresh(&mut dirty, |raw| {
        raw["blocks"][0]["element"] = json!({"type":"multi_external_select","action_id":"value",
            "min_query_length":0,"initial_options":[option("Multi", "multi").raw()]});
    });
    assert!(!inputs::supported(&dirty));
    assert_eq!(inputs::form(&dirty).unwrap().raw_view(), &old_raw);
    assert_eq!(inputs::form(&dirty).unwrap().state().unwrap(), old_state);
    assert!(Ticket::capture(&dirty, &key()).is_none());
}

#[test]
fn newly_initialized_field_and_retained_parent_revisit_have_distinct_seeds() {
    let mut s = setup(true, true);
    let committed = parent(&s);
    let source = external::fixture::depart(&mut s);
    external::fixture::return_home(&mut s, source);
    assert_eq!(parent(&s), committed);
    assert_eq!(
        display(&s, &key()).unwrap().unwrap().raw(),
        &json!({"text":{"type":"plain_text","text":"Installed"},"value":"installed"})
    );
    refresh(&mut s, |raw| {
        raw["blocks"].as_array_mut().unwrap().push(json!({
            "type":"input","block_id":"introduced","optional":true,
            "label":{"type":"plain_text","text":"New field"},
            "element":{"type":"external_select","action_id":"value","min_query_length":0,
                "initial_option":option("New initial", "new").raw()}}));
    });
    let introduced = InputKey {
        block_id: "introduced".into(),
        action_id: "value".into(),
    };
    assert_eq!(
        display(&s, &introduced).unwrap().unwrap().raw(),
        option("New initial", "new").raw()
    );
    let source = external::fixture::depart(&mut s);
    external::fixture::return_home(&mut s, source);
    let display = display(&s, &introduced).unwrap().unwrap();
    assert!(display.raw().get("description").is_none());
    assert_eq!(display.value(), "new");
}
