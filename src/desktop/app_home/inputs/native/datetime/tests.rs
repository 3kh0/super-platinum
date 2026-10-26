use super::*;
fn key() -> InputKey {
    InputKey {
        block_id: "clock".into(),
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
fn setup(optional: bool, quiet: bool, initial: Option<i64>) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([{"type":"input","block_id":"clock","dispatch_action":!quiet,"optional":optional,"label":{"type":"plain_text","text":"Date and time"},"element":{"type":"datetimepicker","action_id":"value"}}]);
    if let Some(v) = initial {
        raw["blocks"][0]["element"]["initial_date_time"] = json!(v);
    }
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    crate::app_home::inputs::hydrate(&mut s);
    mount(&mut s);
    let b = binding(&s);
    let e = &s.app_home.native.entries[&(b.form.clone(), key())];
    assert!(s.app_home.native.datetimes[&(b.form.clone(), key())].initialized);
    assert!(b.current(&s));
    assert!(mounted(&s, &b, e.mount));
    assert!(crate::app_home::inputs::ready(&s));
    s
}
fn event(s: &ShellState, kind: &str, selected: Option<i64>, date: &str, time: &str) -> Event {
    let b = binding(s);
    let row = &s.app_home.native.entries[&(b.form, key())];
    Event {
        mount: row.mount,
        seq: row.seq + 1,
        kind: kind.into(),
        value: serde_json::to_string(&[date, date, time]).unwrap(),
        valid: true,
        bad: false,
        selected,
        date: date.into(),
        time: time.into(),
        picker: date.into(),
        date_parsed: (!date.is_empty()).then(|| date.into()),
        date_focused: false,
        date_raw_dirty: false,
        time_raw_dirty: false,
        date_buffer_valid: true,
        date_resolved_raw: None,
        date_resolved_selected: None,
        picker_status: "idle".into(),
        child: "time".into(),
        caption: "Time zone: Test".into(),
        local_invalid: false,
        expanded: false,
    }
}
fn send(s: &mut ShellState, e: &Event) -> Option<StaticEvent> {
    let b = binding(s);
    receive(s, &b, e.mount, e)
}
fn mount(s: &mut ShellState) {
    let b = binding(s);
    let (selected, _) = configure(s, &b).unwrap();
    let mount_id = next();
    s.app_home.native.entries.insert(
        (b.form.clone(), key()),
        Entry {
            id: next(),
            mount: mount_id,
            seq: 0,
            pending: true,
            invalid: false,
            value: String::new(),
        },
    );
    let e = event(
        s,
        "mounted",
        (selected != 0).then_some(selected),
        "2026-10-25",
        "09:30",
    );
    assert!(send(s, &e).is_none());
}
fn wire(s: &ShellState) -> Value {
    form(s).unwrap().state().unwrap()["values"]["clock"]["value"].clone()
}
fn visible(s: &ShellState) -> i64 {
    s.app_home.native.datetimes[&(owner(s).unwrap(), key())].visible
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
fn mounted_drafts_keep_pristine_omission_and_commit_null_distinct() {
    let mut s = setup(true, false, None);
    assert!(wire(&s).get("selected_date_time").is_none());
    assert_eq!(visible(&s), 0);
    let revision = crate::app_home::inputs::revision(&s);
    let e = event(&s, "input", None, "2026-11-02", "5pm");
    assert!(send(&mut s, &e).is_none());
    assert_eq!(crate::app_home::inputs::revision(&s), revision);
    assert!(wire(&s).get("selected_date_time").is_none());
    assert!(crate::app_home::inputs::ready(&s));
    let e = event(&s, "commit", None, "", "");
    assert!(send(&mut s, &e).unwrap().action()["selected_date_time"].is_null());
    assert!(wire(&s)["selected_date_time"].is_null());
}
#[test]
fn signed_epoch_and_seconds_survive_renderer_callback_and_quiet_parent() {
    let mut s = setup(true, false, Some(1792946262));
    let e = event(&s, "commit", Some(-1), "", "");
    let a = send(&mut s, &e).unwrap();
    assert_eq!(a.action()["selected_date_time"], -1);
    assert_eq!(wire(&s)["selected_date_time"], -1);
    assert_eq!(visible(&s), -1);
    assert!(a.action().get("timezone").is_none());
    let mut q = setup(true, true, Some(1792946262));
    let e = event(&q, "commit", Some(1792946322), "2026-10-25", "17:58");
    assert!(send(&mut q, &e).is_none());
    assert_eq!(wire(&q)["selected_date_time"], 1792946322);
}
#[test]
fn mounted_initial_and_saved_refresh_do_not_reset_selected_or_drafts() {
    let mut s = setup(true, false, Some(1792946262));
    let e = event(&s, "commit", Some(1792946322), "2026-10-25", "17:58");
    send(&mut s, &e);
    let old = binding(&s);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_date_time"] = json!(1800000000);
        raw["state"] = json!({"values":{"clock":{"value":{"type":"datetimepicker","selected_date_time":1810000000}}}});
    });
    assert_eq!(visible(&s), 1792946322);
    assert_eq!(wire(&s)["selected_date_time"], 1792946322);
    assert!(!old.current(&s));
    mount(&mut s);
    assert!(crate::app_home::inputs::ready(&s));
    let e = event(&s, "commit", Some(1792946382), "2026-10-25", "17:59");
    assert_eq!(
        send(&mut s, &e).unwrap().action()["initial_date_time"],
        1800000000
    );
}
#[test]
fn required_leaf_clear_is_suppressed_and_invalid_native_buffer_needs_new_ack() {
    let mut s = setup(false, false, Some(1792946262));
    let e = event(&s, "commit", None, "", "");
    assert!(send(&mut s, &e).is_none());
    assert_eq!(wire(&s)["selected_date_time"], 1792946262);
    let mut bad = event(&s, "input", Some(1792946262), "", "bad");
    bad.valid = false;
    bad.bad = true;
    assert!(send(&mut s, &bad).is_none());
    assert!(!crate::app_home::inputs::ready(&s));
    assert!(form(&s).unwrap().native_error_text(&key()).is_some());
    let observed = event(&s, "input", Some(1792946262), "2026-11-02", "5pm");
    send(&mut s, &observed);
    assert!(form(&s).unwrap().native_error_text(&key()).is_none());
    assert!(crate::app_home::inputs::ready(&s));
    assert_eq!(wire(&s)["selected_date_time"], 1792946262);
}
#[test]
fn stale_sequence_mount_source_and_account_cannot_commit() {
    let mut s = setup(true, false, Some(1792946262));
    let e = event(&s, "commit", Some(1792946322), "2026-10-25", "17:58");
    send(&mut s, &e);
    assert!(send(&mut s, &e).is_none());
    let old = binding(&s);
    let late = event(&s, "commit", Some(1792946382), "2026-10-25", "17:59");
    refresh(&mut s, |raw| {
        raw["blocks"][0]["label"]["text"] = json!("Changed")
    });
    assert!(receive(&mut s, &old, late.mount, &late).is_none());
    assert_eq!(wire(&s)["selected_date_time"], 1792946322);
    mount(&mut s);
    assert!(send(&mut s, &late).is_none());
    let current = binding(&s);
    let e = event(&s, "commit", Some(1792946442), "2026-10-25", "18:00");
    s.core.account_epoch += 1;
    assert!(receive(&mut s, &current, e.mount, &e).is_none());
}
#[test]
fn exact_three_buffer_observation_is_required_and_local_invalid_time_is_not_parent_error() {
    let mut s = setup(true, false, Some(1792946262));
    let mut e = event(&s, "input", Some(1792946262), "2026-11-02", "garbage");
    e.value = json!(["foreign", "garbage"]).to_string();
    let before = s.app_home.native.entries[&(owner(&s).unwrap(), key())].seq;
    assert!(send(&mut s, &e).is_none());
    assert_eq!(
        s.app_home.native.entries[&(owner(&s).unwrap(), key())].seq,
        before
    );
    e.value = json!([e.date, e.picker, e.time]).to_string();
    e.local_invalid = true;
    send(&mut s, &e);
    assert!(crate::app_home::inputs::ready(&s));
    assert!(form(&s).unwrap().native_error_text(&key()).is_none());
    assert_eq!(wire(&s)["selected_date_time"], 1792946262);
}

#[test]
fn captured_action_uses_latest_epoch_state_without_committing_acknowledged_child_drafts() {
    use super_platinum_core::slack::client::RequestBody;
    let mut s = setup(true, false, Some(1792946262));
    let e = event(&s, "commit", Some(1792946322), "2026-10-25", "17:58");
    let original = send(&mut s, &e).unwrap();
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
        .static_action(&original, &b.owner.ctx.session.user_id)
        .unwrap();
    let e = event(&s, "commit", Some(1792946382), "2026-10-25", "17:59");
    send(&mut s, &e);
    let before = custody::Watermark::capture(&s, b.visit).unwrap();
    let draft = event(&s, "input", Some(1792946382), "2026-11-02", "930");
    send(&mut s, &draft);
    let after = custody::Watermark::capture(&s, b.visit).unwrap();
    assert_eq!(after.progress_from(&before), Some(true));
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
    assert_eq!(actions[0]["selected_date_time"], 1792946322);
    assert_eq!(
        state["values"]["clock"]["value"]["selected_date_time"],
        1792946382
    );
    assert_eq!(visible(&s), 1792946382);
    assert_eq!(
        s.app_home.native.entries[&(b.form, key())].value,
        json!(["2026-11-02", "2026-11-02", "930"]).to_string()
    );
}

#[test]
fn legacy_pair_and_foreign_picker_refuse_before_sequence_or_parent_mutation() {
    let mut s = setup(true, false, Some(1792946262));
    let mut e = event(&s, "input", Some(1792946262), "2026-11-02", "17:45");
    let before = proof(&s);
    for value in [json!([e.date, e.time]), json!([e.date, "foreign", e.time])] {
        e.value = value.to_string();
        assert!(send(&mut s, &e).is_none());
        assert_eq!(proof(&s), before);
        assert_eq!(wire(&s)["selected_date_time"], 1792946262);
    }
}

#[test]
fn unresolved_date_blocks_admission_without_eager_error_and_explicit_time_keeps_parent_current() {
    let mut s = setup(true, false, Some(1792946262));
    let mut e = event(&s, "input", Some(1792946262), "invalid raw date", "17:45");
    e.date_parsed = None;
    e.date_buffer_valid = false;
    e.date_raw_dirty = true;
    assert!(send(&mut s, &e).is_none());
    assert!(!crate::app_home::inputs::ready(&s));
    assert!(form(&s).unwrap().native_error_text(&key()).is_none());
    assert_eq!(wire(&s)["selected_date_time"], 1792946262);
    let mut e = event(&s, "commit", Some(1792946322), "invalid raw date", "17:45");
    e.date_parsed = None;
    e.date_buffer_valid = false;
    e.date_raw_dirty = true;
    assert!(send(&mut s, &e).is_some());
    assert_eq!(wire(&s)["selected_date_time"], 1792946322);
    assert_eq!(visible(&s), 1792946322);
    assert!(!crate::app_home::inputs::ready(&s));
    let e = event(&s, "input", Some(1792946322), "2026-11-02", "17:45");
    assert!(send(&mut s, &e).is_none());
    assert!(crate::app_home::inputs::ready(&s));
    assert_eq!(wire(&s)["selected_date_time"], 1792946322);
}

#[test]
fn pristine_removal_releases_token_but_either_received_child_draft_and_dirty_parent_refuse_atomically()
 {
    let mut pristine = setup(true, false, Some(1792946262));
    let raw = form(&pristine).unwrap().raw_view().clone();
    refresh(&mut pristine, |v| v["blocks"] = json!([]));
    assert!(!form(&pristine).unwrap().fields.contains_key(&key()));
    assert!(
        !pristine
            .app_home
            .native
            .datetimes
            .contains_key(&(owner(&pristine).unwrap(), key()))
    );
    refresh(&mut pristine, |v| *v = raw.clone());
    mount(&mut pristine);
    assert_eq!(visible(&pristine), 1792946262);
    for child in ["date", "time", "parent"] {
        let mut s = setup(true, false, Some(1792946262));
        let mut e = event(
            &s,
            if child == "parent" { "commit" } else { "input" },
            Some(1792946322),
            "2026-11-02",
            "17:45",
        );
        e.date_raw_dirty = child == "date";
        e.time_raw_dirty = child == "time";
        send(&mut s, &e);
        let before = wire(&s);
        let old_raw = form(&s).unwrap().raw_view().clone();
        refresh(&mut s, |v| v["blocks"] = json!([]));
        assert_eq!(wire(&s), before, "{child}");
        assert_eq!(form(&s).unwrap().raw_view(), &old_raw, "{child}");
        assert!(form(&s).unwrap().fields.contains_key(&key()));
    }
}
