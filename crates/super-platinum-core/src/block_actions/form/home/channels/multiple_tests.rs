//! Retained z+fW/xjrl/hvzX Home input contract; no directory authority inferred.
use super::*;

fn multi(optional: bool) -> Value {
    let mut source = raw(optional);
    source["blocks"][0]["element"] = json!({
        "type":"multi_channels_select", "action_id":"pick",
        "initial_channels":["CINITIAL1"], "max_selected_items":2,
        "response_url_enabled":true, "opaque":{"keep":[1,2]}
    });
    source
}

fn ids(ids: &[&str]) -> InputValue {
    InputValue::Channels(ids.iter().map(|id| (*id).to_owned()).collect())
}

#[test]
fn ordered_multi_state_enters_explicit_button_without_response_url() {
    let raw = multi(false);
    let view = view(raw.clone());
    let mut form = HomeForm::parse(&raw).unwrap();
    assert_eq!(form.dispatch(&key()), Some(Dispatch::Quiet));
    assert!(form.channel_field(&key()).is_ok());
    assert!(form.user_field(&key()).is_err());
    assert!(form.external_field(&key()).is_err());
    form.set(&key(), ids(&["CSELECT02", "CINITIAL1"])).unwrap();
    assert_eq!(
        selected(&form),
        json!({"type":"multi_channels_select",
        "selected_channels":["CSELECT02","CINITIAL1"]})
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
        for selected_ids in [vec![], vec!["CINITIAL1", "CSELECT02", "CTHIRD003"]] {
            form.set(&key(), ids(&selected_ids)).unwrap();
            assert_eq!(
                form.fields[&key()].error.is_some(),
                !optional || !selected_ids.is_empty()
            );
            assert_eq!(selected(&form)["selected_channels"], json!(selected_ids));
            let wire = action(&view, &form, None);
            let state: Value = serde_json::from_str(&wire["state"]).unwrap();
            assert_eq!(
                state["values"]["channel"]["pick"]["selected_channels"],
                json!(selected_ids)
            );
        }
    }
    let field = parse_message_channels(&multi(false)["blocks"][0]["element"]).unwrap();
    assert!(
        validate_message_channels(
            &field,
            &["CINITIAL1".into(), "CSELECT02".into(), "CTHIRD003".into()]
        )
        .is_err()
    );
    let mut required = field;
    required.optional = false;
    assert!(validate_message_channels(&required, &[]).is_err());
}

#[test]
fn initial_overmax_and_reduced_max_retain_exact_parent_array() {
    let mut raw = multi(false);
    raw["blocks"][0]["element"]["initial_channels"] =
        json!(["CTHIRD003", "CINITIAL1", "CSELECT02"]);
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
        json!({"type":"multi_channels_select"}),
        json!({"type":"multi_channels_select","selected_channels":null}),
    ] {
        let mut raw = multi(true);
        raw["state"] = json!({"values":{"channel":{"pick":saved}}});
        let form = HomeForm::parse(&raw).unwrap();
        assert_eq!(selected(&form)["selected_channels"], json!(["CINITIAL1"]));
    }
    let mut raw = multi(true);
    raw["state"] = json!({"values":{"channel":{"pick":{"type":"multi_channels_select","selected_channels":[]}}}});
    let mut form = HomeForm::parse(&raw).unwrap();
    assert_eq!(selected(&form)["selected_channels"], json!([]));
    form.set(&key(), ids(&["CSELECT02", "CINITIAL1"])).unwrap();
    raw["hash"] = json!("two");
    raw["blocks"][0]["element"]["initial_channels"] = json!(["CTHIRD003"]);
    raw["state"]["values"]["channel"]["pick"]["selected_channels"] = json!(["CSAVED001"]);
    form.reconcile(&raw).unwrap();
    assert_eq!(
        selected(&form)["selected_channels"],
        json!(["CSELECT02", "CINITIAL1"])
    );
    assert_eq!(
        selected(&HomeForm::parse(&raw).unwrap())["selected_channels"],
        json!(["CSAVED001"])
    );
    // The independent z+fW local initial display effect is renderer-owned.
}

#[test]
fn multi_structural_id_duplicate_type_and_size_guards_remain() {
    for bad in [
        vec!["CINITIAL1", "CINITIAL1"],
        vec![""],
        vec!["UUSER0001"],
        vec!["Cbad"],
        vec!["C1"],
    ] {
        let mut form = HomeForm::parse(&multi(true)).unwrap();
        form.set(&key(), ids(&bad)).unwrap();
        assert!(form.state().is_err());
    }
    let mut form = HomeForm::parse(&multi(true)).unwrap();
    assert!(form.set(&key(), InputValue::Channel(None)).is_err());
    form.set(
        &key(),
        InputValue::Channels((0..1001).map(|i| format!("C{i:08}")).collect()),
    )
    .unwrap();
    assert!(form.state().is_err());
    for bad in [json!("CINITIAL1"), json!([null]), json!([1])] {
        let mut raw = multi(true);
        raw["state"] = json!({"values":{"channel":{"pick":{"type":"multi_channels_select","selected_channels":bad}}}});
        assert!(HomeForm::parse(&raw).is_err());
    }
}

#[test]
fn multi_requires_exact_input_without_injected_authority() {
    for flag in [
        "confirm",
        "dispatch_action_config",
        "visible_to_user_ids",
        "filter",
        "default_to_current_conversation",
        "show_enterprise_options",
        "service_team_id",
        "container_actions_payload",
    ] {
        let mut raw = multi(true);
        raw["blocks"][0]["element"][flag] = json!({});
        if let Ok(form) = HomeForm::parse(&raw) {
            assert!(form.channel_field(&key()).is_err());
        }
    }
    let mut raw = multi(true);
    raw["blocks"][0]["dispatch_action"] = json!(true);
    assert!(HomeForm::parse(&raw).unwrap().state().is_ok());
    raw["blocks"][0]["dispatch_action"] = json!(false);
    raw["blocks"][0]["type"] = json!("section");
    raw["blocks"][0]["accessory"] = raw["blocks"][0]["element"].clone();
    assert!(
        HomeForm::parse(&raw)
            .unwrap()
            .channel_field(&key())
            .is_err()
    );
    let mut form = HomeForm::parse(&multi(true)).unwrap();
    form.fields.get_mut(&key()).unwrap().raw["initial_channels"] = json!([]);
    assert!(form.channel_field(&key()).is_err());
}
