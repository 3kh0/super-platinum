//! Actual owned-form/controller path, inert request preparation and lifecycle.
//! Native input/IME/sanitization/lease paint and elapsed timing are not claimed.
use super::*;
use super_platinum_core::slack::client::RequestBody;
fn key() -> InputKey {
    InputKey {
        block_id: "url".into(),
        action_id: "value".into(),
    }
}
fn index(s: &ShellState) -> (FormOwner, InputKey) {
    (owner(s).unwrap(), key())
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
fn setup(quiet: bool) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
        {"type":"input","block_id":"url","dispatch_action":!quiet,"element":{"type":"url_text_input","action_id":"value","initial_value":"https://Initial.test/Path","dispatch_action_config":{"trigger_actions_on":["on_enter_pressed","on_character_entered"]}}},
        {"type":"input","block_id":"sibling","optional":true,"element":{"type":"plain_text_input","action_id":"text"}}
    ]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    s.app_home.text = State::default();
    super::super::super::hydrate(&mut s);
    mount(&mut s);
    s
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
    emit(
        s,
        "mounted",
        c["local"].as_str().unwrap(),
        false,
        true,
        false,
        false,
    );
    assert!(super::super::ready(s));
}
fn emit(
    s: &mut ShellState,
    kind: &str,
    value: &str,
    callback: bool,
    valid: bool,
    bad: bool,
    composing: bool,
) -> Option<u64> {
    let b = binding(s);
    let m = &s.app_home.text.mounts[&index(s)];
    let local = if bad || composing {
        s.app_home.text.urls[&index(s)].local.clone()
    } else {
        value.into()
    };
    receive(
        s,
        &b,
        &UrlOccurrence {
            mount: m.id,
            seq: m.seq + 1,
            kind: kind.into(),
            value: value.into(),
            key: (kind == "enter").then(|| "Enter".into()),
            prevented: kind == "enter",
            local: local.clone(),
            callback,
            callback_value: local,
            valid,
            bad_input: bad,
            composing,
        },
    )
}
fn parent(s: &ShellState) -> Value {
    form(s).unwrap().state().unwrap()["values"]["url"]["value"].clone()
}
fn refresh(s: &mut ShellState, initial: Option<&str>) {
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    if let Some(initial) = initial {
        raw["blocks"][0]["element"]["initial_value"] = json!(initial)
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
fn home_url_first_owned_complete_literal_queue_buffer_remount_visit_and_reset_journey() {
    let mut quiet = setup(true);
    assert!(button::needed(&quiet));
    assert!(emit(&mut quiet, "input", "", true, true, false, false).is_none());
    assert_eq!(parent(&quiet), json!({"type":"url_text_input","value":""}));
    let mut s = setup(false);
    let ix = index(&s);
    assert_eq!(parent(&s)["value"], "https://Initial.test/Path");
    let expected = bridge::expected_visit(&s, binding(&s).visit).unwrap();
    assert!(
        expected["fields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["url"] == true && v["mount"].is_u64())
    );
    let leading = emit(&mut s, "input", "First.test/Path", true, true, false, false).unwrap();
    let first = cadence::take_leading(&mut s, &ix, leading).unwrap();
    assert!(cadence::take_leading(&mut s, &ix, leading).is_none());
    let trailing = emit(
        &mut s,
        "input",
        "Second.test/Path",
        true,
        true,
        false,
        false,
    )
    .unwrap();
    assert!(emit(&mut s, "input", "bad host", true, false, false, false).is_none());
    assert_eq!(parent(&s)["value"], "bad host");
    assert!(form(&s).unwrap().fields[&key()].error.is_some());
    assert!(form(&s).unwrap().native_error_text(&key()).is_none());
    assert!(
        super::super::ready(&s),
        "domain errors remain committable parent data"
    );
    let trailing = cadence::finish(&mut s, &ix, trailing).unwrap();
    assert_eq!(first.action.action()["value"], "First.test/Path");
    assert_eq!(trailing.action.action()["value"], "Second.test/Path");
    let sibling = InputKey {
        block_id: "sibling".into(),
        action_id: "text".into(),
    };
    s.app_home
        .forms
        .get_mut(&ix.0)
        .unwrap()
        .set(&sibling, InputValue::Text("latest sibling".into()))
        .unwrap();
    let view = s.app_home.surface.as_ref().unwrap().view.as_ref().unwrap();
    let request = trailing
        .action
        .prepare_with_context(
            &trailing.binding.owner.ctx.client,
            &trailing.binding.owner.ctx.session,
            view,
            form(&s).unwrap(),
            "fixture",
            trailing.route.team_id(),
        )
        .unwrap();
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    let fields = fields
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        serde_json::from_str::<Value>(&fields["actions"]).unwrap()[0]["value"],
        "Second.test/Path"
    );
    let payload = serde_json::from_str::<Value>(&fields["state"]).unwrap();
    assert_eq!(payload["values"]["url"]["value"]["value"], "bad host");
    assert_eq!(
        payload["values"]["sibling"]["text"]["value"],
        "latest sibling"
    );
    assert!(emit(&mut s, "enter", "", true, true, false, false).is_some());
    assert_eq!(parent(&s)["value"], "");
    assert!(emit(&mut s, "input", "", false, false, true, false).is_none());
    assert!(!super::super::ready(&s));
    assert!(form(&s).unwrap().state().is_err());
    assert_eq!(
        form(&s).unwrap().fields[&key()].value,
        InputValue::Url(Some(String::new()))
    );
    assert!(emit(&mut s, "input", "Resolved.test", true, true, false, false).is_some());
    assert!(super::super::ready(&s));
    assert_eq!(parent(&s)["value"], "Resolved.test");
    emit(&mut s, "composition", "unseen", false, false, false, true);
    assert!(!super::super::ready(&s));
    emit(
        &mut s,
        "input",
        "Composition.test",
        true,
        true,
        false,
        false,
    );
    assert!(super::super::ready(&s));
    let saved = parent(&s);
    emit(
        &mut s,
        "closed",
        "Composition.test",
        false,
        true,
        false,
        false,
    );
    mount(&mut s);
    assert_eq!(
        parent(&s),
        saved,
        "DOM remount must not replay the semantic initial callback"
    );
    let old_mount = s.app_home.text.mounts[&ix].id;
    refresh(&mut s, Some("https://NewInitial.test"));
    assert_eq!(parent(&s)["value"], "Composition.test");
    assert_eq!(s.app_home.text.urls[&ix].local, "Composition.test");
    assert_ne!(s.app_home.text.mounts[&ix].id, old_mount);
    assert!(!trailing.current(&s));
    crate::app_home::fixture::revisit(&mut s);
    hydrate(&mut s, &[]);
    mount(&mut s);
    assert_eq!(parent(&s)["value"], "https://NewInitial.test");
    refresh(&mut s, None);
    crate::app_home::fixture::revisit(&mut s);
    hydrate(&mut s, &[]);
    mount(&mut s);
    assert_eq!(s.app_home.text.urls[&ix].local, "");
    assert_eq!(
        parent(&s)["value"],
        "https://NewInitial.test",
        "fresh empty local does not clear its retained own parent"
    );
    let id = emit(&mut s, "enter", "", true, true, false, false).unwrap();
    let stale = cadence::take_leading(&mut s, &ix, id).unwrap();
    // Exercise the actual owned Reset producer in a mounted Dioxus scope.
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    let slot = Rc::new(Cell::new(None));
    let holder = Rc::new(RefCell::new(Some(s)));
    let mut dom = VirtualDom::new_with_props(
        |(holder, slot): (
            Rc<RefCell<Option<ShellState>>>,
            Rc<Cell<Option<Signal<ShellState>>>>,
        )| {
            let state = use_signal(|| holder.borrow_mut().take().unwrap());
            slot.set(Some(state));
            rsx! {div{}}
        },
        (holder, slot.clone()),
    );
    dom.rebuild_to_vec();
    let mut state = slot.get().unwrap();
    let visit = binding(&state.peek()).visit;
    dom.in_scope(ScopeId::ROOT, || super::super::super::reset(state, visit));
    assert!(!stale.current(&state.peek()));
    mount(&mut state.write());
    assert_eq!(parent(&state.peek()), json!({"type":"url_text_input"}));
    let before = state.peek().app_home.text.mounts[&ix].id;
    state.write().app_home.text.mounts.remove(&ix);
    assert!(
        bridge::expected_visit(&state.peek(), binding(&state.peek()).visit).is_none(),
        "a missing URL sibling cannot be silently omitted"
    );
    mount(&mut state.write());
    assert_ne!(state.peek().app_home.text.mounts[&ix].id, before);
}
