use super::*;
use serde_json::json;

fn scope() -> ModalScope {
    ModalScope {
        account_epoch: 1,
        team: "T1".into(),
        realtime_generation: 7,
    }
}
fn key(id: &str) -> InputKey {
    InputKey {
        block_id: id.into(),
        action_id: "value".into(),
    }
}
fn text(id: &str) -> Value {
    json!({"type":"input","block_id":id,"label":{"type":"plain_text","text":"Name"},
        "element":{"type":"plain_text_input","action_id":"value","initial_value":"kept"}})
}
fn number(id: &str) -> Value {
    json!({"type":"input","block_id":id,"label":{"type":"plain_text","text":"Adjustment"},
        "element":{"type":"number_input","action_id":"value","is_decimal_allowed":false,"initial_value":"-1"}})
}
fn view(blocks: Vec<Value>) -> Value {
    json!({"id":"V1","type":"modal","app_id":"A1","title":{"type":"plain_text","text":"Form"},
        "submit":{"type":"plain_text","text":"Save"},"blocks":blocks})
}
fn frame(kind: &str, payload: Value) -> RawEvent {
    serde_json::from_value(json!({"type":kind,"view_id":payload["id"],"app_id":"A1",
        "view_type":"modal","client_token":"token","view":payload}))
    .unwrap()
}
fn root() -> (ModalState, ModalForm) {
    let mut state = ModalState::new(scope());
    assert!(state.enqueue(&scope(), "token".into(), "A1".into(), 0, 60_000));
    let payload = view(vec![text("A")]);
    assert_eq!(
        state.apply_realtime_integer_context(&scope(), &frame("view_opened", payload.clone()), 1),
        ModalChange::Opened
    );
    assert!(state.integer_introduction_context_live());
    let form = state.parse_integer_root_form().unwrap();
    assert!(
        state.parse_integer_root_form().is_err(),
        "root custody is issued once"
    );
    (state, form)
}
fn update(state: &mut ModalState, form: &mut ModalForm, payload: Value) -> Result<(), FormError> {
    state.capture_integer_drafts([&*form]);
    assert_eq!(
        state.apply_realtime_integer_context(&scope(), &frame("view_updated", payload), 2),
        ModalChange::Updated
    );
    state.reconcile_integer_form(form)
}
fn introduced() -> (ModalState, ModalForm) {
    let (mut state, mut form) = root();
    update(&mut state, &mut form, view(vec![text("A"), number("B")])).unwrap();
    assert_eq!(
        form.submission().unwrap()["values"]["B"]["value"]["value"],
        "-1"
    );
    (state, form)
}

#[test]
fn accepted_new_key_keeps_initial_provenance_separate_and_refreshes_only_current_binding() {
    let (mut state, mut form) = introduced();
    let mut stale = form.clone();
    let payload = form.raw_view.clone();
    update(&mut state, &mut form, payload).unwrap();
    assert!(stale.submission().is_err());
    assert!(
        state.reconcile_integer_form(&mut stale).is_err(),
        "a stale clone cannot regain refresh custody"
    );
    assert!(stale.submission().is_err());
    assert_eq!(
        form.submission().unwrap()["values"]["B"]["value"]["value"],
        "-1"
    );
    assert!(form.has_introduced_integers());
    let unchanged = form.clone();
    assert!(form.reconcile(&form.raw_view.clone()).is_err());
    assert_eq!(form, unchanged);
    assert!(form.reconcile_submitted(&form.raw_view.clone()).is_err());
    form.set_value(&key("B"), InputValue::Number(Some("-2".into())))
        .unwrap();
    assert_eq!(
        form.submission().unwrap()["values"]["B"]["value"]["value"],
        "-2"
    );
    form.set_value(&key("B"), InputValue::Number(None)).unwrap();
    assert!(matches!(form.submission(), Err(FormError::Validation(_))));
    form.set_value(&key("B"), InputValue::Number(Some("3".into())))
        .unwrap();
    drop(state);
    assert!(form.submission().is_err());

    let (state, form) = introduced();
    let temporary_upgrade = state.integer_owner.as_ref().unwrap().cell.clone();
    drop(state);
    assert_eq!(
        temporary_upgrade.load(Ordering::Acquire),
        0,
        "disposal revokes even while an upgrade retains the allocation"
    );
    assert!(form.submission().is_err());
}

#[test]
fn failed_or_unsupported_keys_and_history_exhaustion_never_rearm() {
    let (mut state, mut form) = root();
    let mut excluded = text("A");
    excluded["dispatch_action"] = json!(false);
    update(&mut state, &mut form, view(vec![excluded, number("B")])).unwrap();
    assert!(!state.integer_introduction_context_live());
    assert!(!form.has_introduced_integers());
    assert!(form.submission().is_err());

    let (mut state, mut form) = root();
    let mut bad = number("B");
    bad["label"] = json!({"type":"mrkdwn","text":"not a plain label"});
    assert!(update(&mut state, &mut form, view(vec![text("A"), bad])).is_err());
    assert!(state.integer_owner.as_ref().unwrap().seen.contains("B"));
    assert!(!state.integer_introduction_context_live());
    update(&mut state, &mut form, view(vec![text("A"), number("B")])).unwrap();
    assert!(!form.has_introduced_integers());
    assert!(form.submission().is_err());

    for removed in [false, true] {
        let (mut state, mut form) = root();
        update(&mut state, &mut form, view(vec![text("A"), text("B")])).unwrap();
        if removed {
            update(&mut state, &mut form, view(vec![text("A")])).unwrap();
        }
        update(&mut state, &mut form, view(vec![text("A"), number("B")])).unwrap();
        assert!(!form.has_introduced_integers());
        assert!(form.submission().is_err());
    }

    let (mut state, _) = root();
    for index in 0..HISTORY_LIMIT - 1 {
        assert_eq!(
            state.apply_realtime_integer_context(
                &scope(),
                &frame("view_updated", view(vec![text(&format!("seen{index}"))])),
                3
            ),
            ModalChange::Updated
        );
    }
    assert!(state.integer_introduction_context_live());
    assert_eq!(
        state.integer_owner.as_ref().unwrap().seen.len(),
        HISTORY_LIMIT
    );
    state.apply_realtime_integer_context(
        &scope(),
        &frame("view_updated", view(vec![text("overflow")])),
        4,
    );
    assert!(!state.integer_introduction_context_live());
    assert_eq!(
        state.integer_owner.as_ref().unwrap().seen.len(),
        HISTORY_LIMIT
    );
    state.apply_realtime_integer_context(
        &scope(),
        &frame("view_updated", view(vec![number("brand-new")])),
        5,
    );
    assert!(!state.integer_introduction_context_live());
}

#[test]
fn alternate_writers_revoke_existing_grants_and_keep_removed_pristine_drafts() {
    let (mut state, mut form) = introduced();
    state.capture_integer_drafts([&form]);
    form.set_value(&key("B"), InputValue::Number(Some("23".into())))
        .unwrap();
    let mut replacement = text("B");
    replacement["element"]["initial_value"] = json!("replacement");
    // Skip the helper's active capture: first terminal capture must keep 23,
    // rather than freezing the previously captured pristine -1.
    state.apply_realtime_integer_context(
        &scope(),
        &frame("view_updated", view(vec![text("A"), replacement.clone()])),
        2,
    );
    state.reconcile_integer_form(&mut form).unwrap();
    assert_eq!(
        form.fields[&key("B")].value,
        InputValue::Text("replacement".into())
    );
    state.capture_integer_drafts([&form]);
    replacement["element"]["initial_value"] = json!("later replacement");
    update(&mut state, &mut form, view(vec![text("A"), replacement])).unwrap();
    let mut exported = BTreeMap::new();
    form.capture_introduced_values(&mut exported);
    assert_eq!(
        exported.get(&key("B")),
        Some(&InputValue::Number(Some("23".into())))
    );
    assert_eq!(
        state.integer_owner.as_ref().unwrap().retired.get(&key("B")),
        exported.get(&key("B"))
    );
    assert!(form.submission().is_err());
    form.set_value(&key("B"), InputValue::Text("edited replacement".into()))
        .unwrap();
    let raw = form.raw_view.clone();
    assert!(update(&mut state, &mut form, view(vec![text("A")])).is_err());
    assert_eq!(form.raw_view, raw);
    assert_eq!(
        form.fields[&key("B")].value,
        InputValue::Text("edited replacement".into())
    );
    let mut after_conflict = BTreeMap::new();
    form.capture_introduced_values(&mut after_conflict);
    assert_eq!(after_conflict, exported);
    assert_eq!(
        state.integer_owner.as_ref().unwrap().retired.get(&key("B")),
        exported.get(&key("B"))
    );

    for case in 0..12 {
        let (mut state, mut form) = introduced();
        let stale = form.clone();
        let payload = form.raw_view.clone();
        state.capture_integer_drafts([&form]);
        match case {
            0 => {
                state.apply(&scope(), &frame("view_updated", payload), 3);
            }
            1 => {
                assert!(state.hydrate_current(&scope(), "V1", state.revision(), payload));
            }
            2 => {
                assert!(state.apply_submission(
                    &scope(),
                    "V1",
                    state.revision(),
                    &BlockViewResponseAction::Update(payload)
                ));
            }
            3 => {
                assert!(state.pop_local(&scope()));
            }
            4 => {
                state.clear_local(&scope());
            }
            5 => {
                let mut replacement = scope();
                replacement.account_epoch += 1;
                state.reset_scope(replacement);
            }
            6 => {
                assert!(state.enqueue(&scope(), "child".into(), "A1".into(), 3, 60_000));
                let mut child = view(vec![]);
                child["id"] = json!("V2");
                child["previous_view_id"] = json!("V1");
                child["root_view_id"] = json!("V1");
                let event: RawEvent = serde_json::from_value(json!({"type":"view_opened","view_id":"V2","app_id":"A1",
                    "view_type":"modal","previous_view_id":"V1","client_token":"child","view":child})).unwrap();
                assert_eq!(
                    state.apply_realtime_integer_context(&scope(), &event, 4),
                    ModalChange::Opened
                );
                let child_form = ModalForm::parse_with_signed_integer_context(
                    state.stack()[1].payload.as_ref().unwrap(),
                    None,
                    None,
                )
                .unwrap();
                assert!(child_form.submission().is_ok());
                let revision = state.revision();
                state.apply_realtime_integer_context(
                    &scope(),
                    &frame("view_updated", form.raw_view.clone()),
                    5,
                );
                assert_eq!(
                    state.revision(),
                    revision,
                    "hidden parent does not retire child request identity"
                );
                assert!(state.pop_local(&scope()));
            }
            7 => {
                let event: RawEvent = serde_json::from_value(
                    json!({"type":"view_updated","view_id":"V1","app_id":"A1"}),
                )
                .unwrap();
                state.apply_realtime_integer_context(&scope(), &event, 3);
            }
            8 => {
                let mut payload = payload;
                payload["blocks"][1]["label"]["text"] = json!("Changed label");
                state.apply_realtime_integer_context(&scope(), &frame("view_updated", payload), 3);
            }
            9 => {
                state.apply_realtime_integer_context(
                    &scope(),
                    &frame("view_updated", view(vec![text("A")])),
                    3,
                );
            }
            10 => {
                let mut payload = payload;
                payload["state"] =
                    json!({"values":{"B":{"value":{"type":"number_input","value":"-1"}}}});
                state.apply_realtime_integer_context(&scope(), &frame("view_updated", payload), 3);
            }
            _ => {
                let mut payload = payload;
                payload["visible_to_user_ids"] = json!([]);
                state.apply_realtime_integer_context(&scope(), &frame("view_updated", payload), 3);
            }
        }
        assert!(!state.integer_introduction_context_live(), "case={case}");
        assert!(stale.submission().is_err(), "case={case}");
        let _ = state.reconcile_integer_form(&mut form);
        assert!(form.submission().is_err(), "case={case}");
        assert!(
            form.has_introduced_integers(),
            "retired provenance is never erased"
        );
        let owner = state.integer_owner.as_ref().unwrap();
        assert_eq!(
            owner.retired.get(&key("B")),
            Some(&InputValue::Number(Some("-1".into())))
        );
        if case == 9 {
            assert!(!form.fields.contains_key(&key("B")));
            assert!(form.unsupported.is_empty());
            assert!(form.reconcile(&view(vec![text("A")])).is_err());
        }
    }
}

#[test]
fn ignored_frames_preserve_binding_but_consumption_disposal_and_epoch_exhaustion_are_terminal() {
    let (state, form) = root();
    assert!(state.consume_integer_submission(&form));
    assert!(!state.integer_introduction_context_live());
    assert!(
        form.submission().is_ok(),
        "initial-only forms retain their ordinary submission policy"
    );

    let (mut state, form) = introduced();
    let mut wrong = scope();
    wrong.realtime_generation += 1;
    assert_eq!(
        state.apply_realtime_integer_context(
            &wrong,
            &frame("view_updated", form.raw_view.clone()),
            3
        ),
        ModalChange::Ignored
    );
    assert_eq!(
        state.apply(&wrong, &frame("view_updated", form.raw_view.clone()), 3),
        ModalChange::Ignored
    );
    assert!(!state.hydrate_current(&scope(), "V1", state.revision() + 1, form.raw_view.clone()));
    assert!(form.submission().is_ok());
    let revision = state.revision();
    assert!(state.consume_integer_submission(&form));
    assert_eq!(
        state.revision(),
        revision,
        "first prepared request retains original revision"
    );
    assert!(!state.consume_integer_submission(&form));
    assert!(form.submission().is_err());
    assert!(state.apply_submission(
        &scope(),
        "V1",
        revision,
        &BlockViewResponseAction::Update(view(vec![text("A")]))
    ));
    assert!(!state.integer_introduction_context_live());

    let (mut state, form) = introduced();
    state
        .integer_owner
        .as_ref()
        .unwrap()
        .cell
        .store(u64::MAX, Ordering::Release);
    state.apply_realtime_integer_context(
        &scope(),
        &frame("view_updated", form.raw_view.clone()),
        3,
    );
    assert_eq!(
        state
            .integer_owner
            .as_ref()
            .unwrap()
            .cell
            .load(Ordering::Acquire),
        0
    );
    assert!(form.submission().is_err());
}
