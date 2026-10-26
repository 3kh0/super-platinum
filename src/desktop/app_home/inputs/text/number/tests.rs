//! Serial helper tests: no native browser, physical input or timer execution.
use super::*;
use super_platinum_core::slack::client::RequestBody;

fn key() -> InputKey {
    InputKey {
        block_id: "amount".into(),
        action_id: "value".into(),
    }
}
fn setup(optional: bool, quiet: bool) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([{"type":"input","block_id":"amount","optional":optional,"dispatch_action":!quiet,
        "label":{"type":"plain_text","text":"Amount"},"element":{"type":"number_input","action_id":"value","is_decimal_allowed":true,
        "initial_value":"1.20","dispatch_action_config":{"trigger_actions_on":["on_enter_pressed","on_character_entered"]}}}]);
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
fn mount(s: &mut ShellState) {
    let b = binding(s);
    let c = configure(s, &b).unwrap();
    let local = c["local"].as_str().map(str::to_owned);
    s.app_home.text.mounts.insert(
        (b.form.clone(), key()),
        Mount {
            id: next(),
            seq: 0,
            value: String::new(),
            binding: b.clone(),
        },
    );
    emit(
        s,
        "mounted",
        local.as_deref().unwrap_or(""),
        local.as_deref(),
        false,
        None,
        false,
        false,
    );
    assert!(s.app_home.text.numbers[&(b.form, key())].initialized);
}
fn emit(
    s: &mut ShellState,
    kind: &str,
    dom: &str,
    local: Option<&str>,
    callback: bool,
    value: Option<&str>,
    bad: bool,
    step: bool,
) -> Option<u64> {
    let b = binding(s);
    let m = &s.app_home.text.mounts[&(b.form.clone(), key())];
    let e = Occurrence {
        mount: m.id,
        seq: m.seq + 1,
        kind: kind.into(),
        value: dom.into(),
        key: (kind == "enter").then(|| "Enter".into()),
        code: None,
        ctrl: false,
        alt: false,
        shift: false,
        meta: false,
        repeat: false,
        composing: false,
        trusted: false,
        prevented: kind == "enter",
        email: None,
        number: Some(NumericOccurrence {
            local: local.map(str::to_owned),
            callback,
            callback_value: value.map(str::to_owned),
            bad_input: bad,
            composing: false,
            valid: !bad && !step,
            step_mismatch: step,
            range_underflow: false,
            range_overflow: false,
        }),
    };
    super::super::receive(s, &b, &e)
}
fn parent(s: &ShellState) -> Value {
    form(s).unwrap().state().unwrap()["values"]["amount"]["value"].clone()
}
fn queued(s: &ShellState) -> Queued {
    s.app_home.text.queued[&(owner(s).unwrap(), key())].clone()
}
fn refresh(s: &mut ShellState, initial: &str) {
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"][0]["element"]["initial_value"] = json!(initial);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    super::super::super::hydrate(s);
}
#[test]
fn numeric_literal_parent_and_event_have_independent_custody() {
    let mut s = setup(true, false);
    emit(
        &mut s,
        "input",
        "002.50",
        Some("002.50"),
        true,
        Some("2.5"),
        false,
        true,
    )
    .unwrap();
    assert_eq!(parent(&s)["value"], "2.5");
    assert!(super::super::ready(&s));
    assert_eq!(proof(&s)["numberWidgets"][0]["local"], "002.50");
    assert_eq!(
        bridge::expected_visit(&s, binding(&s).visit).unwrap()["fields"][0]["value"],
        "002.50"
    );
    emit(
        &mut s,
        "enter",
        "002.50",
        Some("002.50"),
        true,
        Some("002.50"),
        false,
        true,
    )
    .unwrap();
    assert_eq!(parent(&s)["value"], "002.50");
    let q = queued(&s);
    let h = s.app_home.surface.as_ref().unwrap();
    let r = q
        .action
        .prepare_with_context(
            &q.binding.owner.ctx.client,
            &q.binding.owner.ctx.session,
            h.view.as_ref().unwrap(),
            form(&s).unwrap(),
            "number-test",
            q.route.team_id(),
        )
        .unwrap();
    let RequestBody::Form(fields) = r.body else {
        panic!("form")
    };
    let fields: BTreeMap<_, _> = fields.into_iter().collect();
    let action: Value = serde_json::from_str(&fields["actions"]).unwrap();
    assert!(action[0].get("initial_value").is_none());
    assert_eq!(action[0]["value"], "002.50");
}
#[test]
fn rejected_local_decimal_can_commit_parent_before_local_restoration() {
    let mut s = setup(true, false);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"][0]["element"]["is_decimal_allowed"] = json!(false);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    super::super::super::hydrate(&mut s);
    mount(&mut s);
    // JS acceptance/rejection itself is tested against the real JS model.
    emit(
        &mut s,
        "input",
        "1.20",
        Some("1.20"),
        true,
        Some("1.5"),
        false,
        true,
    )
    .unwrap();
    assert_eq!(parent(&s)["value"], "1.5");
    assert_eq!(proof(&s)["numberMounts"][0]["value"], "1.20");
    assert!(super::super::ready(&s));
}
#[test]
fn required_empty_keeps_old_action_with_latest_null_parent() {
    let mut s = setup(false, false);
    let id = emit(
        &mut s,
        "input",
        "2",
        Some("2"),
        true,
        Some("2"),
        false,
        false,
    )
    .unwrap();
    assert!(emit(&mut s, "input", "", Some(""), true, None, false, false).is_none());
    assert_eq!(queued(&s).id, id);
    assert!(queued(&s).current(&s));
    assert_eq!(parent(&s)["value"], Value::Null);
    assert!(form(&s).unwrap().fields[&key()].error.is_some());
    assert!(super::super::ready(&s));
}
#[test]
fn mounted_props_keep_local_and_pristine_parent_but_retire_old_action() {
    let mut s = setup(true, false);
    refresh(&mut s, "9");
    assert_eq!(proof(&s)["numberWidgets"][0]["local"], "1.20");
    assert_eq!(parent(&s)["value"], "1.20");
    mount(&mut s);
    emit(
        &mut s,
        "input",
        "2.50",
        Some("2.50"),
        true,
        Some("2.5"),
        false,
        true,
    )
    .unwrap();
    let old = queued(&s);
    refresh(&mut s, "8");
    assert!(!old.current(&s));
    mount(&mut s);
    assert_eq!(proof(&s)["numberWidgets"][0]["local"], "2.50");
    assert_eq!(parent(&s)["value"], "2.5");
    s.app_home.surface.as_mut().unwrap().id = next();
    super::super::super::hydrate(&mut s);
    mount(&mut s);
    assert_eq!(proof(&s)["numberWidgets"][0]["local"], "2.5");
}
#[test]
fn reset_reseeds_and_spends_owned_mount_queue() {
    let mut s = setup(true, false);
    emit(
        &mut s,
        "input",
        "2",
        Some("2"),
        true,
        Some("2"),
        false,
        false,
    )
    .unwrap();
    let old = queued(&s);
    let o = owner(&s).unwrap();
    reset(&mut s, &o);
    assert!(!old.current(&s));
    assert!(s.app_home.text.mounts.is_empty());
    assert!(s.app_home.text.queued.is_empty());
    let raw = form(&s).unwrap().raw_view().clone();
    s.app_home.forms.insert(o, HomeForm::parse(&raw).unwrap());
    hydrate(&mut s);
    mount(&mut s);
    assert_eq!(proof(&s)["numberWidgets"][0]["local"], "1.20");
    assert!(super::super::ready(&s));
}
#[test]
fn bad_input_refuses_custody_without_minting_clear_and_valid_observation_recovers() {
    let mut s = setup(true, false);
    let before = parent(&s);
    assert!(emit(&mut s, "input", "", Some("1.20"), false, None, true, false).is_none());
    assert!(!super::super::ready(&s));
    assert!(error(&s, &key()).is_some());
    assert_eq!(parent(&s), before);
    assert!(
        emit(
            &mut s,
            "input",
            "1.20",
            Some("1.20"),
            false,
            None,
            false,
            true
        )
        .is_none()
    );
    assert!(super::super::ready(&s));
    assert!(error(&s, &key()).is_none());
    assert_eq!(parent(&s), before);
}
#[test]
fn quiet_number_is_in_button_inventory_without_action_and_context_spends_authority() {
    let mut s = setup(true, true);
    assert!(button::needed(&s));
    assert!(
        emit(
            &mut s,
            "input",
            "2.00",
            Some("2.00"),
            true,
            Some("2"),
            false,
            false
        )
        .is_none()
    );
    assert_eq!(parent(&s)["value"], "2");
    assert!(s.app_home.text.queued.is_empty());
    let b = binding(&s);
    let expected = bridge::expected_visit(&s, b.visit).unwrap();
    assert_eq!(expected["fields"][0]["number"], true);
    s.core.account_epoch += 1;
    assert!(!b.current(&s));
    assert!(bridge::expected_visit(&s, b.visit).is_none());
}
