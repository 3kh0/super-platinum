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
    let values = values(&s.app_home.static_select.checks[&i].selection);
    s.app_home.static_select.check_mounts.insert(
        i,
        Mount {
            id: next(),
            seq: 1,
            values,
            binding: b,
        },
    );
}
fn setup(optional: bool, quiet: bool) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([{"type":"input","block_id":"pick","dispatch_action":!quiet,"optional":optional,"label":{"type":"plain_text","text":"Choices"},"element":{"type":"checkboxes","action_id":"value","initial_options":[option("a")],"options":[option("a"),option("b"),option("")]}}]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    super::super::super::hydrate(&mut s);
    mount(&mut s);
    assert!(super::ready(&s));
    s
}
fn occurrence(s: &ShellState, index: usize, checked: bool) -> Occurrence {
    let m = &s.app_home.static_select.check_mounts[&(owner(s).unwrap(), key())];
    let InputKind::Checkboxes { options } = &form(s).unwrap().fields[&key()].kind else {
        panic!("checkbox")
    };
    let mut values = m.values.clone();
    if let Some(option) = options.get(index) {
        if checked {
            values.push(option.value.clone())
        } else {
            values.retain(|v| v != &option.value)
        }
    }
    Occurrence {
        mount: m.id,
        seq: m.seq + 1,
        kind: "change".into(),
        index: Some(index),
        checked: Some(checked),
        values,
        trusted: false,
    }
}
fn emit(s: &mut ShellState, index: usize, checked: bool) -> Option<(u64, Option<Queued>)> {
    let b = binding(s);
    let e = occurrence(s, index, checked);
    receive(s, &b, &e)
}
fn wire(s: &ShellState) -> Value {
    form(s).unwrap().state().unwrap()["values"]["pick"]["value"]["selected_options"].clone()
}
fn visible(s: &ShellState) -> Vec<Value> {
    s.app_home.static_select.checks[&(owner(s).unwrap(), key())]
        .selection
        .selected()
        .to_vec()
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
#[test]
fn ordered_toggle_has_one_leading_then_latest_trailing_and_empty_is_real() {
    let mut s = setup(true, false);
    let (first, leading) = emit(&mut s, 1, true).unwrap();
    assert_eq!(leading.unwrap().edge, "leading");
    let (second, leading) = emit(&mut s, 0, false).unwrap();
    assert!(leading.is_none());
    assert_ne!(first, second);
    emit(&mut s, 0, true);
    emit(&mut s, 2, true);
    assert_eq!(wire(&s), json!([option("b"), option("a"), option("")]));
    let w = &s.app_home.static_select.windows[&(owner(&s).unwrap(), key())];
    assert_eq!(w.trailing.as_ref().unwrap().edge, "trailing");
    assert_eq!(
        w.trailing.as_ref().unwrap().action.action()["selected_options"],
        wire(&s)
    );
}
#[test]
fn required_empty_commits_without_canceling_valid_trailing_action() {
    let mut s = setup(false, false);
    emit(&mut s, 1, true);
    emit(&mut s, 0, false);
    let i = (owner(&s).unwrap(), key());
    let q = s.app_home.static_select.windows[&i]
        .trailing
        .as_ref()
        .unwrap()
        .clone();
    assert!(emit(&mut s, 1, false).is_none());
    assert_eq!(wire(&s), json!([]));
    assert!(form(&s).unwrap().fields[&key()].error.is_some());
    assert_eq!(
        s.app_home.static_select.windows[&i]
            .trailing
            .as_ref()
            .unwrap()
            .id,
        q.id
    );
    assert!(q.current(&s));
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
    assert_eq!(actions[0]["selected_options"], json!([option("b")]));
    assert_eq!(
        state["values"]["pick"]["value"]["selected_options"],
        json!([])
    );
}
#[test]
fn quiet_edits_have_identical_order_and_raw_survivors_without_queue() {
    let mut s = setup(true, true);
    assert!(emit(&mut s, 1, true).is_none());
    assert!(emit(&mut s, 0, false).is_none());
    assert_eq!(wire(&s), json!([option("b")]));
    assert!(s.app_home.static_select.windows.is_empty());
    assert!(emit(&mut s, 1, false).is_none());
    assert_eq!(wire(&s), json!([]));
    assert!(ready(&s));
}
#[test]
fn metadata_refresh_and_hidden_rows_do_not_rewrite_owned_survivors() {
    let mut s = setup(true, false);
    emit(&mut s, 1, true);
    emit(&mut s, 0, false);
    let mut new_b = option("b");
    new_b["text"]["text"] = json!("New Beta");
    new_b["opaque"]["version"] = json!(2);
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]["initial_options"] = json!([]);
        r["blocks"][0]["element"]["options"] = json!([option("a"), new_b.clone(), option("")]);
    });
    assert_eq!(visible(&s), vec![option("b")]);
    emit(&mut s, 0, true);
    assert_eq!(wire(&s), json!([option("b"), option("a")]));
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]["options"] = json!([option("a"), option("")]);
    });
    emit(&mut s, 0, false);
    assert_eq!(wire(&s), json!([option("b")]));
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]["options"] = json!([option("a"), new_b.clone(), option("")]);
    });
    emit(&mut s, 1, false);
    emit(&mut s, 1, true);
    assert_eq!(wire(&s), json!([new_b]));
}
#[test]
fn initial_and_saved_props_leave_mounted_widget_independent_of_parent() {
    let mut s = setup(true, true);
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]["initial_options"] = json!([option("b")]);
        r["state"] = json!({"values":{"pick":{"value":{"type":"checkboxes","selected_options":[option("b")]}}}});
    });
    assert_eq!(visible(&s), vec![option("a")]);
    assert_eq!(wire(&s), json!([option("b")]));
    emit(&mut s, 1, true);
    assert_eq!(wire(&s), json!([option("a"), option("b")]));
}
#[test]
fn forged_sequence_array_index_and_stale_source_cannot_commit() {
    for corrupt in 0..5 {
        let mut s = setup(true, false);
        let b = binding(&s);
        let mut e = occurrence(&s, 1, true);
        match corrupt {
            0 => e.seq += 1,
            1 => e.mount += 1,
            2 => e.values.reverse(),
            3 => e.index = Some(999),
            _ => s.app_home.surface.as_mut().unwrap().revision = next(),
        }
        assert!(receive(&mut s, &b, &e).is_none());
        assert_eq!(wire(&s), json!([option("a")]));
    }
}
#[test]
fn revisit_and_reset_retire_old_mounts_without_replaying_callbacks() {
    let mut s = setup(true, false);
    let q = emit(&mut s, 1, true).unwrap().1.unwrap();
    let o = owner(&s).unwrap();
    crate::app_home::fixture::revisit(&mut s);
    hydrate(&mut s);
    mount(&mut s);
    super::super::poll(&mut s);
    assert_eq!(visible(&s), vec![option("a"), option("b")]);
    assert!(!q.current(&s));
    assert!(s.app_home.static_select.windows.is_empty());
    super::super::reset(&mut s, &o);
    assert!(s.app_home.static_select.checks.is_empty());
    assert!(s.app_home.static_select.check_mounts.is_empty());
}
#[test]
fn quiet_checkbox_button_custody_requires_current_acknowledged_mount() {
    let mut s = setup(true, true);
    let b = binding(&s);
    assert!(super::super::bridge::expected_visit(&s, b.visit).is_some());
    let i = (owner(&s).unwrap(), key());
    s.app_home
        .static_select
        .check_mounts
        .get_mut(&i)
        .unwrap()
        .seq = 0;
    assert!(super::super::bridge::expected_visit(&s, b.visit).is_none());
}
