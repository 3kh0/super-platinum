//! Pure serial-shell regressions against reference26. No browser occurrence,
//! wall-clock timer, loading affordance, or HTTP execution is claimed here.
use super::*;
use super_platinum_core::slack::client::RequestBody;

fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "pick".into(),
    }
}
fn option(value: &str) -> Value {
    json!({"text":{"type":"plain_text","text":value},"value":value,"opaque":{"keep":value}})
}
fn setup(optional: bool) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
      {"type":"input","block_id":"selection","optional":optional,"dispatch_action":true,"label":{"type":"plain_text","text":"Selection"},"element":{"type":"static_select","action_id":"pick","options":[option("a"),option("b")],"initial_option":option("a"),"placeholder":{"type":"plain_text","text":"Choose"}}},
      {"type":"input","block_id":"other","optional":true,"dispatch_action":true,"label":{"type":"plain_text","text":"Other"},"element":{"type":"static_select","action_id":"pick","options":[option("a"),option("b")],"initial_option":option("a")}},
      {"type":"input","block_id":"quiet","optional":true,"label":{"type":"plain_text","text":"Quiet"},"element":{"type":"static_select","action_id":"pick","options":[option("a"),option("b")]}}
    ]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    super::super::hydrate(&mut s);
    mount(&mut s, "selection");
    mount(&mut s, "other");
    assert!(ready(&s));
    s
}
fn binding(s: &ShellState, block: &str) -> Binding {
    let h = s.app_home.surface.as_ref().unwrap();
    Binding {
        owner: h.owner.clone(),
        form: owner(s).unwrap(),
        visit: actions::Key {
            visit: h.id,
            revision: h.revision,
        },
        key: key(block),
        raw: h.view.as_ref().unwrap().raw().clone(),
    }
}
fn mount(s: &mut ShellState, block: &str) {
    let b = binding(s, block);
    let index = (b.form.clone(), b.key.clone());
    let value = s.app_home.static_select.widgets[&index].visible.clone();
    s.app_home.static_select.mounts.insert(
        index,
        Mount {
            id: next(),
            seq: 1,
            value,
            binding: b,
        },
    );
}
fn occurrence(s: &ShellState, b: &Binding, index: Option<usize>) -> Occurrence {
    let m = &s.app_home.static_select.mounts[&(b.form.clone(), b.key.clone())];
    Occurrence {
        mount: m.id,
        seq: m.seq + 1,
        kind: "change".into(),
        index,
        trusted: false,
    }
}
fn emit(s: &mut ShellState, b: &Binding, index: Option<usize>) -> Option<(u64, Option<Queued>)> {
    let e = occurrence(s, b, index);
    receive(s, b, &e)
}
fn wire(s: &ShellState, block: &str) -> Value {
    form(s).unwrap().state().unwrap()["values"][block]["pick"]["selected_option"].clone()
}
fn visible(s: &ShellState, block: &str) -> Option<String> {
    s.app_home.static_select.widgets[&(owner(s).unwrap(), key(block))]
        .visible
        .clone()
}
fn refresh(s: &mut ShellState, edit: impl FnOnce(&mut Value)) {
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    edit(&mut raw);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    super::super::hydrate(s);
}

#[test]
fn repeated_selection_leads_once_then_retains_only_latest_trailing_occurrence() {
    let mut s = setup(true);
    let b = binding(&s, "selection");
    let index = (b.form.clone(), b.key.clone());
    let (first, leading) = emit(&mut s, &b, Some(0)).unwrap();
    let leading = leading.unwrap();
    assert_eq!(leading.edge, "leading");
    assert_eq!(leading.action.action()["selected_option"], option("a"));
    assert!(s.app_home.static_select.windows[&index].trailing.is_none());
    // Same selection is still an actual occurrence; no callback-level dedup.
    let (second, immediate) = emit(&mut s, &b, Some(0)).unwrap();
    assert_ne!(first, second);
    assert!(immediate.is_none());
    let (last, immediate) = emit(&mut s, &b, Some(1)).unwrap();
    assert!(immediate.is_none());
    let w = &s.app_home.static_select.windows[&index];
    let q = w.trailing.as_ref().unwrap();
    assert_eq!(w.id, last);
    assert_eq!(q.id, last);
    assert_eq!(q.edge, "trailing");
    assert_eq!(q.action.action()["selected_option"], option("b"));
    assert_eq!(wire(&s, "selection"), option("b"));
    assert_eq!(leading.action.action()["selected_option"], option("a"));
    // Native fixture checks actual 500ms deadline; no sleep/timer execution here.
}

#[test]
fn required_parent_clear_keeps_queued_valid_action_but_delivery_reads_current_null_state() {
    let mut s = setup(false);
    let b = binding(&s, "selection");
    emit(&mut s, &b, Some(0)).unwrap();
    let (id, _) = emit(&mut s, &b, Some(1)).unwrap();
    // Original legacy direct callback writes before validation; native required
    // clear is deliberately not offered, so exercise the real core callback.
    assert!(
        s.app_home
            .forms
            .get_mut(&b.form)
            .unwrap()
            .static_input(&b.key, None)
            .unwrap()
            .is_none()
    );
    assert!(form(&s).unwrap().fields[&b.key].error.is_some());
    assert_eq!(wire(&s, "selection"), Value::Null);
    let q = s.app_home.static_select.windows[&(b.form.clone(), b.key.clone())]
        .trailing
        .as_ref()
        .unwrap();
    assert_eq!(q.id, id);
    assert!(q.current(&s));
    let h = s.app_home.surface.as_ref().unwrap();
    let request = q
        .action
        .prepare_with_context(
            &b.owner.ctx.client,
            &b.owner.ctx.session,
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
    let action: Value = serde_json::from_str(&fields["actions"]).unwrap();
    let state: Value = serde_json::from_str(&fields["state"]).unwrap();
    assert_eq!(action[0]["selected_option"], option("b"));
    assert_eq!(action[0]["initial_option"], option("a"));
    assert_eq!(
        state["values"]["selection"]["pick"]["selected_option"],
        Value::Null
    );
}

#[test]
fn optional_clear_dispatches_null_while_required_native_clear_cannot_mint_event() {
    let mut s = setup(true);
    let b = binding(&s, "selection");
    let (_, q) = emit(&mut s, &b, None).unwrap();
    assert_eq!(q.unwrap().action.action()["selected_option"], Value::Null);
    assert_eq!(visible(&s, "selection"), None);
    let mut s = setup(false);
    let b = binding(&s, "selection");
    assert!(emit(&mut s, &b, None).is_none());
    assert_eq!(wire(&s, "selection"), option("a"));
    assert!(s.app_home.static_select.windows.is_empty());
}

#[test]
fn props_change_visible_only_and_saved_refresh_does_not_restore_parent_from_widget() {
    let mut s = setup(true);
    let b = binding(&s, "selection");
    emit(&mut s, &b, Some(1)).unwrap();
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_option"] = option("b")
    });
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["initial_option"] = option("a")
    });
    assert_eq!(visible(&s, "selection").as_deref(), Some("a"));
    assert_eq!(wire(&s, "selection"), option("b"));
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_option");
    });
    assert_eq!(visible(&s, "selection"), None);
    assert_eq!(wire(&s, "selection"), option("b"));
    assert!(!b.current(&s));
    // Pristine parent can accept a raw saved-state refresh independently of the
    // mounted widget's saved-prop no-op; never restore parent from its old display.
    let mut s = setup(true);
    refresh(
        &mut s,
        |raw| raw["state"] = json!({"values":{"selection":{"pick":{"type":"static_select","selected_option":option("b")}}}}),
    );
    assert_eq!(wire(&s, "selection"), option("b"));
    assert_eq!(visible(&s, "selection").as_deref(), Some("a"));
}

#[test]
fn options_removal_clears_display_but_does_not_mint_a_replacement_parent_option() {
    let mut s = setup(true);
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_option");
    });
    mount(&mut s, "selection");
    let b = binding(&s, "selection");
    emit(&mut s, &b, Some(0)).unwrap();
    assert_eq!(visible(&s, "selection").as_deref(), Some("a"));
    refresh(&mut s, |raw| {
        raw["blocks"][0]["element"]["options"] = json!([option("b")]);
    });
    assert_eq!(visible(&s, "selection"), None);
    assert_eq!(
        form(&s).unwrap().fields[&key("selection")].value,
        InputValue::Selected(vec!["a".into()])
    );
    // The accepted value is retained, but an option removed by the source cannot
    // become a different wire value. The product's structural guard stays closed.
    assert!(form(&s).unwrap().state().is_err());
    assert!(!b.current(&s));
    poll(&mut s);
    assert!(s.app_home.static_select.windows.is_empty());
}

#[test]
fn loading_is_per_block_and_does_not_erase_independent_callback_window() {
    let mut s = setup(true);
    let b = binding(&s, "selection");
    let other = binding(&s, "other");
    let (_, q) = emit(&mut s, &b, Some(1)).unwrap();
    let q = q.unwrap();
    s.app_home.static_select.activity.insert(
        (b.form.clone(), b.key.block_id.clone()),
        Activity {
            binding: b.clone(),
            route: q.route.clone(),
            loading: true,
            error: None,
        },
    );
    assert!(loading(&s, "selection"));
    assert!(!loading(&s, "other"));
    let (_, other_q) = emit(&mut s, &other, Some(1)).unwrap();
    assert_eq!(other_q.unwrap().edge, "leading");
    assert_eq!(s.app_home.static_select.windows.len(), 2);
    assert!(q.current(&s));
    // Original UI disables the loading block. This pure test does not fabricate
    // user access to a disabled native control or assert pending rejection.
}

#[test]
fn exact_sequence_option_membership_and_mount_custody_precede_parent_mutation() {
    let mut s = setup(true);
    let b = binding(&s, "selection");
    let good = occurrence(&s, &b, Some(1));
    for mode in ["duplicate", "gap", "mount", "index"] {
        let mut bad = good.clone();
        match mode {
            "duplicate" => bad.seq -= 1,
            "gap" => bad.seq += 1,
            "mount" => bad.mount = next(),
            "index" => bad.index = Some(99),
            _ => unreachable!(),
        };
        assert!(receive(&mut s, &b, &bad).is_none(), "{mode}");
        assert_eq!(wire(&s, "selection"), option("a"));
    }
    assert!(receive(&mut s, &b, &good).is_some());
    assert!(!dispatching(&s, &key("quiet")));
    let visit = b.visit;
    assert!(super::super::set_local(
        &mut s,
        visit,
        &key("quiet"),
        InputValue::Selected(vec!["b".into()])
    ));
    assert_eq!(wire(&s, "quiet"), option("b"));
    assert_eq!(s.app_home.static_select.windows.len(), 1);
}

#[test]
fn source_route_socket_account_visit_and_remount_retire_trailing_without_erasing_parent() {
    for retirement in ["source", "context", "socket", "account", "visit", "mount"] {
        let mut s = setup(true);
        let b = binding(&s, "selection");
        emit(&mut s, &b, Some(0)).unwrap();
        emit(&mut s, &b, Some(1)).unwrap();
        let q = s.app_home.static_select.windows[&(b.form.clone(), b.key.clone())]
            .trailing
            .clone()
            .unwrap();
        let team = s.core.active_team.clone().unwrap();
        match retirement {
            "source" => refresh(&mut s, |raw| raw["hash"] = json!("replacement")),
            "context" => s
                .core
                .session
                .as_mut()
                .unwrap()
                .unified_user_client_enabled
                .clear(),
            "socket" => s.core.workspaces.get_mut(&team).unwrap().rt_generation += 1,
            "account" => s.core.account_epoch += 1,
            "visit" => s.app_home.surface.as_mut().unwrap().id = next(),
            "mount" => mount(&mut s, "selection"),
            _ => unreachable!(),
        }
        assert!(!q.current(&s), "{retirement}");
        assert!(poll(&mut s));
        assert!(
            !s.app_home
                .static_select
                .windows
                .contains_key(&(b.form.clone(), b.key.clone()))
        );
        assert_eq!(
            s.app_home.forms[&b.form].fields[&b.key].value,
            InputValue::Selected(vec!["b".into()])
        );
    }
}

#[test]
fn overdue_unserviced_window_does_not_invent_a_second_leading_edge() {
    let mut s = setup(true);
    let b = binding(&s, "selection");
    emit(&mut s, &b, Some(0)).unwrap();
    let index = (b.form.clone(), b.key.clone());
    s.app_home
        .static_select
        .windows
        .get_mut(&index)
        .unwrap()
        .last = Instant::now() - Duration::from_secs(1);
    // Original S6U4 keeps timerId while a due task is unserviced. A later callback
    // replaces trailing args; elapsed time alone must not mint a leading edge.
    let (id, leading) = emit(&mut s, &b, Some(1)).unwrap();
    assert!(leading.is_none());
    let w = &s.app_home.static_select.windows[&index];
    assert_eq!(w.id, id);
    let q = w.trailing.as_ref().unwrap();
    assert_eq!(q.edge, "trailing");
    assert_eq!(q.action.action()["selected_option"], option("b"));
}

#[test]
fn new_visit_after_optional_clear_displays_initial_without_rewriting_committed_null() {
    let mut s = setup(true);
    let b = binding(&s, "selection");
    let (_, leading) = emit(&mut s, &b, None).unwrap();
    assert!(leading.is_some());
    assert_eq!(wire(&s, "selection"), Value::Null);
    assert_eq!(visible(&s, "selection"), None);
    hydrate(&mut s);
    assert_eq!(visible(&s, "selection"), None); // Same mounted widget remains cleared.
    let index = (b.form.clone(), b.key.clone());
    let window_id = s.app_home.static_select.windows[&index].id;
    let owner = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = owner.home_visit;
    h.owner = owner;
    super::super::hydrate(&mut s);
    assert_eq!(visible(&s, "selection").as_deref(), Some("a"));
    assert_eq!(wire(&s, "selection"), Value::Null);
    // Hydration neither emits a selection nor replaces the old queued window.
    assert_eq!(s.app_home.static_select.windows.len(), 1);
    assert_eq!(s.app_home.static_select.windows[&index].id, window_id);
    assert!(s.app_home.static_select.windows[&index].trailing.is_none());
    assert!(!b.current(&s));
    poll(&mut s);
    assert!(s.app_home.static_select.windows.is_empty());
}

#[test]
fn reset_spends_widget_mount_and_queue_before_restoring_source_initial() {
    let mut s = setup(true);
    let b = binding(&s, "selection");
    emit(&mut s, &b, Some(0)).unwrap();
    emit(&mut s, &b, Some(1)).unwrap();
    let index = (b.form.clone(), b.key.clone());
    let q = s.app_home.static_select.windows[&index]
        .trailing
        .clone()
        .unwrap();
    let old_mount = s.app_home.static_select.mounts[&index].id;
    s.app_home.static_select.activity.insert(
        (b.form.clone(), b.key.block_id.clone()),
        Activity {
            binding: b.clone(),
            route: q.route.clone(),
            loading: true,
            error: None,
        },
    );
    assert_eq!(visible(&s, "selection").as_deref(), Some("b"));
    assert_eq!(wire(&s, "selection"), option("b"));
    assert!(q.current(&s));
    let parsed = HomeForm::parse(form(&s).unwrap().raw_view()).unwrap();
    // Exercise the real owner-scoped cleanup. The Signal Reset button and its
    // sequencing are independently covered by the native fixture.
    reset(&mut s, &b.form);
    assert!(s.app_home.static_select.widgets.is_empty());
    assert!(s.app_home.static_select.mounts.is_empty());
    assert!(s.app_home.static_select.windows.is_empty());
    assert!(s.app_home.static_select.activity.is_empty());
    assert!(!q.current(&s));
    s.app_home.forms.insert(b.form.clone(), parsed);
    bump(&mut s, &b.form);
    s.app_home.surface.as_mut().unwrap().revision = next();
    hydrate(&mut s);
    assert_eq!(visible(&s, "selection").as_deref(), Some("a"));
    assert_eq!(wire(&s, "selection"), option("a"));
    assert!(!ready(&s));
    assert!(s.app_home.static_select.windows.is_empty());
    mount(&mut s, "selection");
    mount(&mut s, "other");
    assert!(ready(&s));
    assert_ne!(s.app_home.static_select.mounts[&index].id, old_mount);
    assert!(!b.current(&s));
    assert!(!q.current(&s));
}

#[test]
fn date_only_shared_delivery_has_empty_static_inventory_bound_to_home_visit() {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::inputs::native::date::fixture::install(&mut s);
    let h = s.app_home.surface.as_ref().unwrap();
    let visit = actions::Key {
        visit: h.id,
        revision: h.revision,
    };
    assert!(
        form(&s)
            .unwrap()
            .fields
            .values()
            .any(|f| matches!(f.kind, InputKind::Date))
    );
    assert_eq!(
        bridge::expected_visit(&s, visit),
        Some(json!({
            "visit":visit.visit,"revision":visit.revision,"fields":[]
        }))
    );
    s.app_home.surface.as_mut().unwrap().revision = next();
    assert!(bridge::expected_visit(&s, visit).is_none());
}
