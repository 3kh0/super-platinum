//! Serial lifecycle/cadence tests. No browser, physical input or elapsed timer claim.
use super::*;
use super_platinum_core::slack::client::RequestBody;
fn key() -> InputKey {
    InputKey {
        block_id: "email".into(),
        action_id: "value".into(),
    }
}
fn setup(optional: bool, quiet: bool) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([{"type":"input","block_id":"email","optional":optional,"dispatch_action":!quiet,"label":{"type":"plain_text","text":"Email"},"element":{"type":"email_text_input","action_id":"value","initial_value":"Initial@Example.com","dispatch_action_config":{"trigger_actions_on":["on_enter_pressed","on_character_entered"]}}}]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    s.app_home.text = State::default();
    super::super::super::hydrate(&mut s);
    mount(&mut s);
    assert!(super::super::ready(&s));
    s
}
fn binding(s: &ShellState) -> Binding {
    let h = s.app_home.surface.as_ref().unwrap();
    Binding {
        owner: h.owner.clone(),
        form: owner(s).unwrap(),
        visit: actions::Key {
            visit: h.id,
            revision: h.revision,
        },
        key: key(),
        raw: h.view.as_ref().unwrap().raw().clone(),
    }
}
fn index(s: &ShellState) -> (FormOwner, InputKey) {
    (owner(s).unwrap(), key())
}
fn mount(s: &mut ShellState) {
    super::super::poll(s);
    let b = binding(s);
    let c = configure(s, &b).unwrap();
    s.app_home.text.mounts.insert(
        index(s),
        Mount {
            id: next(),
            seq: 0,
            value: String::new(),
            binding: b,
        },
    );
    emit(s, "mounted", c["local"].as_str().unwrap(), false, false);
    assert!(s.app_home.text.emails[&index(s)].initialized);
}
fn emit(
    s: &mut ShellState,
    kind: &str,
    local: &str,
    callback: bool,
    composing: bool,
) -> Option<u64> {
    let b = binding(s);
    let m = &s.app_home.text.mounts[&index(s)];
    let e = Occurrence {
        mount: m.id,
        seq: m.seq + 1,
        kind: kind.into(),
        value: local.into(),
        key: (kind == "enter").then(|| "Enter".into()),
        code: None,
        ctrl: false,
        alt: false,
        shift: false,
        meta: false,
        repeat: false,
        composing,
        trusted: false,
        prevented: kind == "enter",
        number: None,
        email: Some(EmailOccurrence {
            local: local.into(),
            callback,
            callback_value: local.into(),
            enter: true,
            character: true,
            type_mismatch: false,
            composing,
        }),
    };
    super::super::receive(s, &b, &e)
}
fn parent(s: &ShellState) -> Value {
    form(s).unwrap().state().unwrap()["values"]["email"]["value"].clone()
}
fn local(s: &ShellState) -> String {
    s.app_home.text.emails[&index(s)].local.clone()
}
fn refresh(s: &mut ShellState, initial: Option<&str>) {
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    if let Some(initial) = initial {
        raw["blocks"][0]["element"]["initial_value"] = json!(initial);
    } else {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_value");
    }
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    super::super::super::hydrate(s);
    mount(s);
}
#[test]
fn fresh_parent_is_type_only_while_visible_initial_and_native_mount_are_initialized() {
    let s = setup(true, false);
    assert_eq!(parent(&s), json!({"type":"email_text_input"}));
    assert_eq!(local(&s), "Initial@Example.com");
    let expected = bridge::expected_visit(&s, binding(&s).visit).unwrap();
    assert_eq!(expected["fields"][0]["email"], true);
    assert_eq!(expected["fields"][0]["value"], "Initial@Example.com");
}
#[test]
fn single_leading_has_no_trailing_duplicate_equal_valid_callback_replaces_only_trailing() {
    let mut s = setup(true, false);
    let ix = index(&s);
    let first = emit(&mut s, "input", "a@example.com", true, false).unwrap();
    let q = cadence::take_leading(&mut s, &ix, first).unwrap();
    assert_eq!(q.action.action()["value"], "a@example.com");
    assert!(cadence::take_leading(&mut s, &ix, first).is_none());
    assert!(cadence::finish(&mut s, &ix, first).is_none());
    let a = emit(&mut s, "input", "b@example.com", true, false).unwrap();
    assert!(cadence::take_leading(&mut s, &ix, a).is_some());
    let b = emit(&mut s, "input", "b@example.com", true, false).unwrap();
    assert_ne!(a, b);
    assert!(cadence::finish(&mut s, &ix, a).is_none());
    assert_eq!(
        cadence::finish(&mut s, &ix, b).unwrap().action.action()["value"],
        "b@example.com"
    );
    assert!(s.app_home.text.emails[&ix].window.is_none());
}
#[test]
fn invalid_new_parent_keeps_valid_trailing_with_immutable_action_and_latest_full_state() {
    let mut s = setup(false, false);
    let ix = index(&s);
    let a = emit(&mut s, "input", "a@example.com", true, false).unwrap();
    cadence::take_leading(&mut s, &ix, a).unwrap();
    let b = emit(&mut s, "input", "b@example.com", true, false).unwrap();
    assert!(emit(&mut s, "input", "bad", true, false).is_none());
    assert_eq!(parent(&s)["value"], "bad");
    assert!(super::super::ready(&s));
    assert!(super::super::super::presentation_error(&s, &key()).is_none());
    let q = cadence::finish(&mut s, &ix, b).unwrap();
    let view = s.app_home.surface.as_ref().unwrap().view.as_ref().unwrap();
    let request = q
        .action
        .prepare_with_context(
            &q.binding.owner.ctx.client,
            &q.binding.owner.ctx.session,
            view,
            form(&s).unwrap(),
            "fixture",
            q.route.team_id(),
        )
        .unwrap();
    let RequestBody::Form(body) = request.body else {
        panic!("form body")
    };
    let args = body
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        serde_json::from_str::<Value>(&args["actions"]).unwrap()[0]["value"],
        "b@example.com"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&args["state"]).unwrap()["values"]["email"]["value"]["value"],
        "bad"
    );
}
#[test]
fn mounted_props_do_not_sync_but_fresh_retained_widget_applies_initial_once() {
    let mut s = setup(true, false);
    emit(&mut s, "input", "draft@example.com", true, false);
    refresh(&mut s, Some("New@Example.com"));
    assert_eq!(local(&s), "draft@example.com");
    assert_eq!(parent(&s)["value"], "draft@example.com");
    assert!(s.app_home.text.emails[&index(&s)].window.is_none());
    s.app_home.surface.as_mut().unwrap().id = next();
    hydrate(&mut s, &[]);
    mount(&mut s);
    assert_eq!(local(&s), "New@Example.com");
    assert_eq!(parent(&s)["value"], "New@Example.com");
    assert!(s.app_home.text.emails[&index(&s)].window.is_none());
    refresh(&mut s, None);
    s.app_home.surface.as_mut().unwrap().id = next();
    hydrate(&mut s, &[]);
    mount(&mut s);
    assert_eq!(local(&s), "");
    assert_eq!(parent(&s)["value"], "New@Example.com");
}
#[test]
fn reset_reseeds_new_parent_and_spends_original_native_and_queued_authority() {
    let mut s = setup(true, false);
    let ix = index(&s);
    let id = emit(&mut s, "input", "draft@example.com", true, false).unwrap();
    let old = cadence::take_leading(&mut s, &ix, id).unwrap();
    let old_mount = old.mount;
    reset(&mut s, &ix.0);
    let raw = form(&s).unwrap().raw_view().clone();
    s.app_home
        .forms
        .insert(ix.0.clone(), HomeForm::parse(&raw).unwrap());
    s.app_home.surface.as_mut().unwrap().revision = next();
    hydrate(&mut s, &[key()]);
    mount(&mut s);
    assert_ne!(s.app_home.text.mounts[&ix].id, old_mount);
    assert!(!old.current(&s));
    assert_eq!(parent(&s), json!({"type":"email_text_input"}));
    assert_eq!(local(&s), "Initial@Example.com");
}
#[test]
fn quiet_changes_commit_and_pending_callback_manager_does_not_reject_direct_admitted_event() {
    let mut quiet = setup(true, true);
    assert!(emit(&mut quiet, "input", "", true, false).is_none());
    assert_eq!(parent(&quiet)["value"], "");
    assert!(button::needed(&quiet));
    let mut s = setup(true, false);
    let b = binding(&s);
    s.app_home.text.activity.insert(
        (b.form.clone(), key().block_id),
        Activity {
            binding: b,
            route: context::Witness::capture(&s).unwrap(),
            loading: true,
            error: None,
        },
    );
    assert!(loading(&s, &key()));
    assert!(emit(&mut s, "input", "a@example.com", true, false).is_some());
}
#[test]
fn composition_blocks_readiness_without_inventing_parent_clear_and_stale_source_cannot_commit() {
    let mut s = setup(true, false);
    emit(&mut s, "composition", "draft", false, true);
    assert!(!super::super::ready(&s));
    assert_eq!(parent(&s), json!({"type":"email_text_input"}));
    emit(&mut s, "input", "a@example.com", true, false);
    assert!(super::super::ready(&s));
    let b = binding(&s);
    let old = s.app_home.text.emails[&index(&s)]
        .window
        .as_ref()
        .unwrap()
        .id;
    s.app_home.surface.as_mut().unwrap().revision = next();
    assert!(!b.current(&s));
    assert!(poll(&mut s));
    assert!(cadence::finish(&mut s, &(b.form, key()), old).is_none());
}

#[test]
fn replaced_native_instance_spends_old_window_and_next_callback_gets_fresh_leading() {
    let mut s = setup(true, false);
    let ix = index(&s);
    let id = emit(&mut s, "input", "old@example.com", true, false).unwrap();
    let old = cadence::take_leading(&mut s, &ix, id).unwrap();
    mount(&mut s);
    assert!(!old.current(&s));
    let fresh = emit(&mut s, "input", "new@example.com", true, false).unwrap();
    assert!(cadence::take_leading(&mut s, &ix, fresh).is_some());
    assert!(cadence::finish(&mut s, &ix, fresh).is_none());
}
