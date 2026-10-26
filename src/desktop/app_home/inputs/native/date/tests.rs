use super::*;
fn key() -> InputKey {
    InputKey {
        block_id: "day".into(),
        action_id: "value".into(),
    }
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
fn setup(optional: bool, quiet: bool) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([{"type":"input","block_id":"day","dispatch_action":!quiet,"optional":optional,"label":{"type":"plain_text","text":"Date"},"element":{"type":"datepicker","action_id":"value","initial_date":"2026-10-25"}}]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    crate::app_home::inputs::hydrate(&mut s);
    mount(&mut s);
    s
}
fn mount(s: &mut ShellState) {
    let b = binding(s);
    let (value, _) = configure(s, &b).unwrap();
    let value = value.unwrap_or_default();
    let mount = next();
    s.app_home.native.entries.insert(
        (b.form.clone(), key()),
        Entry {
            id: next(),
            mount,
            seq: 0,
            pending: true,
            invalid: false,
            value: value.clone(),
        },
    );
    let e = occurrence(mount, 1, "mounted", &value, true);
    assert!(receive(s, &b, mount, &e).is_none());
}
fn emit(s: &mut ShellState, kind: &str, value: &str, valid: bool) -> Option<StaticEvent> {
    let b = binding(s);
    let e = &s.app_home.native.entries[&(b.form.clone(), key())];
    let event = occurrence(e.mount, e.seq + 1, kind, value, valid);
    receive(s, &b, event.mount, &event)
}
fn wire(s: &ShellState) -> Value {
    form(s).unwrap().state().unwrap()["values"]["day"]["value"]["selected_date"].clone()
}
fn visible(s: &ShellState) -> Option<String> {
    s.app_home.native.dates[&(owner(s).unwrap(), key())]
        .visible
        .clone()
}
fn refresh(s: &mut ShellState, edit: impl FnOnce(&mut Value)) {
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    edit(&mut raw);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    crate::app_home::inputs::hydrate(s);
}
#[test]
fn observation_is_not_parent_commit_and_same_date_does_not_change_input_revision() {
    let mut s = setup(true, false);
    let revision = crate::app_home::inputs::revision(&s);
    assert!(emit(&mut s, "input", "2026-11-01", true).is_none());
    assert_eq!(wire(&s), "2026-10-25");
    assert!(crate::app_home::inputs::ready(&s));
    assert_eq!(crate::app_home::inputs::revision(&s), revision);
    let action = emit(&mut s, "commit", "2026-11-01", true).unwrap();
    assert_eq!(action.action()["selected_date"], "2026-11-01");
    assert_eq!(wire(&s), "2026-11-01");
    let revision = crate::app_home::inputs::revision(&s);
    assert!(emit(&mut s, "commit", "2026-11-01", true).is_none());
    assert_eq!(crate::app_home::inputs::revision(&s), revision);
}
#[test]
fn pristine_date_removal_and_readdition_retires_and_reseeds_the_widget() {
    let mut s = setup(true, false);
    let original = form(&s).unwrap().raw_view()["blocks"][0].clone();
    let old_focus = crate::app_home::inputs::focus::proof(&s);
    assert!(!form(&s).unwrap().dirty());
    refresh(&mut s, |raw| raw["blocks"] = json!([]));
    assert!(form(&s).unwrap().fields.is_empty());
    assert!(s.app_home.native.dates.is_empty());
    assert!(
        crate::app_home::inputs::focus::proof(&s)["focusWidgets"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    refresh(&mut s, |raw| raw["blocks"] = json!([original]));
    mount(&mut s);
    assert_eq!(wire(&s), "2026-10-25");
    assert!(crate::app_home::inputs::ready(&s));
    assert_ne!(
        old_focus["focusWidgets"][0]["instance"],
        crate::app_home::inputs::focus::proof(&s)["focusWidgets"][0]["instance"]
    );
}
#[test]
fn committed_dirty_date_removal_still_refuses_atomically() {
    let mut s = setup(true, false);
    emit(&mut s, "commit", "2026-11-01", true).unwrap();
    let before = form(&s).unwrap().raw_view().clone();
    let state = form(&s).unwrap().state().unwrap();
    refresh(&mut s, |raw| raw["blocks"] = json!([]));
    assert_eq!(form(&s).unwrap().raw_view(), &before);
    assert_eq!(form(&s).unwrap().state().unwrap(), state);
    assert_eq!(visible(&s).as_deref(), Some("2026-11-01"));
    assert!(!crate::app_home::inputs::ready(&s));
}
#[test]
fn unsettled_or_independently_edited_native_pair_keeps_removal_protected() {
    for mode in ["pending", "invalid", "aggregate", "picker"] {
        let mut s = setup(true, false);
        let index = (owner(&s).unwrap(), key());
        match mode {
            "pending" => s.app_home.native.entries.get_mut(&index).unwrap().pending = true,
            "invalid" => s.app_home.native.entries.get_mut(&index).unwrap().invalid = true,
            "aggregate" => {
                s.app_home.native.entries.get_mut(&index).unwrap().value =
                    json!(["unseen", "2026-10-25"]).to_string()
            }
            "picker" => {
                let w = s.app_home.native.dates.get_mut(&index).unwrap();
                w.picker = "2026-11-03".into();
                s.app_home.native.entries.get_mut(&index).unwrap().value =
                    json!([w.raw, w.picker]).to_string();
            }
            _ => unreachable!(),
        }
        let raw = form(&s).unwrap().raw_view().clone();
        refresh(&mut s, |source| source["blocks"] = json!([]));
        assert_eq!(form(&s).unwrap().raw_view(), &raw, "{mode}");
        assert_eq!(wire(&s), "2026-10-25");
    }
}
#[test]
fn acknowledged_literal_draft_protects_pristine_parent_from_removal_and_type_replacement() {
    for (literal, parsed) in [
        ("November 3rd, 2026", Some("2026-11-03")),
        ("October 25th, 2026", Some("2026-10-25")),
        ("", None),
        ("bad draft", None),
    ] {
        for replace in [false, true] {
            let mut s = setup(true, false);
            let b = binding(&s);
            let e = &s.app_home.native.entries[&(b.form.clone(), key())];
            let mut event = occurrence(e.mount, e.seq + 1, "input", "2026-10-25", true);
            event.raw = literal.into();
            event.raw_dirty = true;
            event.parsed = parsed.map(str::to_owned);
            event.buffer_valid = parsed.is_some() || literal.is_empty();
            event.value = json!([event.raw, event.picker]).to_string();
            assert!(receive(&mut s, &b, event.mount, &event).is_none());
            assert!(!form(&s).unwrap().dirty());
            let before = form(&s).unwrap().raw_view().clone();
            let state = form(&s).unwrap().state().unwrap();
            refresh(&mut s, |raw| {
                if replace {
                    raw["blocks"][0]["element"] =
                        json!({"type":"plain_text_input","action_id":"value"});
                } else {
                    raw["blocks"] = json!([]);
                }
            });
            assert_eq!(form(&s).unwrap().raw_view(), &before);
            assert_eq!(form(&s).unwrap().state().unwrap(), state);
            let w = &s.app_home.native.dates[&(b.form.clone(), key())];
            assert_eq!(w.raw, literal);
            assert!(w.raw_dirty);
            assert_eq!(w.visible.as_deref(), Some("2026-10-25"));
        }
    }
}
#[test]
fn real_hydrate_preserves_initialized_pristine_parent_while_initial_changes_display_only() {
    let mut s = setup(true, false);
    assert!(!form(&s).unwrap().dirty());
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_date"] = json!("2026-12-01")
    });
    assert_eq!(wire(&s), "2026-10-25");
    assert_eq!(visible(&s).as_deref(), Some("2026-12-01"));
    assert!(!form(&s).unwrap().dirty());
    mount(&mut s);
    assert_eq!(wire(&s), "2026-10-25");
    assert!(crate::app_home::inputs::ready(&s));
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_date");
    });
    assert_eq!(visible(&s), None);
    assert_eq!(wire(&s), "2026-10-25");
    mount(&mut s);
    assert_eq!(wire(&s), "2026-10-25");
}
#[test]
fn dirty_parent_survives_initial_and_saved_updates_and_fresh_visit_seeds_latest_parent() {
    let mut s = setup(true, false);
    emit(&mut s, "commit", "2026-11-01", true).unwrap();
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_date"] = json!("2026-12-01")
    });
    mount(&mut s);
    assert_eq!(wire(&s), "2026-11-01");
    assert_eq!(visible(&s).as_deref(), Some("2026-12-01"));
    refresh(
        &mut s,
        |raw| {
            raw["state"] = json!({"values":{"day":{"value":{"type":"datepicker","selected_date":"2027-01-01"}}}})
        },
    );
    mount(&mut s);
    assert_eq!(wire(&s), "2026-11-01");
    assert_eq!(visible(&s).as_deref(), Some("2026-12-01"));
    let new_owner = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = new_owner.home_visit;
    h.owner = new_owner;
    hydrate(&mut s);
    assert!(parents(&s).is_empty());
    mount(&mut s);
    assert_eq!(visible(&s).as_deref(), Some("2026-11-01"));
}
#[test]
fn optional_clear_quiet_commit_and_required_native_null_refusal_are_distinct() {
    let mut s = setup(true, false);
    let e = emit(&mut s, "commit", "", true).unwrap();
    assert!(e.action()["selected_date"].is_null());
    assert!(wire(&s).is_null());
    let mut quiet = setup(true, true);
    assert!(emit(&mut quiet, "commit", "2026-11-01", true).is_none());
    assert_eq!(wire(&quiet), "2026-11-01");
    let mut required = setup(false, false);
    assert!(emit(&mut required, "commit", "", true).is_none());
    assert_eq!(wire(&required), "2026-10-25");
}
#[test]
fn native_invalid_correction_clears_safety_without_committing_changed_buffer() {
    let mut s = setup(true, false);
    assert!(emit(&mut s, "input", "", false).is_none());
    assert!(form(&s).unwrap().state().is_err());
    assert!(!crate::app_home::inputs::ready(&s));
    assert!(crate::app_home::inputs::presentation_error(&s, &key()).is_some());
    hydrate(&mut s);
    assert_eq!(visible(&s).as_deref(), Some("2026-10-25"));
    assert!(emit(&mut s, "input", "2026-12-01", true).is_none());
    assert_eq!(wire(&s), "2026-10-25");
    assert!(crate::app_home::inputs::presentation_error(&s, &key()).is_none());
    assert!(crate::app_home::inputs::ready(&s));
    assert!(emit(&mut s, "input", "2026-10-25", true).is_none());
    assert!(crate::app_home::inputs::ready(&s));
}
#[test]
fn stale_source_old_mount_and_foreign_account_cannot_commit() {
    let mut s = setup(true, false);
    let b = binding(&s);
    let e = &s.app_home.native.entries[&(b.form.clone(), key())];
    let event = occurrence(e.mount, e.seq + 1, "commit", "2026-11-01", true);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_date"] = json!("2026-12-01")
    });
    assert!(receive(&mut s, &b, event.mount, &event).is_none());
    assert_eq!(wire(&s), "2026-10-25");
    mount(&mut s);
    assert!(receive(&mut s, &b, event.mount, &event).is_none());
    assert_eq!(wire(&s), "2026-10-25");
    let b = binding(&s);
    s.core.account_epoch = s.core.account_epoch.wrapping_add(1);
    assert!(receive(&mut s, &b, event.mount, &event).is_none());
    assert!(parents(&s).is_empty());
}

#[test]
fn retired_closed_valid_draft_parks_without_parent_commit_or_release() {
    let mut s = setup(true, false);
    emit(&mut s, "input", "2026-12-01", true);
    let b = binding(&s);
    let e = &s.app_home.native.entries[&(b.form.clone(), key())];
    let event = occurrence(e.mount, e.seq + 1, "closed", "2026-12-01", true);
    s.app_home.surface = None;
    assert!(receive(&mut s, &b, event.mount, &event).is_none());
    let e = &s.app_home.native.entries[&(b.form.clone(), key())];
    assert!(e.pending);
    assert_eq!(e.value, json!(["2026-12-01", "2026-12-01"]).to_string());
    assert_eq!(
        s.app_home.forms[&b.form].state().unwrap()["values"]["day"]["value"]["selected_date"],
        "2026-10-25"
    );
    assert!(parents(&s).is_empty());
}

#[test]
fn absent_local_date_and_committed_empty_local_date_are_not_the_same_occurrence() {
    let mut s = setup(true, false);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_date");
    });
    mount(&mut s);
    assert_eq!(visible(&s), None);
    let first = emit(&mut s, "commit", "", true).unwrap();
    assert!(first.action()["selected_date"].is_null());
    assert_eq!(visible(&s), Some(String::new()));
    assert!(wire(&s).is_null());
    let revision = crate::app_home::inputs::revision(&s);
    assert!(emit(&mut s, "commit", "", true).is_none());
    assert_eq!(crate::app_home::inputs::revision(&s), revision);
}

#[test]
fn acknowledged_valid_draft_keeps_earlier_action_and_latest_committed_parent() {
    use super_platinum_core::slack::client::RequestBody;
    let mut s = setup(true, false);
    let event = emit(&mut s, "commit", "2026-11-01", true).unwrap();
    let b = binding(&s);
    let view = s
        .app_home
        .surface
        .as_ref()
        .unwrap()
        .view
        .as_ref()
        .unwrap()
        .clone();
    let action = view
        .static_action(&event, &b.owner.ctx.session.user_id)
        .unwrap();
    assert!(emit(&mut s, "input", "2026-12-01", true).is_none());
    assert!(crate::app_home::inputs::ready(&s));
    assert_eq!(visible(&s).as_deref(), Some("2026-11-01"));
    assert_eq!(
        s.app_home.native.entries[&(b.form.clone(), key())].value,
        json!(["2026-12-01", "2026-12-01"]).to_string()
    );
    let route = context::Witness::capture(&s).unwrap();
    let request = action
        .prepare_with_context(
            &b.owner.ctx.client,
            &b.owner.ctx.session,
            &view,
            form(&s).unwrap(),
            "synthetic",
            route.team_id(),
        )
        .unwrap();
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    let fields: std::collections::BTreeMap<_, _> = fields.into_iter().collect();
    let actions: Value = serde_json::from_str(&fields["actions"]).unwrap();
    let state: Value = serde_json::from_str(&fields["state"]).unwrap();
    assert_eq!(actions[0]["selected_date"], "2026-11-01");
    assert_eq!(
        state["values"]["day"]["value"]["selected_date"],
        "2026-11-01"
    );
    assert_eq!(wire(&s), "2026-11-01");
}

fn occurrence(mount: u64, seq: u64, kind: &str, value: &str, valid: bool) -> Event {
    Event {
        mount,
        seq,
        kind: kind.into(),
        value: json!([value, value]).to_string(),
        raw: value.into(),
        raw_dirty: false,
        picker: value.into(),
        selected: Some(value.into()),
        parsed: (!value.is_empty()).then(|| value.into()),
        focused: false,
        picker_status: "idle".into(),
        buffer_valid: valid,
        resolved_raw: None,
        resolved_selected: None,
        valid,
        bad: !valid,
    }
}
#[test]
fn editable_observation_attests_literal_separately_from_selected_and_parent() {
    let mut s = setup(true, false);
    let b = binding(&s);
    let e = &s.app_home.native.entries[&(b.form.clone(), key())];
    let mut event = occurrence(e.mount, e.seq + 1, "input", "2026-10-25", true);
    event.raw = "November 3rd, 2026".into();
    event.parsed = Some("2026-11-03".into());
    event.value = json!([event.raw, event.picker]).to_string();
    event.focused = true;
    assert!(receive(&mut s, &b, event.mount, &event).is_none());
    assert_eq!(wire(&s), "2026-10-25");
    assert!(crate::app_home::inputs::ready(&s));
    let w = &s.app_home.native.dates[&(b.form.clone(), key())];
    assert_eq!(w.raw, "November 3rd, 2026");
    assert_eq!(w.visible.as_deref(), Some("2026-10-25"));
    event.seq += 1;
    event.kind = "commit".into();
    event.selected = Some("2026-11-03".into());
    event.picker = "2026-11-03".into();
    event.value = json!([event.raw, event.picker]).to_string();
    assert_eq!(
        receive(&mut s, &b, event.mount, &event).unwrap().action()["selected_date"],
        "2026-11-03"
    );
    assert_eq!(wire(&s), "2026-11-03");
}
#[test]
fn mismatched_aggregate_never_rewrites_parent() {
    let mut s = setup(true, false);
    let b = binding(&s);
    let e = &s.app_home.native.entries[&(b.form.clone(), key())];
    let mut event = occurrence(e.mount, e.seq + 1, "commit", "2026-11-03", true);
    event.raw = "not the aggregate".into();
    assert!(receive(&mut s, &b, event.mount, &event).is_none());
    assert_eq!(wire(&s), "2026-10-25");
}

#[test]
fn invalid_editing_buffer_blocks_readiness_without_premature_field_error_and_explicit_clear_recovers()
 {
    let mut s = setup(true, false);
    let b = binding(&s);
    let e = &s.app_home.native.entries[&(b.form.clone(), key())];
    let mut event = occurrence(e.mount, e.seq + 1, "input", "2026-10-25", true);
    event.raw = "bad draft".into();
    event.parsed = None;
    event.buffer_valid = false;
    event.focused = true;
    event.value = json!([event.raw, event.picker]).to_string();
    assert!(receive(&mut s, &b, event.mount, &event).is_none());
    assert!(!crate::app_home::inputs::ready(&s));
    assert_eq!(wire(&s), "2026-10-25");
    assert!(crate::app_home::inputs::presentation_error(&s, &key()).is_none());
    event.seq += 1;
    event.kind = "commit".into();
    event.selected = Some(String::new());
    event.picker.clear();
    event.buffer_valid = true;
    event.resolved_raw = Some(event.raw.clone());
    event.resolved_selected = Some(String::new());
    event.value = json!([event.raw, event.picker]).to_string();
    assert!(receive(&mut s, &b, event.mount, &event).unwrap().action()["selected_date"].is_null());
    assert!(wire(&s).is_null());
    assert!(crate::app_home::inputs::ready(&s));
    assert_eq!(
        s.app_home.native.dates[&(b.form.clone(), key())].raw,
        "bad draft"
    );
}
