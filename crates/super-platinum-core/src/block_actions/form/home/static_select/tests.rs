use super::*;

fn key() -> InputKey {
    InputKey {
        block_id: "choice".into(),
        action_id: "pick".into(),
    }
}
fn option(value: &str) -> Value {
    json!({"text":{"type":"plain_text","text":value,"emoji":true},"value":value,
        "description":{"type":"plain_text","text":"Description"},"opaque":{"kept":value}})
}
fn raw(optional: bool) -> Value {
    json!({"id":"V1","type":"home","blocks":[{"type":"input","block_id":"choice",
        "dispatch_action":true,"optional":optional,"label":{"type":"plain_text","text":"Choice"},
        "element":{"type":"static_select","action_id":"pick","options":[option("a"),option("b")],
        "initial_option":option("a"),"placeholder":{"type":"plain_text","text":"Choose"}}}]})
}
fn state(form: &HomeForm) -> Value {
    form.state().unwrap()["values"]["choice"]["pick"].clone()
}

#[test]
fn selection_keeps_exact_option_and_callback_props_but_state_excludes_metadata() {
    let view = raw(false);
    let mut form = HomeForm::parse(&view).unwrap();
    let event = form
        .static_input(&key(), Some("b".into()))
        .unwrap()
        .unwrap();
    assert_eq!(
        event.action(),
        &json!({"type":"static_select","action_id":"pick","block_id":"choice",
        "selected_option":option("b"),"initial_option":option("a"),"placeholder":{"type":"plain_text","text":"Choose"}})
    );
    assert_eq!(
        state(&form),
        json!({"type":"static_select","selected_option":option("b")})
    );
    assert!(event.same_view(&view));
    assert_eq!(event.key(), &key());
    assert_eq!(
        form.static_input(&key(), Some("b".into()))
            .unwrap()
            .unwrap(),
        event,
        "Repeated callbacks are not deduplicated by value"
    );
}

#[test]
fn optional_clear_dispatches_but_required_clear_stores_state_before_suppression() {
    for optional in [true, false] {
        let mut form = HomeForm::parse(&raw(optional)).unwrap();
        let queued = form
            .static_input(&key(), Some("b".into()))
            .unwrap()
            .unwrap();
        let clear = form.static_input(&key(), None).unwrap();
        assert_eq!(clear.is_some(), optional);
        if let Some(event) = clear {
            assert!(event.action()["selected_option"].is_null());
        }
        assert_eq!(
            state(&form),
            json!({"type":"static_select","selected_option":null})
        );
        assert_eq!(form.fields[&key()].error.is_some(), !optional);
        assert_eq!(queued.action()["selected_option"], option("b"));
    }
}

#[test]
fn empty_option_value_resolves_to_null_instead_of_a_selected_option() {
    for optional in [true, false] {
        let mut view = raw(optional);
        view["blocks"][0]["element"]["options"][1] = option("");
        let mut form = HomeForm::parse(&view).unwrap();
        let event = form.static_input(&key(), Some(String::new())).unwrap();
        assert_eq!(event.is_some(), optional);
        if let Some(event) = event {
            assert!(event.action()["selected_option"].is_null());
        }
        assert!(state(&form)["selected_option"].is_null());
    }
}

#[test]
fn quiet_selection_updates_only_state_and_groups_resolve_original_options() {
    let mut view = raw(true);
    view["blocks"][0]["dispatch_action"] = json!(false);
    let element = view["blocks"][0]["element"].as_object_mut().unwrap();
    element.remove("options");
    element.insert(
        "option_groups".into(),
        json!([{"label":{"type":"plain_text","text":"Group"},"options":[option("a"),option("b")]}]),
    );
    let mut form = HomeForm::parse(&view).unwrap();
    assert!(
        form.static_input(&key(), Some("b".into()))
            .unwrap()
            .is_none()
    );
    assert_eq!(state(&form)["selected_option"], option("b"));
    assert!(form.static_input(&key(), None).unwrap().is_none());
    assert!(state(&form)["selected_option"].is_null());
}

#[test]
fn unknown_option_or_changed_source_cannot_mint_an_event_or_overwrite_state() {
    let mut form = HomeForm::parse(&raw(true)).unwrap();
    let before = form.clone();
    assert!(form.static_input(&key(), Some("foreign".into())).is_err());
    assert_eq!(form, before);
    form.fields.get_mut(&key()).unwrap().raw["options"][0]["value"] = json!("forged");
    assert!(form.static_input(&key(), Some("a".into())).is_err());
    let mut form = HomeForm::parse(&raw(true)).unwrap();
    let InputKind::StaticSelect { options, .. } = &mut form.fields.get_mut(&key()).unwrap().kind
    else {
        panic!()
    };
    options[0].raw["opaque"] = json!("forged");
    assert!(form.static_input(&key(), Some("a".into())).is_err());
}

#[test]
fn confirmation_and_visibility_stay_explicitly_unsupported() {
    for variant in 1..3 {
        let mut view = raw(true);
        match variant {
            1 => {
                view["blocks"][0]["element"]["confirm"] = json!({});
            }
            _ => {
                view["blocks"][0]["element"]["visible_to_user_ids"] = json!(["U1"]);
            }
        }
        let form = HomeForm::parse(&view).unwrap();
        assert!(!form.unsupported.is_empty());
        assert!(form.static_field(&key()).is_err());
        assert!(form.state().is_err());
    }
}

fn wrapped(parent: &str, optional: bool) -> Value {
    let mut view = raw(optional);
    let element = view["blocks"][0]["element"].clone();
    view["blocks"][0] = if parent == "actions" {
        json!({"type":"actions","block_id":"choice","optional":optional,"elements":[
            {"type":"button","action_id":"submit","text":{"type":"plain_text","text":"Send"}},element]})
    } else {
        json!({"type":"section","block_id":"choice","optional":optional,
            "text":{"type":"mrkdwn","text":"Release"},"accessory":element})
    };
    view
}

#[test]
fn stateful_wrappers_dispatch_selection_and_null_without_input_required_errors() {
    for parent in ["actions", "section"] {
        let mut form = HomeForm::parse(&wrapped(parent, false)).unwrap();
        assert!(form.unsupported.is_empty());
        assert_eq!(form.dispatch(&key()), Some(Dispatch::Change));
        let event = form
            .static_input(&key(), Some("b".into()))
            .unwrap()
            .unwrap();
        assert_eq!(event.action()["selected_option"], option("b"));
        assert_eq!(event.action()["initial_option"], option("a"));
        assert_eq!(state(&form)["selected_option"], option("b"));
        let event = form.static_input(&key(), None).unwrap().unwrap();
        assert!(event.action()["selected_option"].is_null());
        assert!(state(&form)["selected_option"].is_null());
        assert!(form.fields[&key()].error.is_none());
    }
}

#[test]
fn wrapper_clear_affordance_is_distinct_from_null_callback_support() {
    for optional in [false, true] {
        assert_eq!(
            HomeForm::parse(&raw(optional))
                .unwrap()
                .static_allows_clear(&key())
                .unwrap(),
            optional
        );
        for parent in ["actions", "section"] {
            let mut view = wrapped(parent, optional);
            let element = if parent == "actions" {
                &mut view["blocks"][0]["elements"][1]
            } else {
                &mut view["blocks"][0]["accessory"]
            };
            element["options"][1] = option("");
            let mut form = HomeForm::parse(&view).unwrap();
            assert!(!form.static_allows_clear(&key()).unwrap());
            let event = form
                .static_input(&key(), Some(String::new()))
                .unwrap()
                .unwrap();
            assert!(event.action()["selected_option"].is_null());
            assert!(state(&form)["selected_option"].is_null());
            assert!(form.fields[&key()].error.is_none());
        }
    }
}

#[test]
fn actions_resolve_the_exact_element_and_reject_forged_or_duplicate_identity() {
    let mut view = wrapped("actions", false);
    let mut second = view["blocks"][0]["elements"][1].clone();
    second["action_id"] = json!("second");
    second["options"] = json!([option("x")]);
    second["initial_option"] = option("x");
    view["blocks"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(second.clone());
    let second_key = InputKey {
        block_id: "choice".into(),
        action_id: "second".into(),
    };
    let mut form = HomeForm::parse(&view).unwrap();
    let before = form.clone();
    assert!(form.static_input(&key(), Some("x".into())).is_err());
    assert_eq!(form, before);
    assert_eq!(
        form.static_input(&second_key, Some("x".into()))
            .unwrap()
            .unwrap()
            .action()["selected_option"],
        option("x")
    );
    form.fields.get_mut(&key()).unwrap().raw = second;
    assert!(form.static_field(&key()).is_err());
    view["blocks"][0]["elements"][2]["action_id"] = json!("pick");
    assert!(HomeForm::parse(&view).is_err());
}

#[test]
fn stateful_wrapper_empty_initial_and_reconciled_null_are_valid_without_dispatch() {
    for parent in ["actions", "section"] {
        let mut view = wrapped(parent, false);
        let element = if parent == "actions" {
            &mut view["blocks"][0]["elements"][1]
        } else {
            &mut view["blocks"][0]["accessory"]
        };
        element.as_object_mut().unwrap().remove("initial_option");
        let mut form = HomeForm::parse(&view).unwrap();
        assert!(form.fields[&key()].error.is_none());
        assert!(state(&form)["selected_option"].is_null());
        form.static_input(&key(), None).unwrap().unwrap();
        let mut refreshed = wrapped(parent, false);
        refreshed["state"] = json!({"values":{"choice":{"pick":{"type":"static_select","selected_option":option("b")}}}});
        form.reconcile(&refreshed).unwrap();
        assert!(state(&form)["selected_option"].is_null());
        assert!(form.fields[&key()].error.is_none());
        assert!(form.fields[&key()].dirty);
    }
}
