use super::*;

fn key() -> InputKey {
    InputKey {
        block_id: "choice".into(),
        action_id: "a".into(),
    }
}
fn option(value: &str, label: &str) -> Value {
    json!({"text":{"type":"plain_text","text":label},"value":value,
        "description":{"type":"mrkdwn","text":"*Description*"},"opaque":{"keep":label}})
}
fn raw(optional: bool, quiet: bool) -> Value {
    json!({"id":"V1","type":"home","app_id":"A1","app_installed_team_id":"T1","bot_id":"B1","blocks":[
        {"type":"input","block_id":"choice","optional":optional,"dispatch_action":!quiet,
        "element":{"type":"radio_buttons","action_id":"a","options":[option("a","Alpha"),option("b","Beta"),option("","Empty")],
        "initial_option":option("a","Alpha"),"placeholder":{"type":"plain_text","text":"Unused"}}}]})
}
fn selected(form: &HomeForm) -> Value {
    form.state().unwrap()["values"]["choice"]["a"]["selected_option"].clone()
}

#[test]
fn current_row_callback_keeps_raw_and_initial_but_no_placeholder() {
    let view = raw(true, false);
    let mut form = HomeForm::parse(&view).unwrap();
    let event = form.radio_input(&key(), Some(1)).unwrap().unwrap();
    assert_eq!(
        event.action(),
        &json!({"type":"radio_buttons","block_id":"choice","action_id":"a",
        "selected_option":option("b","Beta"),"initial_option":option("a","Alpha")})
    );
    assert!(event.same_view(&view));
    assert_eq!(selected(&form), option("b", "Beta"));
    assert!(event.action().get("placeholder").is_none());
    assert_eq!(
        form.radio_input(&key(), Some(1)).unwrap().unwrap(),
        event,
        "Direct repeated callbacks emit; native checked-radio reachability is separate"
    );
}

#[test]
fn empty_string_is_real_required_null_commits_before_suppression() {
    for optional in [false, true] {
        let mut form = HomeForm::parse(&raw(optional, false)).unwrap();
        let event = form.radio_input(&key(), Some(2)).unwrap().unwrap();
        assert_eq!(event.action()["selected_option"], option("", "Empty"));
        assert!(form.fields[&key()].error.is_none());
        let cleared = form.radio_input(&key(), None).unwrap();
        assert_eq!(selected(&form), Value::Null);
        assert_eq!(form.fields[&key()].error.is_some(), !optional);
        assert_eq!(cleared.is_some(), optional);
        if let Some(event) = cleared {
            assert_eq!(event.action()["selected_option"], Value::Null);
        }
    }
}

#[test]
fn quiet_stores_identically_without_dispatch() {
    let mut form = HomeForm::parse(&raw(true, true)).unwrap();
    assert!(form.radio_input(&key(), Some(1)).unwrap().is_none());
    assert_eq!(selected(&form), option("b", "Beta"));
    assert!(form.fields[&key()].dirty);
    assert!(form.radio_input(&key(), None).unwrap().is_none());
    assert_eq!(selected(&form), Value::Null);
}

#[test]
fn dirty_hidden_raw_survives_source_changes_and_current_callback_replaces_it() {
    let mut view = raw(false, false);
    let mut form = HomeForm::parse(&view).unwrap();
    let old = form.radio_input(&key(), Some(1)).unwrap().unwrap();
    view["hash"] = json!("updated");
    view["blocks"][0]["element"]["options"] =
        json!([option("a", "Alpha current"), option("", "Empty")]);
    view["blocks"][0]["element"]
        .as_object_mut()
        .unwrap()
        .remove("initial_option");
    form.reconcile(&view).unwrap();
    assert_eq!(selected(&form), option("b", "Beta"));
    assert_eq!(form.radio_seed(&key()).unwrap(), Some("b".into()));
    assert!(form.fields[&key()].error.is_none());
    assert!(
        values::error(&form.fields[&key()]).is_some(),
        "Shared identity validation unchanged"
    );
    let next = form.radio_input(&key(), Some(0)).unwrap().unwrap();
    assert_eq!(
        next.action()["selected_option"],
        option("a", "Alpha current")
    );
    assert!(next.action().get("initial_option").is_none());
    assert_eq!(old.action()["selected_option"], option("b", "Beta"));
    assert!(!old.same_view(&view));
}

#[test]
fn metadata_only_and_initial_prop_changes_preserve_dirty_parent_until_callback() {
    let mut view = raw(true, false);
    let mut form = HomeForm::parse(&view).unwrap();
    form.radio_input(&key(), Some(1)).unwrap();
    view["blocks"][0]["element"]["options"][1] = option("b", "Beta current");
    view["blocks"][0]["element"]["initial_option"] = option("", "Empty");
    view["state"] = json!({"values":{"choice":{"a":{"type":"radio_buttons","selected_option":option("a","Alpha")}}}});
    form.reconcile(&view).unwrap();
    assert_eq!(selected(&form), option("b", "Beta"));
    let event = form.radio_input(&key(), Some(1)).unwrap().unwrap();
    assert_eq!(
        event.action()["selected_option"],
        option("b", "Beta current")
    );
    assert_eq!(event.action()["initial_option"], option("", "Empty"));
}

#[test]
fn fresh_seed_uses_trusted_saved_nonempty_else_source_initial() {
    for (saved, initial, expected) in [
        (Value::Null, Some(option("a", "Alpha")), Some("a")),
        (option("", "Empty"), Some(option("a", "Alpha")), Some("a")),
        (option("b", "Beta"), Some(option("a", "Alpha")), Some("b")),
        (Value::Null, Some(option("", "Empty")), Some("")),
        (option("", "Empty"), None, None),
        (Value::Null, None, None),
    ] {
        let mut view = raw(true, false);
        if let Some(initial) = initial {
            view["blocks"][0]["element"]["initial_option"] = initial;
        } else {
            view["blocks"][0]["element"]
                .as_object_mut()
                .unwrap()
                .remove("initial_option");
        }
        view["state"] =
            json!({"values":{"choice":{"a":{"type":"radio_buttons","selected_option":saved}}}});
        let form = HomeForm::parse(&view).unwrap();
        assert_eq!(form.radio_seed(&key()).unwrap().as_deref(), expected);
        assert_eq!(
            selected(&form),
            saved,
            "Display fallback must not restore parent state"
        );
    }
}

#[test]
fn foreign_indices_keys_and_public_tampering_fail_atomically() {
    let original = HomeForm::parse(&raw(true, false)).unwrap();
    for index in [3, usize::MAX] {
        let mut form = original.clone();
        assert!(form.radio_input(&key(), Some(index)).is_err());
        assert_eq!(form, original);
    }
    let mut form = original.clone();
    assert!(
        form.radio_input(
            &InputKey {
                block_id: "foreign".into(),
                action_id: "a".into()
            },
            Some(0)
        )
        .is_err()
    );
    assert_eq!(form, original);
    for mode in 0..7 {
        let mut form = original.clone();
        match mode {
            0 => {
                form.fields.remove(&key());
            }
            1 => {
                form.fields.get_mut(&key()).unwrap().kind = InputKind::Text {
                    multiline: false,
                    min_length: None,
                    max_length: None,
                }
            }
            2 => form.fields.get_mut(&key()).unwrap().optional = false,
            3 => form.fields.get_mut(&key()).unwrap().raw["options"][0]["value"] = json!("forged"),
            4 => {
                form.fields.get_mut(&key()).unwrap().value = InputValue::Selected(vec!["b".into()])
            }
            5 => {
                form.fields.get_mut(&key()).unwrap().value =
                    InputValue::Selected(vec!["a".into(), "a".into()])
            }
            _ => form.fields.get_mut(&key()).unwrap().error = Some("forged".into()),
        }
        assert!(form.state().is_err(), "tamper {mode}");
        if mode != 6 {
            assert!(form.radio_seed(&key()).is_err());
        }
    }
}

#[test]
fn generic_set_cannot_create_retained_unknown_or_duplicate_authority() {
    for values in [
        vec!["foreign".into()],
        vec!["a".into(), "a".into()],
        vec!["a".into(), "b".into()],
    ] {
        let mut form = HomeForm::parse(&raw(true, false)).unwrap();
        form.set(&key(), InputValue::Selected(values)).unwrap();
        assert!(form.state().is_err());
        assert!(form.radio_seed(&key()).is_err());
        assert!(form.radio_input(&key(), Some(0)).is_err());
    }
}

#[test]
fn owner_changes_and_source_field_removal_refuse_dirty_reconcile() {
    let view = raw(true, false);
    let mut form = HomeForm::parse(&view).unwrap();
    form.radio_input(&key(), Some(1)).unwrap();
    for owner in ["id", "app_id", "app_installed_team_id", "bot_id"] {
        let before = form.clone();
        let mut next = view.clone();
        next[owner] = json!("foreign");
        assert!(form.reconcile(&next).is_err());
        assert_eq!(form, before);
    }
    let mut next = view.clone();
    next["blocks"] = json!([]);
    assert!(form.reconcile(&next).is_err());
}

#[test]
fn unsupported_shapes_fail_whole_form_but_rich_compositions_are_valid() {
    for quiet in [false, true] {
        for mode in 0..5 {
            let mut view = raw(true, quiet);
            let e = &mut view["blocks"][0]["element"];
            match mode {
                0 => {
                    let options = e.as_object_mut().unwrap().remove("options").unwrap();
                    e["option_groups"] =
                        json!([{"label":{"type":"plain_text","text":"Group"},"options":options}]);
                }
                1 => e["options"][1]["text"]["type"] = json!("html"),
                2 => e["options"][1]["description"]["type"] = json!("html"),
                3 => e["confirm"] = json!({}),
                _ => e["visible_to_user_ids"] = json!(["U1"]),
            }
            let form = HomeForm::parse(&view).unwrap();
            assert!(!form.unsupported.is_empty());
            assert!(form.state().is_err());
        }
    }
    let mut view = raw(true, false);
    view["blocks"][0]["element"]["options"][1]["text"] = json!({"type":"mrkdwn","text":"*Beta*"});
    let mut form = HomeForm::parse(&view).unwrap();
    assert_eq!(
        form.radio_input(&key(), Some(1)).unwrap().unwrap().action()["selected_option"]["text"]["type"],
        "mrkdwn"
    );
}

#[test]
fn existing_raw_initial_and_saved_descriptor_admission_is_unchanged() {
    let mut view = raw(true, false);
    view["blocks"][0]["element"]["initial_option"] = Value::Null;
    assert!(HomeForm::parse(&view).is_err());
    let mut view = raw(true, false);
    view["state"] = json!({"values":{"choice":{"a":{"type":"radio_buttons","selected_option":option("b","Forged")}}}});
    assert!(HomeForm::parse(&view).is_err());
    let mut view = raw(true, false);
    view["blocks"][0]["element"]["options"][1] = option("a", "Duplicate");
    assert!(HomeForm::parse(&view).is_err());
}

#[test]
fn private_wire_shape_and_public_identity_must_agree() {
    for mode in 0..5 {
        let mut form = HomeForm::parse(&raw(true, false)).unwrap();
        let wire = form.wire.get_mut(&key()).unwrap();
        match mode {
            0 => wire["type"] = json!("static_select"),
            1 => {
                wire.as_object_mut().unwrap().remove("selected_option");
            }
            2 => wire["extra"] = json!(true),
            3 => wire["selected_option"] = option("b", "Beta"),
            _ => wire["selected_option"]["description"] = json!({"type":"html","text":"bad"}),
        }
        assert!(form.state().is_err());
        assert!(form.radio_seed(&key()).is_err());
        assert!(form.radio_input(&key(), Some(0)).is_err());
    }
}

#[test]
fn radio_wrappers_remain_unsupported_and_requiredness_recomputes_for_retained_state() {
    for kind in ["actions", "section"] {
        let mut view = raw(true, false);
        let element = view["blocks"][0]["element"].clone();
        view["blocks"][0] = if kind == "actions" {
            json!({"type":kind,"block_id":"choice","elements":[element]})
        } else {
            json!({"type":kind,"block_id":"choice","accessory":element})
        };
        let form = HomeForm::parse(&view).unwrap();
        assert!(!form.unsupported.is_empty());
        assert!(form.state().is_err());
    }
    let mut view = raw(true, false);
    let mut form = HomeForm::parse(&view).unwrap();
    form.radio_input(&key(), None).unwrap();
    view["blocks"][0]["optional"] = json!(false);
    form.reconcile(&view).unwrap();
    assert_eq!(selected(&form), Value::Null);
    assert_eq!(
        form.fields[&key()].error.as_deref(),
        Some("Complete this required field.")
    );
}
