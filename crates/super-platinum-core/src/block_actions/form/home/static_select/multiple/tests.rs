use super::*;

fn key() -> InputKey {
    InputKey {
        block_id: "choice".into(),
        action_id: "pick".into(),
    }
}
fn option(value: &str) -> Value {
    json!({"text":{"type":"plain_text","text":format!("Option {value}"),"emoji":true},
        "value":value,"description":{"type":"plain_text","text":"Description"},
        "opaque":{"kept":value}})
}
fn raw(optional: bool) -> Value {
    json!({"id":"V1","type":"home","blocks":[{"type":"input","block_id":"choice",
        "dispatch_action":true,"optional":optional,"label":{"type":"plain_text","text":"Choice"},
        "element":{"type":"multi_static_select","action_id":"pick",
        "options":[option("a"),option("b"),option("c"),option("")],"initial_options":[],
        "max_selected_items":2,"placeholder":{"type":"plain_text","text":"Choose"}}}]})
}
fn selected(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}
fn state(form: &HomeForm) -> Value {
    form.state().unwrap()["values"]["choice"]["pick"]["selected_options"].clone()
}

#[test]
fn ordered_callbacks_keep_raw_metadata_and_empty_string_options() {
    let view = raw(true);
    let mut form = HomeForm::parse(&view).unwrap();
    for values in [&["b", "a"][..], &["a"], &["a", "b"], &[""]] {
        let event = form
            .multi_static_input(&key(), selected(values))
            .unwrap()
            .unwrap();
        let options: Vec<_> = values.iter().map(|value| option(value)).collect();
        assert_eq!(
            event.action(),
            &json!({"type":"multi_static_select","block_id":"choice",
            "action_id":"pick","selected_options":options,"initial_options":[],
            "placeholder":{"type":"plain_text","text":"Choose"}})
        );
        assert_eq!(state(&form), json!(options));
        assert!(event.same_view(&view));
        assert_eq!(
            form.multi_static_input(&key(), selected(values))
                .unwrap()
                .unwrap(),
            event
        );
    }
    assert!(form.static_field(&key()).is_err());
    assert!(form.static_input(&key(), Some("a".into())).is_err());
}

#[test]
fn required_empty_and_overmax_commit_before_suppressing_new_events() {
    for optional in [true, false] {
        let mut form = HomeForm::parse(&raw(optional)).unwrap();
        let queued = form
            .multi_static_input(&key(), selected(&["a"]))
            .unwrap()
            .unwrap();
        assert!(
            form.multi_static_input(&key(), selected(&["a", "b", "c"]))
                .unwrap()
                .is_none()
        );
        assert_eq!(state(&form), json!([option("a"), option("b"), option("c")]));
        assert!(form.fields[&key()].error.is_some());
        assert_eq!(queued.action()["selected_options"], json!([option("a")]));
        assert_eq!(
            form.multi_static_input(&key(), vec![]).unwrap().is_some(),
            optional
        );
        assert_eq!(state(&form), json!([]));
        assert_eq!(form.fields[&key()].error.is_some(), !optional);
        let correction = form
            .multi_static_input(&key(), selected(&["c", "a"]))
            .unwrap()
            .unwrap();
        assert_eq!(
            correction.action()["selected_options"],
            json!([option("c"), option("a")])
        );
        assert!(form.fields[&key()].error.is_none());
    }
}

#[test]
fn conservative_native_admission_rejects_duplicates_foreign_and_tampered_options() {
    let mut form = HomeForm::parse(&raw(true)).unwrap();
    for values in [&["a", "a"][..], &["foreign"], &["a", "foreign"]] {
        let before = form.clone();
        assert!(form.multi_static_input(&key(), selected(values)).is_err());
        assert_eq!(form, before);
    }
    form.fields.get_mut(&key()).unwrap().raw["options"][0]["opaque"] = json!("forged");
    assert!(form.multi_static_input(&key(), selected(&["a"])).is_err());
    let mut form = HomeForm::parse(&raw(true)).unwrap();
    let InputKind::StaticSelect { options, .. } = &mut form.fields.get_mut(&key()).unwrap().kind
    else {
        panic!()
    };
    options[0].raw["opaque"] = json!("forged");
    assert!(form.multi_static_input(&key(), selected(&["a"])).is_err());
}

#[test]
fn quiet_groups_update_state_without_developer_events() {
    let mut view = raw(true);
    view["blocks"][0]["dispatch_action"] = json!(false);
    let element = view["blocks"][0]["element"].as_object_mut().unwrap();
    let options = element.remove("options").unwrap();
    element.insert(
        "option_groups".into(),
        json!([{"label":{"type":"plain_text","text":"Group"},"options":options}]),
    );
    let mut form = HomeForm::parse(&view).unwrap();
    assert!(
        form.multi_static_input(&key(), selected(&["b", "a"]))
            .unwrap()
            .is_none()
    );
    assert_eq!(state(&form), json!([option("b"), option("a")]));
    assert!(
        form.multi_static_input(&key(), selected(&["a", "b", "c"]))
            .unwrap()
            .is_none()
    );
    assert_eq!(state(&form), json!([option("a"), option("b"), option("c")]));
}

#[test]
fn noninput_multi_confirmation_and_visibility_remain_unsupported() {
    for variant in 0..4 {
        let mut view = raw(true);
        let element = view["blocks"][0]["element"].clone();
        match variant {
            0 => {
                view["blocks"][0] =
                    json!({"type":"actions","block_id":"choice","elements":[element]})
            }
            1 => {
                view["blocks"][0] =
                    json!({"type":"section","block_id":"choice","accessory":element})
            }
            2 => view["blocks"][0]["element"]["confirm"] = json!({}),
            _ => view["blocks"][0]["element"]["visible_to_user_ids"] = json!(["U1"]),
        }
        let mut form = HomeForm::parse(&view).unwrap();
        assert!(!form.unsupported.is_empty());
        assert!(form.multi_static_input(&key(), selected(&["a"])).is_err());
        assert!(form.state().is_err());
    }
}

#[test]
fn saved_and_reconciled_arrays_keep_order_and_next_callback_uses_current_metadata() {
    let mut view = raw(true);
    view["state"] = json!({"values":{"choice":{"pick":{"type":"multi_static_select",
        "selected_options":[option("b"),option("a")]}}}});
    let mut form = HomeForm::parse(&view).unwrap();
    assert_eq!(state(&form), json!([option("b"), option("a")]));
    form.multi_static_input(&key(), selected(&["a", "c"]))
        .unwrap();
    let old_wire = state(&form);
    let mut changed = view.clone();
    changed["blocks"][0]["element"]["max_selected_items"] = json!(1);
    changed["blocks"][0]["element"]["options"][2]["opaque"]["version"] = json!(2);
    form.reconcile(&changed).unwrap();
    assert_eq!(
        state(&form),
        old_wire,
        "A source refresh does not emit a callback"
    );
    assert!(form.fields[&key()].error.is_some());
    let event = form
        .multi_static_input(&key(), selected(&["c"]))
        .unwrap()
        .unwrap();
    assert_eq!(
        event.action()["selected_options"][0],
        changed["blocks"][0]["element"]["options"][2]
    );
    assert_eq!(state(&form), event.action()["selected_options"]);
}

#[test]
fn home_maximum_exception_does_not_relax_identity_or_shared_modal_validation() {
    let mut form = HomeForm::parse(&raw(true)).unwrap();
    form.set(&key(), InputValue::Selected(selected(&["a", "b", "c"])))
        .unwrap();
    assert!(
        state(&form)
            .as_array()
            .is_some_and(|options| options.len() == 3)
    );
    assert!(
        values::error(&form.fields[&key()]).is_some(),
        "Shared validation still rejects overmax"
    );
    for invalid_values in [&["a", "a"][..], &["unknown"]] {
        form.set(&key(), InputValue::Selected(selected(invalid_values)))
            .unwrap();
        assert!(form.state().is_err());
    }
}
