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
fn setup(optional: bool, quiet: bool) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([{"type":"input","block_id":"clock","dispatch_action":!quiet,"optional":optional,"label":{"type":"plain_text","text":"Time"},"element":{"type":"timepicker","action_id":"value","initial_time":"09:30"}}]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    crate::app_home::inputs::hydrate(&mut s);
    mount(&mut s);
    let b = binding(&s);
    let row = &s.app_home.native.entries[&(b.form.clone(), key())];
    assert!(s.app_home.native.times[&(b.form.clone(), key())].initialized);
    assert!(b.current(&s));
    assert!(mounted(&s, &b, row.mount));
    assert!(crate::app_home::inputs::ready(&s));
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
    let e = Event {
        mount,
        seq: 1,
        kind: "mounted".into(),
        selected: Some(value.clone()),
        parsed: Some(value.clone()),
        local_invalid: false,
        expanded: false,
        raw_dirty: false,
        value,
        valid: true,
        bad: false,
    };
    assert!(receive(s, &b, mount, &e).is_none());
}
fn emit(s: &mut ShellState, kind: &str, value: &str, valid: bool) -> Option<StaticEvent> {
    let b = binding(s);
    let e = &s.app_home.native.entries[&(b.form.clone(), key())];
    let event = Event {
        mount: e.mount,
        seq: e.seq + 1,
        kind: kind.into(),
        value: value.into(),
        selected: (!value.is_empty()).then(|| value.into()),
        parsed: (!value.is_empty()).then(|| value.into()),
        local_invalid: false,
        expanded: false,
        raw_dirty: false,
        valid,
        bad: !valid,
    };
    receive(s, &b, event.mount, &event)
}
fn wire(s: &ShellState) -> Value {
    form(s).unwrap().state().unwrap()["values"]["clock"]["value"]["selected_time"].clone()
}
fn visible(s: &ShellState) -> Option<String> {
    s.app_home.native.times[&(owner(s).unwrap(), key())]
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
fn literal_draft_does_not_commit_or_cancel_captured_action() {
    use super_platinum_core::slack::client::RequestBody;
    let mut s = setup(true, false);
    let event = emit(&mut s, "commit", "17:45", true).unwrap();
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
    let row = &s.app_home.native.entries[&(b.form.clone(), key())];
    let draft = Event {
        mount: row.mount,
        seq: row.seq + 1,
        kind: "input".into(),
        value: "930".into(),
        valid: true,
        bad: false,
        selected: Some("17:45".into()),
        parsed: Some("09:30".into()),
        local_invalid: false,
        expanded: true,
        raw_dirty: true,
    };
    let revision = crate::app_home::inputs::revision(&s);
    assert!(receive(&mut s, &b, draft.mount, &draft).is_none());
    assert_eq!(crate::app_home::inputs::revision(&s), revision);
    assert_eq!(wire(&s), "17:45");
    assert_eq!(visible(&s).as_deref(), Some("17:45"));
    assert!(crate::app_home::inputs::ready(&s));
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
    assert_eq!(actions[0]["selected_time"], "17:45");
    assert_eq!(state["values"]["clock"]["value"]["selected_time"], "17:45");
    assert_eq!(s.app_home.native.entries[&(b.form, key())].value, "930");
}

#[test]
fn changed_initial_is_display_only_and_old_timezone_wire_survives_until_callback() {
    let mut s = setup(true, false);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_time"] = json!("11:00");
        raw["blocks"][0]["element"]["timezone"] = json!("Asia/Tokyo");
    });
    assert_eq!(wire(&s), "09:30");
    assert_eq!(visible(&s).as_deref(), Some("11:00"));
    assert!(
        form(&s).unwrap().state().unwrap()["values"]["clock"]["value"]
            .get("timezone")
            .is_none()
    );
    mount(&mut s);
    let event = emit(&mut s, "commit", "12:00", true).unwrap();
    assert_eq!(event.action()["timezone"], "Asia/Tokyo");
    assert_eq!(event.action()["initial_time"], "11:00");
    assert_eq!(wire(&s), "12:00");
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_time");
    });
    assert_eq!(visible(&s), None);
    assert_eq!(wire(&s), "12:00");
    mount(&mut s);
    assert!(crate::app_home::inputs::ready(&s));
}

#[test]
fn repeated_optional_blank_callbacks_are_not_date_dedup_and_quiet_still_commits() {
    let mut s = setup(true, false);
    for _ in 0..2 {
        let event = emit(&mut s, "commit", "", true).unwrap();
        assert!(event.action()["selected_time"].is_null());
    }
    assert!(wire(&s).is_null());
    assert_eq!(visible(&s), None);
    let mut q = setup(true, true);
    assert!(emit(&mut q, "commit", "17:45", true).is_none());
    assert_eq!(wire(&q), "17:45");
    let mut required = setup(false, false);
    assert!(emit(&mut required, "commit", "", true).is_none());
    assert_eq!(wire(&required), "09:30");
}

#[test]
fn unconfirmed_native_input_blocks_but_observed_invalid_local_text_does_not_become_parent_error() {
    let mut s = setup(true, false);
    assert!(emit(&mut s, "input", "", false).is_none());
    assert!(!crate::app_home::inputs::ready(&s));
    let b = binding(&s);
    let row = &s.app_home.native.entries[&(b.form.clone(), key())];
    let event = Event {
        mount: row.mount,
        seq: row.seq + 1,
        kind: "input".into(),
        value: "garbage".into(),
        valid: true,
        bad: false,
        selected: Some("09:30".into()),
        parsed: None,
        local_invalid: true,
        expanded: true,
        raw_dirty: true,
    };
    assert!(receive(&mut s, &b, event.mount, &event).is_none());
    assert!(crate::app_home::inputs::ready(&s));
    assert_eq!(wire(&s), "09:30");
    assert!(crate::app_home::inputs::presentation_error(&s, &key()).is_none());
    assert!(s.app_home.native.times[&(b.form, key())].local_invalid);
}

#[test]
fn stale_source_mount_and_locale_cannot_authorize_time_delivery() {
    let mut s = setup(true, false);
    let b = binding(&s);
    let row = &s.app_home.native.entries[&(b.form.clone(), key())];
    let event = Event {
        mount: row.mount,
        seq: row.seq + 1,
        kind: "commit".into(),
        value: "17:45".into(),
        valid: true,
        bad: false,
        selected: Some("17:45".into()),
        parsed: Some("17:45".into()),
        local_invalid: false,
        expanded: false,
        raw_dirty: false,
    };
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_time"] = json!("11:00")
    });
    assert!(receive(&mut s, &b, event.mount, &event).is_none());
    assert_eq!(wire(&s), "09:30");
    mount(&mut s);
    assert!(receive(&mut s, &b, event.mount, &event).is_none());
    let b = binding(&s);
    let mount_id = s.app_home.native.entries[&(b.form.clone(), key())].mount;
    assert!(mounted(&s, &b, mount_id));
    s.core.workspaces.get_mut("T1").unwrap().locale = Some("ko-KR".into());
    assert!(!mounted(&s, &b, mount_id));
    assert!(!crate::app_home::inputs::ready(&s));
    mount(&mut s);
    assert!(crate::app_home::inputs::ready(&s));
    assert_eq!(preferences(&s).0, "ko-KR");
}

#[test]
fn fresh_visit_seeds_committed_parent_and_closed_draft_never_commits() {
    let mut s = setup(true, false);
    emit(&mut s, "commit", "17:45", true).unwrap();
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_time"] = json!("11:00")
    });
    mount(&mut s);
    assert_eq!(visible(&s).as_deref(), Some("11:00"));
    let new_owner = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = new_owner.home_visit;
    h.owner = new_owner;
    hydrate(&mut s);
    mount(&mut s);
    assert_eq!(visible(&s).as_deref(), Some("17:45"));
    let b = binding(&s);
    let row = &s.app_home.native.entries[&(b.form.clone(), key())];
    let event = Event {
        mount: row.mount,
        seq: row.seq + 1,
        kind: "closed".into(),
        value: "930".into(),
        valid: true,
        bad: false,
        selected: Some("17:45".into()),
        parsed: Some("09:30".into()),
        local_invalid: false,
        expanded: false,
        raw_dirty: false,
    };
    s.app_home.surface = None;
    assert!(receive(&mut s, &b, event.mount, &event).is_none());
    assert_eq!(
        s.app_home.forms[&b.form].state().unwrap()["values"]["clock"]["value"]["selected_time"],
        "17:45"
    );
    assert!(s.app_home.native.entries[&(b.form, key())].pending);
    assert!(parents(&s).is_empty());
}

#[test]
fn configured_locale_provider_filters_exact_codes_and_falls_back_without_page_sources() {
    let mut s = setup(true, false);
    for configured in [
        None,
        Some(""),
        Some("xx-YY"),
        Some("es"),
        Some("es-419"),
        Some("de-de"),
    ] {
        s.core.workspaces.get_mut("T1").unwrap().locale = configured.map(str::to_owned);
        assert_eq!(preferences(&s).0, "en-US", "{configured:?}");
        mount(&mut s);
        assert!(crate::app_home::inputs::ready(&s));
        assert_eq!(wire(&s), "09:30");
    }
    for configured in [
        "en-US", "en-GB", "de-DE", "es-ES", "es-LA", "fr-FR", "it-IT", "ja-JP", "ko-KR", "pt-BR",
        "zh-CN", "zh-TW",
    ] {
        s.core.workspaces.get_mut("T1").unwrap().locale = Some(configured.into());
        assert_eq!(preferences(&s).0, configured);
        mount(&mut s);
        assert!(crate::app_home::inputs::ready(&s));
    }
}

#[test]
fn pristine_time_removal_readd_rearms_focus_while_dirty_parent_removal_refuses() {
    let mut s = setup(true, false);
    let before = crate::app_home::inputs::focus::config(&s, &key()).unwrap();
    assert_eq!(before["kind"], "time");
    let raw = form(&s).unwrap().raw_view().clone();
    refresh(&mut s, |v| v["blocks"] = json!([]));
    assert!(!form(&s).unwrap().fields.contains_key(&key()));
    assert!(crate::app_home::inputs::focus::config(&s, &key()).is_none());
    refresh(&mut s, |v| *v = raw);
    mount(&mut s);
    assert_ne!(
        crate::app_home::inputs::focus::config(&s, &key()).unwrap()["instance"],
        before["instance"]
    );
    emit(&mut s, "commit", "17:45", true).unwrap();
    let state = form(&s).unwrap().state().unwrap();
    let raw = form(&s).unwrap().raw_view().clone();
    refresh(&mut s, |v| v["blocks"] = json!([]));
    assert_eq!(form(&s).unwrap().state().unwrap(), state);
    assert_eq!(form(&s).unwrap().raw_view(), &raw);
}
#[test]
fn acknowledged_literal_time_draft_protects_pristine_parent_from_removed_source() {
    let mut s = setup(true, true);
    let b = binding(&s);
    let row = &s.app_home.native.entries[&(b.form.clone(), key())];
    let event = Event {
        mount: row.mount,
        seq: row.seq + 1,
        kind: "input".into(),
        value: "17:45".into(),
        valid: true,
        bad: false,
        selected: Some("09:30".into()),
        parsed: Some("17:45".into()),
        local_invalid: false,
        expanded: true,
        raw_dirty: true,
    };
    assert!(receive(&mut s, &b, event.mount, &event).is_none());
    let raw = form(&s).unwrap().raw_view().clone();
    let state = form(&s).unwrap().state().unwrap();
    refresh(&mut s, |v| v["blocks"] = json!([]));
    assert_eq!(form(&s).unwrap().raw_view(), &raw);
    assert_eq!(form(&s).unwrap().state().unwrap(), state);
    assert_eq!(
        s.app_home.native.entries[&(b.form.clone(), key())].value,
        "17:45"
    );
    assert!(s.app_home.native.times[&(b.form, key())].raw_dirty);
}
#[test]
fn time_reset_retains_focus_history_but_reseeds_native_widget() {
    let mut s = setup(true, true);
    let o = owner(&s).unwrap();
    let before = crate::app_home::inputs::focus::config(&s, &key()).unwrap();
    emit(&mut s, "commit", "17:45", true);
    let raw = form(&s).unwrap().raw_view().clone();
    let mut dom = dioxus::prelude::VirtualDom::new(dioxus::prelude::VNode::empty);
    dom.rebuild_to_vec();
    dom.in_scope(dioxus::prelude::ScopeId::ROOT, || {
        super::super::reset(&mut s, &o);
    });
    s.app_home.forms.insert(o, HomeForm::parse(&raw).unwrap());
    crate::app_home::inputs::hydrate(&mut s);
    mount(&mut s);
    assert_eq!(visible(&s).as_deref(), Some("09:30"));
    assert_eq!(
        crate::app_home::inputs::focus::config(&s, &key()).unwrap()["instance"],
        before["instance"]
    );
}
