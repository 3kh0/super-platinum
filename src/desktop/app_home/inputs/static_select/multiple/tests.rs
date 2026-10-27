//! Serial-shell contracts; native timer/DOM execution is covered by named fixtures.
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
    let i = (b.form.clone(), b.key.clone());
    let values = values(&s.app_home.static_select.multiples[&i].visible);
    s.app_home.static_select.multi_mounts.insert(
        i,
        Mount {
            id: next(),
            seq: 1,
            values,
            binding: b,
        },
    );
}
fn setup(optional: bool) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([{"type":"input","block_id":"pick","dispatch_action":true,"optional":optional,"label":{"type":"plain_text","text":"Choices"},"element":{"type":"multi_static_select","action_id":"value","max_selected_items":2,"initial_options":[option("a")],"options":[option("a"),option("b"),option("c"),option("")]}}]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    super::super::super::hydrate(&mut s);
    mount(&mut s);
    assert!(super::ready(&s));
    s
}
fn emit(s: &mut ShellState, v: &[&str]) -> Option<u64> {
    let b = binding(s);
    let m = &s.app_home.static_select.multi_mounts[&(b.form.clone(), b.key.clone())];
    let e = Occurrence {
        mount: m.id,
        seq: m.seq + 1,
        kind: "change".into(),
        values: v.iter().map(|v| v.to_string()).collect(),
        trusted: false,
    };
    receive(s, &b, &e)
}
fn wire(s: &ShellState) -> Value {
    form(s).unwrap().state().unwrap()["values"]["pick"]["value"]["selected_options"].clone()
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
fn ordered_callbacks_are_trailing_only_and_empty_value_is_a_real_option() {
    let mut s = setup(true);
    let a = emit(&mut s, &["b", "a"]).unwrap();
    let i = (owner(&s).unwrap(), key());
    let w = &s.app_home.static_select.windows[&i];
    assert_eq!(w.id, a);
    assert_eq!(w.trailing.as_ref().unwrap().edge, "trailing");
    assert_eq!(wire(&s), json!([option("b"), option("a")]));
    let b = emit(&mut s, &["a", ""]).unwrap();
    assert_ne!(a, b);
    assert_eq!(wire(&s), json!([option("a"), option("")]));
    assert_eq!(
        s.app_home.static_select.windows[&i]
            .trailing
            .as_ref()
            .unwrap()
            .action
            .action()["selected_options"],
        wire(&s)
    );
}
#[test]
fn required_empty_and_overmax_commit_without_canceling_valid_queue() {
    for invalid in [vec![], vec!["a", "b", "c"]] {
        let mut s = setup(false);
        let id = emit(&mut s, &["b"]).unwrap();
        assert!(emit(&mut s, &invalid).is_none());
        let shown = proof(&s)["widgets"][0]["presentation"].clone();
        assert!(form(&s).unwrap().fields[&key()].error.is_some());
        assert_eq!(shown["parentError"], Value::Null);
        assert_eq!(shown["invalid"], invalid.len() > 2);
        assert_eq!(shown["text"], "You can select up to 2 items.");
        let i = (owner(&s).unwrap(), key());
        let q = s.app_home.static_select.windows[&i]
            .trailing
            .as_ref()
            .unwrap();
        assert_eq!(q.id, id);
        assert!(q.current(&s));
        assert!(form(&s).unwrap().fields[&key()].error.is_some());
        let h = s.app_home.surface.as_ref().unwrap();
        let req = q
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
        let RequestBody::Form(fields) = req.body else {
            panic!("form")
        };
        let fields: BTreeMap<_, _> = fields.into_iter().collect();
        let action: Value = serde_json::from_str(&fields["actions"]).unwrap();
        let state: Value = serde_json::from_str(&fields["state"]).unwrap();
        assert_eq!(action[0]["selected_options"], json!([option("b")]));
        assert_eq!(
            state["values"]["pick"]["value"]["selected_options"],
            wire(&s)
        );
    }
}
#[test]
fn optional_empty_dispatches_and_duplicate_or_unknown_callbacks_do_not_write() {
    let mut s = setup(true);
    assert!(emit(&mut s, &[]).is_some());
    assert_eq!(wire(&s), json!([]));
    for bad in [vec!["a", "a"], vec!["foreign"]] {
        assert!(emit(&mut s, &bad).is_none());
        assert_eq!(wire(&s), json!([]));
    }
}
#[test]
fn initial_only_changes_display_options_only_and_max_only_preserve_it() {
    let mut s = setup(true);
    emit(&mut s, &["b"]);
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]["initial_options"] = json!([])
    });
    let i = (owner(&s).unwrap(), key());
    assert!(s.app_home.static_select.multiples[&i].visible.is_empty());
    assert_eq!(wire(&s), json!([option("b")]));
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_options");
        r["blocks"][0]["element"]["max_selected_items"] = json!(1);
    });
    assert!(s.app_home.static_select.multiples[&i].visible.is_empty());
    assert_eq!(wire(&s), json!([option("b")]));
}
#[test]
fn removed_option_keeps_local_token_and_invalid_flag_survives_max_raise() {
    let mut s = setup(true);
    emit(&mut s, &["a", "b", "c"]);
    let i = (owner(&s).unwrap(), key());
    assert!(s.app_home.static_select.multiples[&i].invalid);
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]["max_selected_items"] = json!(4)
    });
    assert!(s.app_home.static_select.multiples[&i].invalid);
    assert_eq!(proof(&s)["widgets"][0]["presentation"]["invalid"], false);
    refresh(&mut s, |r| {
        r["blocks"][0]["element"]["options"] = json!([option("a"), option("c"), option("")])
    });
    assert_eq!(
        values(&s.app_home.static_select.multiples[&i].visible),
        vec!["a", "b", "c"]
    );
    crate::app_home::fixture::revisit(&mut s);
    hydrate(&mut s);
    assert_eq!(
        values(&s.app_home.static_select.multiples[&i].visible),
        vec!["a", "b", "c"]
    );
    assert_eq!(
        s.app_home.static_select.multiples[&i].visible[1]["text"]["text"],
        "Choice b"
    );
}
#[test]
fn revisit_uses_current_dirty_parent_order_and_saved_parent_without_replaying() {
    let mut s = setup(true);
    emit(&mut s, &["b", "a"]);
    let i = (owner(&s).unwrap(), key());
    crate::app_home::fixture::revisit(&mut s);
    hydrate(&mut s);
    mount(&mut s);
    assert_eq!(
        values(&s.app_home.static_select.multiples[&i].visible),
        vec!["b", "a"]
    );
    poll(&mut s);
    super::super::poll(&mut s);
    assert!(s.app_home.static_select.windows.is_empty());
    let mut s = setup(true);
    s.app_home.forms.clear();
    refresh(
        &mut s,
        |r| r["state"] = json!({"values":{"pick":{"value":{"type":"multi_static_select","selected_options":[option("c"),option("a")]}}}}),
    );
    let i = (owner(&s).unwrap(), key());
    crate::app_home::fixture::revisit(&mut s);
    hydrate(&mut s);
    assert_eq!(
        values(&s.app_home.static_select.multiples[&i].visible),
        vec!["c", "a"]
    );
    assert!(s.app_home.static_select.windows.is_empty());
}
#[test]
fn changed_owner_source_and_reset_retire_occurrence_authority() {
    let mut s = setup(true);
    emit(&mut s, &["b"]);
    let i = (owner(&s).unwrap(), key());
    let q = s.app_home.static_select.windows[&i]
        .trailing
        .as_ref()
        .unwrap()
        .clone();
    s.app_home.surface.as_mut().unwrap().revision = next();
    assert!(!q.current(&s));
    super::super::reset(&mut s, &i.0);
    assert!(s.app_home.static_select.multiples.is_empty());
    assert!(s.app_home.static_select.multi_mounts.is_empty());
    assert!(s.app_home.static_select.windows.is_empty());
}
