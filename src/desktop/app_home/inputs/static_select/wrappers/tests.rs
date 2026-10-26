//! Wrapper source and serial-state regressions. Browser events, leases and
//! response timing remain native-fixture proof, not claims of this suite.
use super::*;
use super_platinum_core::block_actions::BlockConfirmationState;

fn key(block: &str, action: &str) -> InputKey {
    fixture::key(block, action)
}
fn setup() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s);
    s
}
fn metadata(block: &str, action: &str) -> BlockElementMetadata {
    BlockElementMetadata {
        source: BlockElementSource::UnsupportedControl {
            kind: "static_select".into(),
        },
        block_id: Some(block.into()),
        action_id: Some(action.into()),
        value: None,
        confirm: BlockConfirmationState::Missing,
        accessibility_label: None,
    }
}
fn binding(s: &ShellState, key: InputKey) -> Binding {
    let h = s.app_home.surface.as_ref().unwrap();
    Binding {
        owner: h.owner.clone(),
        form: owner(s).unwrap(),
        visit: actions::Key {
            visit: h.id,
            revision: h.revision,
        },
        key,
        raw: h.view.as_ref().unwrap().raw().clone(),
    }
}
fn mount(s: &mut ShellState, key: InputKey) -> Binding {
    let b = binding(s, key);
    let index = (b.form.clone(), b.key.clone());
    let value = s.app_home.static_select.widgets[&index].visible.clone();
    s.app_home.static_select.mounts.insert(
        index,
        Mount {
            id: next(),
            seq: 1,
            value,
            binding: b.clone(),
        },
    );
    b
}
fn emit(s: &mut ShellState, b: &Binding, index: Option<usize>) -> Option<(u64, Option<Queued>)> {
    let m = &s.app_home.static_select.mounts[&(b.form.clone(), b.key.clone())];
    let e = Occurrence {
        mount: m.id,
        seq: m.seq + 1,
        kind: "change".into(),
        index,
        trusted: false,
    };
    receive(s, b, &e)
}
fn refresh(s: &mut ShellState, change: impl FnOnce(&mut Value)) {
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    change(&mut raw);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
}
fn button(s: &ShellState) -> super_platinum_core::block_actions::home::Button {
    let h = s.app_home.surface.as_ref().unwrap();
    h.view
        .as_ref()
        .unwrap()
        .button_with_form(
            "controls",
            "send",
            &h.owner.ctx.session.user_id,
            form(s).unwrap(),
        )
        .unwrap()
}

#[test]
fn inline_admission_uses_current_exact_actions_or_accessory_source() {
    let s = setup();
    for (block, action) in [
        ("controls", "first"),
        ("controls", "second"),
        ("summary", "accessory"),
    ] {
        assert_eq!(
            inline_key(&s, &metadata(block, action)),
            Some(key(block, action))
        );
    }
    // The input has its existing renderer; Button and unrelated keys cannot
    // borrow the wrapper renderer merely by naming this Home view.
    assert!(inline_key(&s, &metadata("pick", "value")).is_none());
    assert!(inline_key(&s, &metadata("controls", "send")).is_none());
    assert!(inline_key(&s, &metadata("other", "first")).is_none());
    let mut wrong = metadata("controls", "first");
    wrong.source = BlockElementSource::Button;
    assert!(inline_key(&s, &wrong).is_none());
    wrong.source = BlockElementSource::UnsupportedControl {
        kind: "multi_static_select".into(),
    };
    assert!(inline_key(&s, &wrong).is_none());
}

#[test]
fn removed_dirty_wrapper_is_not_marked_shown_or_rendered_as_source_present() {
    let mut s = setup();
    let b = mount(&mut s, key("controls", "first"));
    emit(&mut s, &b, Some(1)).unwrap();
    let raw = form(&s).unwrap().raw_view().clone();
    assert_eq!(shown_keys(&s, &raw["blocks"][3]).len(), 2);
    assert_eq!(
        shown_keys(&s, &raw["blocks"][4]),
        vec![key("summary", "accessory")]
    );
    assert!(shown_keys(&s, &raw["blocks"][0]).is_empty());
    refresh(&mut s, |raw| {
        raw["blocks"][3]["elements"]
            .as_array_mut()
            .unwrap()
            .remove(0);
    });
    let f = form(&s).unwrap();
    assert_eq!(
        f.fields[&b.key].value,
        InputValue::Selected(vec!["beta".into()])
    );
    assert!(f.static_field(&b.key).is_err());
    assert!(inline_key(&s, &metadata("controls", "first")).is_none());
    assert_eq!(
        shown_keys(&s, &f.raw_view()["blocks"][3]),
        vec![key("controls", "second")]
    );
    assert!(!b.current(&s));
}

#[test]
fn wrapper_optional_validation_does_not_create_a_clear_affordance() {
    for (block, action) in [("controls", "first"), ("summary", "accessory")] {
        let mut s = setup();
        let b = mount(&mut s, key(block, action));
        let f = form(&s).unwrap();
        assert!(f.static_field(&b.key).unwrap().optional);
        assert!(!f.static_allows_clear(&b.key).unwrap());
        assert!(emit(&mut s, &b, None).is_none());
        assert!(s.app_home.static_select.windows.is_empty());
        assert_eq!(
            form(&s).unwrap().fields[&b.key].value,
            InputValue::Selected(vec!["alpha".into()])
        );
        // A real source option with empty value is different from an invented
        // clear affordance: z3Rn resolves it to null and Te7m emits that state.
        let (_, q) = emit(&mut s, &b, Some(3)).unwrap();
        let q = q.unwrap();
        assert_eq!(q.action.action()["selected_option"], Value::Null);
        assert_eq!(
            form(&s).unwrap().state().unwrap()["values"][block][action]["selected_option"],
            Value::Null
        );
        assert!(q.current(&s));
        assert!(form(&s).unwrap().fields[&b.key].error.is_none());
    }
    let mut s = setup();
    let b = mount(&mut s, key("pick", "value"));
    assert!(form(&s).unwrap().static_allows_clear(&b.key).unwrap());
    assert!(emit(&mut s, &b, None).unwrap().1.is_some());
}

#[test]
fn repeated_wrapper_callbacks_keep_exact_option_and_independent_action_windows() {
    let mut s = setup();
    let first = mount(&mut s, key("controls", "first"));
    let second = mount(&mut s, key("controls", "second"));
    let (_, a) = emit(&mut s, &first, Some(1)).unwrap();
    let a = a.unwrap();
    assert_eq!(
        a.action.action()["selected_option"],
        super::super::fixture::option("beta", "Beta")
    );
    assert!(emit(&mut s, &first, Some(1)).unwrap().1.is_none());
    assert_eq!(
        s.app_home.static_select.windows[&(first.form.clone(), first.key.clone())]
            .trailing
            .as_ref()
            .unwrap()
            .action
            .action()["selected_option"],
        a.action.action()["selected_option"]
    );
    assert_eq!(
        emit(&mut s, &second, Some(2)).unwrap().1.unwrap().edge,
        "leading"
    );
    assert_eq!(s.app_home.static_select.windows.len(), 2);
    let old_mount = a.mount;
    mount(&mut s, first.key.clone());
    assert_ne!(
        s.app_home.static_select.mounts[&(first.form.clone(), first.key.clone())].id,
        old_mount
    );
    assert!(!a.current(&s));
}

#[test]
fn same_block_loading_is_a_set_and_first_completion_can_clear_later_work() {
    let mut s = setup();
    let route = context::Witness::capture(&s).unwrap();
    let button = button(&s);
    let first = button_started(&mut s, &button, &route).unwrap();
    assert!(loading(&s, "controls"));
    assert!(!loading(&s, "summary"));
    assert!(!loading(&s, "pick"));
    let later = button_started(&mut s, &button, &route).unwrap();
    assert_eq!(s.app_home.static_select.activity.len(), 1);
    button_finished(&mut s, first, &route, Some("first request failed".into()));
    assert!(!loading(&s, "controls"));
    assert_eq!(
        status(&s, "controls").as_deref(),
        Some("first request failed")
    );
    // The later invocation is still current; its completion clears the error.
    assert!(later.current(&s));
    button_finished(&mut s, later, &route, None);
    assert!(!loading(&s, "controls"));
    assert_eq!(status(&s, "controls"), None);
    // This tests the shared serial Set only. Button click admission and its
    // independent one-second gate have their own actions regression suite.
}

#[test]
fn stale_button_completion_cannot_clear_replacement_source_activity() {
    let mut s = setup();
    let route = context::Witness::capture(&s).unwrap();
    let original_button = button(&s);
    let old = button_started(&mut s, &original_button, &route).unwrap();
    refresh(&mut s, |raw| raw["hash"] = json!("replacement"));
    let new_route = context::Witness::capture(&s).unwrap();
    let replacement = button(&s);
    let current = button_started(&mut s, &replacement, &new_route).unwrap();
    button_finished(&mut s, old, &route, Some("stale error".into()));
    assert!(loading(&s, "controls"));
    let entry = &s.app_home.static_select.activity[&(current.form.clone(), "controls".into())];
    assert!(entry.error.is_none());
    assert_eq!(entry.binding.raw, current.raw);
    s.core.account_epoch += 1;
    button_finished(
        &mut s,
        current.clone(),
        &new_route,
        Some("retired account".into()),
    );
    assert!(!loading(&s, "controls"));
    assert_eq!(status(&s, "controls"), None);
    // The inert old entry is not mutated after account retirement.
    assert!(s.app_home.static_select.activity[&(current.form, "controls".into())].loading);
}
