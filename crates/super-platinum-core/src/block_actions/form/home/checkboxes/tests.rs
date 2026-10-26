use super::*;

fn key() -> InputKey {
    InputKey {
        block_id: "choice".into(),
        action_id: "a".into(),
    }
}
fn option(value: &str, label: &str) -> Value {
    json!({"text":{"type":"plain_text","text":label},"value":value,
        "description":{"type":"plain_text","text":"Description"},"opaque":{"kept":label}})
}
fn raw(optional: bool, quiet: bool) -> Value {
    json!({"id":"V1","type":"home","app_id":"A1","app_installed_team_id":"T1","bot_id":"B1","blocks":[
        {"type":"input","block_id":"choice","optional":optional,"dispatch_action":!quiet,
        "element":{"type":"checkboxes","action_id":"a","options":[option("a","Alpha"),option("b","Beta"),option("","Empty")],
        "initial_options":[option("b","Beta")]}}]})
}
fn selected(form: &HomeForm) -> Value {
    form.state().unwrap()["values"]["choice"]["a"]["selected_options"].clone()
}

#[test]
fn survivor_metadata_is_owned_old_raw_until_removed_and_readded() {
    let mut view = raw(true, false);
    let mut form = HomeForm::parse(&view).unwrap();
    let mut local = form.checkbox_selection(&key()).unwrap();
    view["blocks"][0]["element"]["options"][1] = option("b", "Beta current");
    view["blocks"][0]["element"]["initial_options"] = json!([]);
    form.reconcile(&view).unwrap();
    assert_eq!(
        selected(&form),
        json!([]),
        "Pristine parent can change independently"
    );
    assert_eq!(local.selected(), &[option("b", "Beta")]);
    let event = form
        .checkbox_input(&key(), &mut local, 0, true)
        .unwrap()
        .unwrap();
    assert_eq!(
        selected(&form),
        json!([option("b", "Beta"), option("a", "Alpha")])
    );
    assert_eq!(
        event.action(),
        &json!({"type":"checkboxes","block_id":"choice","action_id":"a",
        "selected_options":[option("b","Beta"),option("a","Alpha")]})
    );
    form.checkbox_input(&key(), &mut local, 1, false).unwrap();
    form.checkbox_input(&key(), &mut local, 1, true).unwrap();
    assert_eq!(
        selected(&form),
        json!([option("a", "Alpha"), option("b", "Beta current")])
    );
    assert!(event.same_view(&view));
    assert!(event.action().get("initial_options").is_none());
    assert!(event.action().get("placeholder").is_none());
}

#[test]
fn dirty_hidden_survivor_remains_in_state_and_new_callback() {
    let mut view = raw(false, false);
    let mut form = HomeForm::parse(&view).unwrap();
    let mut local = form.checkbox_selection(&key()).unwrap();
    form.checkbox_input(&key(), &mut local, 0, true).unwrap();
    view["blocks"][0]["element"]["options"] = json!([option("a", "Alpha"), option("", "Empty")]);
    view["blocks"][0]["element"]
        .as_object_mut()
        .unwrap()
        .remove("initial_options");
    form.reconcile(&view).unwrap();
    assert_eq!(
        selected(&form),
        json!([option("b", "Beta"), option("a", "Alpha")])
    );
    assert!(form.fields[&key()].error.is_none());
    assert!(
        values::error(&form.fields[&key()]).is_some(),
        "Shared Modal/Message identity validation unchanged"
    );
    let event = form
        .checkbox_input(&key(), &mut local, 0, false)
        .unwrap()
        .unwrap();
    assert_eq!(
        event.action()["selected_options"],
        json!([option("b", "Beta")])
    );
    let seeded = form.checkbox_selection(&key()).unwrap();
    assert_eq!(
        seeded, local,
        "New seed uses private committed wire including hidden survivor"
    );
}

#[test]
fn empty_string_is_real_and_required_empty_commits_before_suppression() {
    for optional in [true, false] {
        let mut form = HomeForm::parse(&raw(optional, false)).unwrap();
        let mut local = form.checkbox_selection(&key()).unwrap();
        let earlier = form
            .checkbox_input(&key(), &mut local, 2, true)
            .unwrap()
            .unwrap();
        form.checkbox_input(&key(), &mut local, 1, false).unwrap();
        assert_eq!(selected(&form), json!([option("", "Empty")]));
        assert!(form.fields[&key()].error.is_none());
        assert_eq!(
            form.checkbox_input(&key(), &mut local, 2, false)
                .unwrap()
                .is_some(),
            optional
        );
        assert_eq!(selected(&form), json!([]));
        assert_eq!(form.fields[&key()].error.is_some(), !optional);
        assert_eq!(
            earlier.action()["selected_options"],
            json!([option("b", "Beta"), option("", "Empty")])
        );
    }
}

#[test]
fn quiet_callbacks_keep_old_survivors_but_never_mint_events() {
    let mut view = raw(true, true);
    let mut form = HomeForm::parse(&view).unwrap();
    let mut local = form.checkbox_selection(&key()).unwrap();
    view["blocks"][0]["element"]["options"][1] = option("b", "New");
    view["blocks"][0]["element"]["initial_options"] = json!([option("b", "New")]);
    form.reconcile(&view).unwrap();
    assert!(
        form.checkbox_input(&key(), &mut local, 0, true)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        selected(&form),
        json!([option("b", "Beta"), option("a", "Alpha")])
    );
    assert!(
        form.checkbox_input(&key(), &mut local, 1, false)
            .unwrap()
            .is_none()
    );
    assert!(
        form.checkbox_input(&key(), &mut local, 0, false)
            .unwrap()
            .is_none()
    );
    assert_eq!(selected(&form), json!([]));
}

#[test]
fn no_checkbox_maximum_or_mounted_prop_synchronization_is_introduced() {
    let mut view = raw(true, false);
    let mut form = HomeForm::parse(&view).unwrap();
    let mut local = form.checkbox_selection(&key()).unwrap();
    view["blocks"][0]["element"]["max_selected_items"] = json!(1);
    view["blocks"][0]["element"]["initial_options"] = json!([]);
    view["state"] = json!({"values":{"choice":{"a":{"type":"checkboxes","selected_options":[]}}}});
    form.reconcile(&view).unwrap();
    assert_eq!(local.selected(), &[option("b", "Beta")]);
    assert!(
        form.checkbox_input(&key(), &mut local, 0, true)
            .unwrap()
            .is_some()
    );
    assert_eq!(selected(&form).as_array().unwrap().len(), 2);
    assert!(form.fields[&key()].error.is_none());
}

#[test]
fn duplicate_foreign_and_nontransitions_are_atomic_refusals() {
    let mut form = HomeForm::parse(&raw(true, false)).unwrap();
    let mut local = form.checkbox_selection(&key()).unwrap();
    for (index, checked) in [(1, true), (0, false), (99, true)] {
        let before = form.clone();
        let old = local.clone();
        assert!(
            form.checkbox_input(&key(), &mut local, index, checked)
                .is_err()
        );
        assert_eq!(form, before);
        assert_eq!(local, old);
    }
    for values in [vec!["b".into(), "b".into()], vec!["unknown".into()]] {
        let mut forged = form.clone();
        forged.set(&key(), InputValue::Selected(values)).unwrap();
        assert!(forged.state().is_err());
        assert!(forged.checkbox_selection(&key()).is_err());
    }
}

#[test]
fn public_value_kind_raw_optional_and_error_cannot_forge_private_wire() {
    for variant in 0..8 {
        let mut form = HomeForm::parse(&raw(false, false)).unwrap();
        match variant {
            0 => {
                form.fields.get_mut(&key()).unwrap().value = InputValue::Selected(vec!["a".into()])
            }
            1 => form.fields.get_mut(&key()).unwrap().optional = true,
            2 => form.fields.get_mut(&key()).unwrap().raw["options"][1]["opaque"] = json!("forged"),
            3 => {
                let InputKind::Checkboxes { options } =
                    &mut form.fields.get_mut(&key()).unwrap().kind
                else {
                    panic!()
                };
                options[1].raw["opaque"] = json!("forged");
            }
            4 => form.fields.get_mut(&key()).unwrap().error = Some("forged".into()),
            5 => form.fields.get_mut(&key()).unwrap().kind = InputKind::Radio { options: vec![] },
            6 => {
                let InputKind::Checkboxes { options } = form.fields[&key()].kind.clone() else {
                    panic!()
                };
                form.fields.get_mut(&key()).unwrap().kind = InputKind::StaticSelect {
                    options,
                    multiple: true,
                    max_selected_items: None,
                };
            }
            _ => {
                form.fields.remove(&key());
            }
        }
        assert!(form.state().is_err(), "variant {variant}");
    }
}

#[test]
fn selection_and_dirty_wire_cannot_cross_view_app_or_field_identity() {
    let view = raw(true, false);
    let mut form = HomeForm::parse(&view).unwrap();
    let mut local = form.checkbox_selection(&key()).unwrap();
    form.checkbox_input(&key(), &mut local, 0, true).unwrap();
    for name in ["id", "app_id", "app_installed_team_id", "bot_id"] {
        let mut changed = view.clone();
        changed[name] = json!("other");
        let mut other = HomeForm::parse(&changed).unwrap();
        let before = other.clone();
        let mut candidate = local.clone();
        assert!(
            other
                .checkbox_input(&key(), &mut candidate, 1, false)
                .is_err()
        );
        assert_eq!(other, before);
        assert_eq!(candidate, local);
        assert!(form.clone().reconcile(&changed).is_err());
    }
    let mut changed = view.clone();
    changed["blocks"][0]["block_id"] = json!("other");
    let mut other = HomeForm::parse(&changed).unwrap();
    let foreign = InputKey {
        block_id: "other".into(),
        action_id: "a".into(),
    };
    assert!(
        other
            .checkbox_input(&foreign, &mut local, 1, false)
            .is_err()
    );
}

#[test]
fn source_saved_state_requires_exact_current_raw_and_new_ingress_is_not_inferred() {
    let mut view = raw(true, false);
    let mut raw_saved = option("a", "Forged saved label");
    raw_saved["opaque"] = json!({"forged":true});
    view["state"] = json!({"values":{"choice":{"a":{"type":"checkboxes","selected_options":[option("a","Alpha")]}}}});
    let form = HomeForm::parse(&view).unwrap();
    assert_eq!(
        form.checkbox_selection(&key()).unwrap().selected(),
        &[option("a", "Alpha")]
    );
    view["state"]["values"]["choice"]["a"]["selected_options"] = json!([raw_saved]);
    assert!(HomeForm::parse(&view).is_err());
    view["state"]["values"]["choice"]["a"]["selected_options"] =
        json!([option("foreign", "Foreign")]);
    assert!(HomeForm::parse(&view).is_err());
}

#[test]
fn wrappers_confirmation_and_wrong_static_apis_remain_refused() {
    for variant in 0..4 {
        let mut view = raw(true, false);
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
        let form = HomeForm::parse(&view).unwrap();
        assert!(form.checkbox_selection(&key()).is_err());
        assert!(form.state().is_err());
    }
    let mut form = HomeForm::parse(&raw(true, false)).unwrap();
    assert!(form.static_input(&key(), Some("a".into())).is_err());
    assert!(form.multi_static_input(&key(), vec!["a".into()]).is_err());
}

#[test]
fn checkbox_option_shape_reuses_composition_grammar_without_message_policies() {
    for quiet in [false, true] {
        for variant in 0..3 {
            let mut view = raw(true, quiet);
            match variant {
                0 => {
                    let e = view["blocks"][0]["element"].as_object_mut().unwrap();
                    let options = e.remove("options").unwrap();
                    e.insert(
                        "option_groups".into(),
                        json!([{"label":{"type":"plain_text","text":"Group"},"options":options}]),
                    );
                }
                1 => {
                    view["blocks"][0]["element"]["options"][0]["description"]["type"] =
                        json!("image")
                }
                _ => view["blocks"][0]["element"]["options"][0]["text"]["emoji"] = json!("yes"),
            }
            let form = HomeForm::parse(&view).unwrap();
            assert!(!form.unsupported.is_empty());
            assert!(form.state().is_err());
            assert!(form.checkbox_selection(&key()).is_err());
        }
        let mut view = raw(true, quiet);
        view["blocks"][0]["element"]["options"][0]["text"]["type"] = json!("mrkdwn");
        view["blocks"][0]["element"]["options"][0]["description"]["type"] = json!("mrkdwn");
        view["blocks"][0]["element"]["max_selected_items"] = json!(1);
        let mut form = HomeForm::parse(&view).unwrap();
        let mut local = form.checkbox_selection(&key()).unwrap();
        assert_eq!(
            form.checkbox_input(&key(), &mut local, 0, true)
                .unwrap()
                .is_some(),
            !quiet
        );
        assert_eq!(selected(&form).as_array().unwrap().len(), 2);
    }
}

#[test]
fn candidate_form_and_selection_clones_commit_only_when_admitted() {
    let form = HomeForm::parse(&raw(true, false)).unwrap();
    let local = form.checkbox_selection(&key()).unwrap();
    let mut candidate = form.clone();
    let mut selected = local.clone();
    candidate
        .checkbox_input(&key(), &mut selected, 0, true)
        .unwrap();
    assert_eq!(local.selected(), &[option("b", "Beta")]);
    assert_eq!(
        form.fields[&key()].value,
        InputValue::Selected(vec!["b".into()])
    );
    assert_eq!(
        selected.selected(),
        &[option("b", "Beta"), option("a", "Alpha")]
    );
}
