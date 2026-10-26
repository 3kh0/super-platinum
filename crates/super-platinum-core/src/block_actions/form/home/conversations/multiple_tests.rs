//! Retained QjSI/bgmB/hvzX Home input contract; no directory authority inferred.
use super::*;

fn multi(optional: bool) -> Value {
    let mut source = raw(optional);
    source["blocks"][0]["element"] = json!({
        "type":"multi_conversations_select", "action_id":"pick",
        "initial_conversations":["CINITIAL1"], "max_selected_items":2,
        "response_url_enabled":true, "opaque":{"keep":[1,2]}
    });
    source
}

fn ids(ids: &[&str]) -> InputValue {
    InputValue::Conversations(ids.iter().map(|id| (*id).to_owned()).collect())
}

#[test]
fn ordered_multi_state_enters_explicit_button_without_response_url() {
    let raw = multi(false);
    let view = view(raw.clone());
    let mut form = HomeForm::parse(&raw).unwrap();
    assert_eq!(form.dispatch(&key()), Some(Dispatch::Quiet));
    assert!(form.conversation_field(&key()).is_ok());
    assert!(form.user_field(&key()).is_err());
    assert!(form.external_field(&key()).is_err());
    form.set(&key(), ids(&["UMEMBER02", "CINITIAL1"])).unwrap();
    assert_eq!(
        selected(&form),
        json!({"type":"multi_conversations_select",
        "selected_conversations":["UMEMBER02","CINITIAL1"]})
    );
    for (context, team) in [(None, "T2"), (Some("TCONTEXT1"), "TCONTEXT1")] {
        let wire = action(&view, &form, context);
        let state: Value = serde_json::from_str(&wire["state"]).unwrap();
        assert_eq!(state["values"]["channel"]["pick"], selected(&form));
        assert_eq!(wire["service_team_id"], team);
        let actions: Value = serde_json::from_str(&wire["actions"]).unwrap();
        assert_eq!(actions.as_array().unwrap().len(), 1);
        assert_eq!(actions[0]["type"], "button");
    }
    assert_eq!(form.raw_view(), &raw);
}

#[test]
fn required_empty_and_overmax_remain_committed_but_have_validation_feedback() {
    for optional in [false, true] {
        let raw = multi(optional);
        let view = view(raw.clone());
        let mut form = HomeForm::parse(&raw).unwrap();
        for selected_ids in [vec![], vec!["CINITIAL1", "UMEMBER02", "GMPIM0003"]] {
            form.set(&key(), ids(&selected_ids)).unwrap();
            assert_eq!(
                form.fields[&key()].error.is_some(),
                !optional || !selected_ids.is_empty()
            );
            assert_eq!(
                selected(&form)["selected_conversations"],
                json!(selected_ids)
            );
            let wire = action(&view, &form, None);
            let state: Value = serde_json::from_str(&wire["state"]).unwrap();
            assert_eq!(
                state["values"]["channel"]["pick"]["selected_conversations"],
                json!(selected_ids)
            );
        }
    }
    let field = parse_message_conversations(&multi(false)["blocks"][0]["element"]).unwrap();
    assert!(
        validate_message_conversations(
            &field,
            &["CINITIAL1".into(), "UMEMBER02".into(), "GMPIM0003".into()]
        )
        .is_err()
    );
    let mut required = field;
    required.optional = false;
    assert!(validate_message_conversations(&required, &[]).is_err());
}

#[test]
fn initial_overmax_and_reduced_max_retain_exact_parent_array() {
    let mut raw = multi(false);
    raw["blocks"][0]["element"]["initial_conversations"] =
        json!(["GMPIM0003", "CINITIAL1", "UMEMBER02"]);
    let mut form = HomeForm::parse(&raw).unwrap();
    assert!(form.fields[&key()].error.is_some());
    let before = selected(&form);
    raw["hash"] = json!("two");
    raw["blocks"][0]["element"]["max_selected_items"] = json!(1);
    form.reconcile(&raw).unwrap();
    assert_eq!(selected(&form), before);
    assert!(form.fields[&key()].error.is_some());
    let wire = action(&view(raw), &form, None);
    let state: Value = serde_json::from_str(&wire["state"]).unwrap();
    assert_eq!(state["values"]["channel"]["pick"], before);
}

#[test]
fn compatible_saved_hydration_and_dirty_reconcile_are_preserved() {
    // Existing Home compatibility behavior; no new upstream ingress claim.
    for saved in [
        json!({"type":"multi_conversations_select"}),
        json!({"type":"multi_conversations_select","selected_conversations":null}),
    ] {
        let mut raw = multi(true);
        raw["state"] = json!({"values":{"channel":{"pick":saved}}});
        let form = HomeForm::parse(&raw).unwrap();
        assert_eq!(
            selected(&form)["selected_conversations"],
            json!(["CINITIAL1"])
        );
    }
    let mut raw = multi(true);
    raw["state"] = json!({"values":{"channel":{"pick":{"type":"multi_conversations_select","selected_conversations":[]}}}});
    let mut form = HomeForm::parse(&raw).unwrap();
    assert_eq!(selected(&form)["selected_conversations"], json!([]));
    form.set(&key(), ids(&["UMEMBER02", "CINITIAL1"])).unwrap();
    raw["hash"] = json!("two");
    raw["blocks"][0]["element"]["initial_conversations"] = json!(["GMPIM0003"]);
    raw["state"]["values"]["channel"]["pick"]["selected_conversations"] = json!(["CSAVED001"]);
    form.reconcile(&raw).unwrap();
    assert_eq!(
        selected(&form)["selected_conversations"],
        json!(["UMEMBER02", "CINITIAL1"])
    );
    assert_eq!(
        selected(&HomeForm::parse(&raw).unwrap())["selected_conversations"],
        json!(["CSAVED001"])
    );
    // The independent z+fW local initial display effect is renderer-owned.
}

#[test]
fn multi_structural_id_duplicate_type_and_size_guards_remain() {
    for bad in [
        vec!["CINITIAL1", "CINITIAL1"],
        vec![""],
        vec!["TTEAM0001"],
        vec!["Cbad"],
        vec!["C1"],
    ] {
        let mut form = HomeForm::parse(&multi(true)).unwrap();
        form.set(&key(), ids(&bad)).unwrap();
        assert!(form.state().is_err());
    }
    let mut form = HomeForm::parse(&multi(true)).unwrap();
    assert!(form.set(&key(), InputValue::Conversation(None)).is_err());
    form.set(
        &key(),
        InputValue::Conversations((0..1001).map(|i| format!("C{i:08}")).collect()),
    )
    .unwrap();
    assert!(form.state().is_err());
    for bad in [json!("CINITIAL1"), json!([null]), json!([1])] {
        let mut raw = multi(true);
        raw["state"] = json!({"values":{"channel":{"pick":{"type":"multi_conversations_select","selected_conversations":bad}}}});
        assert!(HomeForm::parse(&raw).is_err());
    }
}

#[test]
fn multi_requires_exact_input_without_injected_authority() {
    for flag in [
        "confirm",
        "dispatch_action_config",
        "visible_to_user_ids",
        "show_enterprise_options",
        "service_team_id",
        "container_actions_payload",
    ] {
        let mut raw = multi(true);
        raw["blocks"][0]["element"][flag] = json!({});
        if let Ok(form) = HomeForm::parse(&raw) {
            assert!(form.conversation_field(&key()).is_err());
        }
    }
    let mut raw = multi(true);
    raw["blocks"][0]["dispatch_action"] = json!(true);
    let mut dispatching = HomeForm::parse(&raw).unwrap();
    assert!(
        dispatching
            .multi_conversation_input(&key(), vec!["UMEMBER02".into()])
            .unwrap()
            .is_some()
    );
    raw["blocks"][0]["dispatch_action"] = json!(false);
    raw["blocks"][0]["type"] = json!("section");
    raw["blocks"][0]["accessory"] = raw["blocks"][0]["element"].clone();
    assert!(
        HomeForm::parse(&raw)
            .unwrap()
            .conversation_field(&key())
            .is_err()
    );
    let mut form = HomeForm::parse(&multi(true)).unwrap();
    form.fields.get_mut(&key()).unwrap().raw["initial_conversations"] = json!([]);
    assert!(form.conversation_field(&key()).is_err());
}

#[test]
fn home_default_flag_is_ignored_without_changing_modal_origin_resolution() {
    for flag in [false, true] {
        let mut raw = multi(true);
        let element = &mut raw["blocks"][0]["element"];
        element
            .as_object_mut()
            .unwrap()
            .remove("initial_conversations");
        element["default_to_current_conversation"] = json!(flag);
        element["filter"] = json!({"include":["private","im"],"only_from_team_id":"EENTER001"});
        let form = HomeForm::parse(&raw).unwrap();
        // The composed Home initializer keeps absent initial selection omitted.
        assert_eq!(
            selected(&form),
            json!({"type":"multi_conversations_select"})
        );
        assert_eq!(
            form.conversation_field(&key()).unwrap().raw,
            raw["blocks"][0]["element"]
        );
        let (modal_kind, _) = parse::input(&raw["blocks"][0]["element"], true, None, None).unwrap();
        assert_eq!(matches!(modal_kind, InputKind::Unsupported { .. }), flag);
    }
}
