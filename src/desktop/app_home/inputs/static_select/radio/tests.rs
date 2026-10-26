//! Serial ownership/state tests; native fixtures exercise actual callback timing.
use super::*;
use super_platinum_core::slack::client::RequestBody;
fn key() -> InputKey {
    InputKey {
        block_id: "pick".into(),
        action_id: "value".into(),
    }
}
fn option(v: &str) -> Value {
    json!({"text":{"type":"plain_text","text":format!("Choice {v}")},"value":v,"opaque":{"keep":v}})
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
fn mount(s: &mut ShellState) {
    let b = binding(s);
    let i = (b.form.clone(), key());
    let value = s.app_home.static_select.radios[&i].visible.clone();
    s.app_home.static_select.radio_mounts.insert(
        i,
        Mount {
            id: next(),
            seq: 1,
            value,
            binding: b,
        },
    );
}
fn setup(optional: bool, quiet: bool) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([{"type":"input","block_id":"pick","dispatch_action":!quiet,"optional":optional,"label":{"type":"plain_text","text":"Choices"},"element":{"type":"radio_buttons","action_id":"value","initial_option":option("a"),"options":[option("a"),option("b"),option("")]}}]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    super::super::super::hydrate(&mut s);
    mount(&mut s);
    assert!(super::ready(&s));
    s
}
fn occurrence(s: &ShellState, index: usize) -> Occurrence {
    let m = &s.app_home.static_select.radio_mounts[&(owner(s).unwrap(), key())];
    let InputKind::Radio { options } = &form(s).unwrap().fields[&key()].kind else {
        panic!("radio")
    };
    Occurrence {
        mount: m.id,
        seq: m.seq + 1,
        kind: "change".into(),
        index: Some(index),
        value: options.get(index).map(|o| o.value.clone()),
        trusted: false,
    }
}
fn emit(s: &mut ShellState, index: usize) -> Option<(u64, Option<Queued>)> {
    let b = binding(s);
    let e = occurrence(s, index);
    receive(s, &b, &e)
}
fn wire(s: &ShellState) -> Value {
    form(s).unwrap().state().unwrap()["values"]["pick"]["value"]["selected_option"].clone()
}
fn visible(s: &ShellState) -> Option<String> {
    s.app_home.static_select.radios[&(owner(s).unwrap(), key())]
        .visible
        .clone()
}
fn refresh(s: &mut ShellState, edit: impl FnOnce(&mut Value)) {
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    edit(&mut raw);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    super::super::super::hydrate(s);
    mount(s);
}
fn revisit(s: &mut ShellState) {
    let owner = Owner::capture(s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = owner.home_visit;
    h.owner = owner;
    hydrate(s);
    mount(s);
}
#[test]
fn native_rows_have_leading_then_latest_trailing_and_empty_value_is_real() {
    let mut s = setup(false, false);
    let (first, q) = emit(&mut s, 1).unwrap();
    assert_eq!(q.unwrap().edge, "leading");
    let (second, q) = emit(&mut s, 0).unwrap();
    assert!(q.is_none());
    assert_ne!(first, second);
    emit(&mut s, 2);
    assert_eq!(wire(&s), option(""));
    assert_eq!(visible(&s), Some("".into()));
    assert!(form(&s).unwrap().fields[&key()].error.is_none());
    let w = &s.app_home.static_select.windows[&(owner(&s).unwrap(), key())];
    assert_eq!(
        w.trailing.as_ref().unwrap().action.action()["selected_option"],
        option("")
    );
}
#[test]
fn same_selected_native_change_and_absent_clear_do_not_commit_or_queue() {
    let mut s = setup(true, false);
    let before = form(&s).unwrap().clone();
    assert!(emit(&mut s, 0).is_none());
    let b = binding(&s);
    let mut e = occurrence(&s, 1);
    e.index = None;
    e.value = None;
    assert!(receive(&mut s, &b, &e).is_none());
    assert_eq!(form(&s).unwrap(), &before);
    assert!(s.app_home.static_select.windows.is_empty());
}
#[test]
fn quiet_radio_commits_current_raw_and_participates_in_button_custody() {
    let mut s = setup(true, true);
    assert!(emit(&mut s, 1).is_none());
    assert_eq!(wire(&s), option("b"));
    assert!(s.app_home.static_select.windows.is_empty());
    let b = binding(&s);
    let expected = super::super::bridge::expected_visit(&s, b.visit).unwrap();
    assert_eq!(expected["fields"][0]["radio"], true);
    assert_eq!(expected["fields"][0]["value"], "b");
    let i = (owner(&s).unwrap(), key());
    s.app_home
        .static_select
        .radio_mounts
        .get_mut(&i)
        .unwrap()
        .seq = 0;
    assert!(super::super::bridge::expected_visit(&s, b.visit).is_none());
}
#[test]
fn props_keep_visible_value_and_hidden_committed_raw_until_current_row_callback() {
    let mut s = setup(true, false);
    emit(&mut s, 1);
    let mut new_b = option("b");
    new_b["text"]["text"] = json!("New Beta");
    new_b["opaque"]["version"] = json!(2);
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]["options"] = json!([option("a"), new_b.clone(), option("")]);
    });
    assert_eq!(visible(&s), Some("b".into()));
    assert_eq!(wire(&s), option("b"));
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]["options"] = json!([option("a"), option("")]);
    });
    assert_eq!(visible(&s), Some("b".into()));
    assert_eq!(wire(&s), option("b"));
    assert!(ready(&s));
    emit(&mut s, 0);
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]["options"] = json!([option("a"), new_b.clone(), option("")]);
    });
    emit(&mut s, 1);
    assert_eq!(wire(&s), new_b);
}
#[test]
fn changed_initial_and_saved_parent_do_not_reset_mounted_value() {
    let mut s = setup(true, true);
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]["initial_option"] = option("b");
        r["state"] = json!({"values":{"pick":{"value":{"type":"radio_buttons","selected_option":option("b")}}}});
    });
    assert_eq!(visible(&s), Some("a".into()));
    assert_eq!(wire(&s), option("b"));
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_option");
    });
    assert_eq!(visible(&s), Some("a".into()));
}
#[test]
fn new_visit_uses_nonempty_parent_or_source_initial_including_empty_initial() {
    for selected in [None, Some(2)] {
        let mut s = setup(true, false);
        let o = owner(&s).unwrap();
        s.app_home
            .forms
            .get_mut(&o)
            .unwrap()
            .radio_input(&key(), selected)
            .unwrap();
        revisit(&mut s);
        assert_eq!(visible(&s), Some("a".into()));
        assert_eq!(
            wire(&s),
            if selected.is_some() {
                option("")
            } else {
                Value::Null
            }
        );
    }
    let mut s = setup(true, true);
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]["initial_option"] = option("");
        r["state"] =
            json!({"values":{"pick":{"value":{"type":"radio_buttons","selected_option":null}}}});
    });
    revisit(&mut s);
    assert_eq!(visible(&s), Some("".into()));
    assert_eq!(wire(&s), Value::Null);
}
#[test]
fn invalid_direct_parent_clear_keeps_queued_valid_action_with_latest_null_state() {
    let mut s = setup(false, false);
    emit(&mut s, 1);
    emit(&mut s, 0);
    let i = (owner(&s).unwrap(), key());
    let q = s.app_home.static_select.windows[&i]
        .trailing
        .as_ref()
        .unwrap()
        .clone();
    assert!(
        s.app_home
            .forms
            .get_mut(&i.0)
            .unwrap()
            .radio_input(&key(), None)
            .unwrap()
            .is_none()
    );
    assert_eq!(wire(&s), Value::Null);
    assert_eq!(
        s.app_home.static_select.windows[&i]
            .trailing
            .as_ref()
            .unwrap()
            .id,
        q.id
    );
    let h = s.app_home.surface.as_ref().unwrap();
    let request = q
        .action
        .prepare_with_context(
            &q.binding.owner.ctx.client,
            &q.binding.owner.ctx.session,
            h.view.as_ref().unwrap(),
            form(&s).unwrap(),
            "synthetic",
            q.route.team_id(),
        )
        .unwrap();
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    let fields: BTreeMap<_, _> = fields.into_iter().collect();
    let actions: Value = serde_json::from_str(&fields["actions"]).unwrap();
    let state: Value = serde_json::from_str(&fields["state"]).unwrap();
    assert_eq!(actions[0]["selected_option"], option("a"));
    assert_eq!(
        state["values"]["pick"]["value"]["selected_option"],
        Value::Null
    );
}
#[test]
fn forged_native_occurrence_and_retired_owner_refuse_atomically() {
    for corrupt in 0..7 {
        let mut s = setup(true, false);
        let b = binding(&s);
        let mut e = occurrence(&s, 1);
        match corrupt {
            0 => e.seq += 1,
            1 => e.mount += 1,
            2 => e.value = Some("foreign".into()),
            3 => e.index = Some(999),
            4 => s.app_home.surface.as_mut().unwrap().revision = next(),
            5 => {
                s.core
                    .workspaces
                    .get_mut(&b.owner.ctx.session.team_id)
                    .unwrap()
                    .rt_generation += 1
            }
            _ => s.core.account_epoch += 1,
        };
        assert!(receive(&mut s, &b, &e).is_none());
        assert_eq!(
            s.app_home.forms[&b.form].state().unwrap()["values"]["pick"]["value"]["selected_option"],
            option("a")
        );
    }
}
#[test]
fn revisit_and_reset_retire_old_occurrences_and_clear_owned_state() {
    let mut s = setup(true, false);
    let q = emit(&mut s, 1).unwrap().1.unwrap();
    let o = owner(&s).unwrap();
    revisit(&mut s);
    super::super::poll(&mut s);
    assert_eq!(visible(&s), Some("b".into()));
    assert!(!q.current(&s));
    assert!(s.app_home.static_select.windows.is_empty());
    super::super::reset(&mut s, &o);
    assert!(s.app_home.static_select.radios.is_empty());
    assert!(s.app_home.static_select.radio_mounts.is_empty());
}

#[test]
fn callback_validation_does_not_supply_home_parent_feedback() {
    let mut s = setup(false, true);
    s.app_home.forms.clear();
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_option");
    });
    revisit(&mut s);
    assert!(!form(&s).unwrap().fields[&key()].dirty);
    assert!(form(&s).unwrap().fields[&key()].error.is_some());
    assert_eq!(visible(&s), None);
    assert_eq!(presentation_error(&s, &key()), None);
    let o = owner(&s).unwrap();
    s.app_home
        .forms
        .get_mut(&o)
        .unwrap()
        .radio_input(&key(), None)
        .unwrap();
    assert!(form(&s).unwrap().fields[&key()].dirty);
    assert!(form(&s).unwrap().fields[&key()].error.is_some());
    assert_eq!(presentation_error(&s, &key()), None);
    // A malformed or missing-row occurrence does not validate this mounted input.
    assert!(emit(&mut s, 999).is_none());
    assert_eq!(presentation_error(&s, &key()), None);
    assert!(emit(&mut s, 1).is_none()); // Quiet callbacks still validate locally.
    assert!(s.app_home.static_select.radios[&(o.clone(), key())].validated);
    s.app_home
        .forms
        .get_mut(&o)
        .unwrap()
        .radio_input(&key(), None)
        .unwrap();
    assert!(form(&s).unwrap().fields[&key()].error.is_some());
    assert_eq!(presentation_error(&s, &key()), None);
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]["initial_option"] = option("b");
    });
    assert!(s.app_home.static_select.radios[&(o.clone(), key())].validated);
    assert!(form(&s).unwrap().fields[&key()].error.is_some());
    assert_eq!(presentation_error(&s, &key()), None);
    revisit(&mut s);
    assert!(form(&s).unwrap().fields[&key()].dirty);
    assert!(form(&s).unwrap().fields[&key()].error.is_some());
    assert_eq!(presentation_error(&s, &key()), None);
    assert!(!s.app_home.static_select.radios[&(o, key())].validated);
}
