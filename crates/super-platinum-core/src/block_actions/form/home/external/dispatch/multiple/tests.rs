use super::*;
fn key() -> InputKey {
    InputKey {
        block_id: "pick".into(),
        action_id: "a".into(),
    }
}
fn option(value: &str, label: &str) -> ExternalOption {
    ExternalOption::parse(&json!({"text":{"type":"plain_text","text":label,"emoji":false},
        "value":value,"description":{"type":"plain_text","text":"Details"},"opaque":{"keep":[1,false,null]}})).unwrap()
}
fn raw(optional: bool, dispatch: bool) -> Value {
    json!({"id":"V1","type":"home","app_id":"A1","app_installed_team_id":"T2","bot_id":"B1","blocks":[
        {"type":"input","block_id":"pick","optional":optional,"dispatch_action":dispatch,
         "element":{"type":"multi_external_select","action_id":"a","max_selected_items":2,
          "initial_options":[option("a","Initial").raw()],"placeholder":{"type":"plain_text","text":"Find","emoji":false}}}]})
}
fn wire(f: &HomeForm) -> Value {
    f.state().unwrap()["values"]["pick"]["a"].clone()
}
fn reconcile(f: &mut HomeForm, raw: &Value, parents: &[ExternalParent]) -> Result<(), FormError> {
    f.reconcile_with_external_parents(raw, &[], &[], &[], &[], &[], &[], parents)
}

#[test]
fn ordered_full_descriptors_empty_value_and_same_value_metadata_are_captured() {
    let mut f = HomeForm::parse(&raw(false, true)).unwrap();
    let chosen = vec![option("b", "Beta"), option("", "Empty")];
    let event = f
        .multi_external_input(&key(), chosen.clone())
        .unwrap()
        .unwrap();
    assert_eq!(
        event.action(),
        &json!({"type":"multi_external_select","block_id":"pick","action_id":"a",
        "selected_options":[chosen[0].raw(),chosen[1].raw()],"initial_options":[option("a","Initial").raw()],
        "placeholder":{"type":"plain_text","text":"Find","emoji":false}})
    );
    let changed = option("b", "Changed");
    let next = f
        .multi_external_input(&key(), vec![changed.clone()])
        .unwrap()
        .unwrap();
    assert_eq!(next.action()["selected_options"], json!([changed.raw()]));
    assert_eq!(
        event.action()["selected_options"],
        json!([chosen[0].raw(), chosen[1].raw()])
    );
}

#[test]
fn local_overmax_preserves_entire_parent_and_required_empty_commits() {
    let mut f = HomeForm::parse(&raw(false, true)).unwrap();
    let event = f
        .multi_external_input(&key(), vec![option("b", "Queued")])
        .unwrap()
        .unwrap();
    let before = f.clone();
    assert!(
        f.multi_external_input(
            &key(),
            vec![option("a", "A"), option("b", "B"), option("c", "C")]
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(f, before);
    assert!(f.multi_external_input(&key(), vec![]).unwrap().is_none());
    assert_eq!(
        wire(&f),
        json!({"type":"multi_external_select","selected_options":[]})
    );
    assert!(f.fields[&key()].error.is_some());
    assert_eq!(
        event.action()["selected_options"],
        json!([option("b", "Queued").raw()])
    );
}

#[test]
fn structural_duplicates_and_size_refuse_atomically_before_local_maximum() {
    let mut f = HomeForm::parse(&raw(true, true)).unwrap();
    let before = f.clone();
    for values in [
        vec![option("a", "A"), option("a", "Different")],
        (0..1001)
            .map(|i| option(&i.to_string(), "Option"))
            .collect(),
    ] {
        assert!(f.multi_external_input(&key(), values).is_err());
        assert_eq!(f, before);
    }
}

#[test]
fn pristine_omission_differs_from_initial_and_callback_empty_arrays() {
    for initial in [None, Some(Value::Null), Some(json!([]))] {
        let mut v = raw(true, true);
        v["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_options");
        if let Some(value) = &initial {
            v["blocks"][0]["element"]["initial_options"] = value.clone();
        }
        let mut f = HomeForm::parse(&v).unwrap();
        assert_eq!(
            wire(&f).get("selected_options").cloned(),
            initial.as_ref().filter(|v| v.is_array()).cloned()
        );
        let event = f.multi_external_input(&key(), vec![]).unwrap().unwrap();
        assert_eq!(wire(&f)["selected_options"], json!([]));
        assert_eq!(
            event.action().get("initial_options"),
            initial.as_ref().filter(|v| v.is_array())
        );
    }
}

#[test]
fn first_full_initial_and_retained_projection_do_not_rewrite_parent() {
    let mut v = raw(true, true);
    v["blocks"][0]["element"]["initial_options"] = json!([option("", "Empty").raw()]);
    let f = HomeForm::parse(&v).unwrap();
    let before = f.clone();
    assert_eq!(
        f.multi_external_initial_seed(&key()).unwrap(),
        vec![option("", "Empty")]
    );
    assert_eq!(
        f.multi_external_seed(&key()).unwrap()[0].raw(),
        &json!({"text":option("","Empty").raw()["text"],"value":""})
    );
    assert_eq!(f, before);
}

#[test]
fn raw_saved_compatibility_empty_arrays_win_and_descriptors_project() {
    for saved in [
        None,
        Some(Value::Null),
        Some(json!([])),
        Some(json!([option("b", "Saved").raw()])),
    ] {
        let mut v = raw(true, true);
        let mut row = json!({"type":"multi_external_select"});
        if let Some(value) = &saved {
            row["selected_options"] = value.clone();
        }
        v["state"] = json!({"values":{"pick":{"a":row}}});
        let f = HomeForm::parse(&v).unwrap();
        let before = f.clone();
        let seed = f.multi_external_initial_seed(&key()).unwrap();
        match saved {
            Some(Value::Array(values)) => {
                assert_eq!(seed.len(), values.len());
                assert_eq!(wire(&f)["selected_options"], json!(values));
                for item in seed {
                    assert!(item.raw().get("opaque").is_none());
                }
            }
            _ => assert_eq!(seed, vec![option("a", "Initial")]),
        }
        assert_eq!(f, before);
    }
}

#[test]
fn initialized_parent_retains_pristine_then_actual_props_callback_uses_current_metadata() {
    for edited in [false, true] {
        let mut v = raw(true, true);
        v["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_options");
        let mut f = HomeForm::parse(&v).unwrap();
        if edited {
            f.multi_external_input(&key(), vec![option("b", "Chosen")])
                .unwrap();
        }
        let parent = f.multi_external_parent(&key()).unwrap();
        let old = wire(&f);
        let mut next = v.clone();
        next["blocks"][0]["element"]["initial_options"] = json!([]);
        next["blocks"][0]["element"]["placeholder"]["text"] = json!("Current");
        reconcile(&mut f, &next, &[parent]).unwrap();
        assert_eq!(wire(&f), old);
        assert_eq!(f.dirty(), edited);
        let event = f.multi_external_input(&key(), vec![]).unwrap().unwrap();
        assert_eq!(event.action()["initial_options"], json!([]));
        assert_eq!(event.action()["placeholder"]["text"], "Current");
        assert_eq!(wire(&f)["selected_options"], json!([]));
    }
}

#[test]
fn initialized_omission_falls_back_to_current_initial_without_parent_commit() {
    let mut v = raw(true, true);
    v["blocks"][0]["element"]
        .as_object_mut()
        .unwrap()
        .remove("initial_options");
    let mut f = HomeForm::parse(&v).unwrap();
    let parent = f.multi_external_parent(&key()).unwrap();
    v["blocks"][0]["element"]["initial_options"] = json!([option("b", "New").raw()]);
    reconcile(&mut f, &v, &[parent]).unwrap();
    assert_eq!(
        f.multi_external_seed(&key()).unwrap(),
        vec![option("b", "New")]
    );
    assert_eq!(wire(&f), json!({"type":"multi_external_select"}));
    f.multi_external_input(&key(), vec![]).unwrap();
    assert!(f.multi_external_seed(&key()).unwrap().is_empty());
}

#[test]
fn parent_tokens_are_kind_owner_key_and_duplicate_bound() {
    let v = raw(true, true);
    let f = HomeForm::parse(&v).unwrap();
    let parent = f.multi_external_parent(&key()).unwrap();
    assert!(f.external_parent(&key()).is_err());
    assert!(f.external_seed(&key()).is_err());
    for mode in 0..5 {
        let mut changed = v.clone();
        let mut tokens = vec![parent.clone()];
        match mode {
            0 => changed["id"] = json!("V2"),
            1 => changed["app_id"] = json!("A2"),
            2 => {
                changed["blocks"][0]["element"] = json!({"type":"external_select","action_id":"a"})
            }
            3 => changed["blocks"][0]["element"]["action_id"] = json!("other"),
            _ => tokens.push(parent.clone()),
        }
        let mut copy = f.clone();
        assert!(reconcile(&mut copy, &changed, &tokens).is_err());
        assert_eq!(copy, f);
    }
}

#[test]
fn corruption_native_error_and_wrong_kind_do_not_grant_callback_authority() {
    let f = HomeForm::parse(&raw(true, true)).unwrap();
    for mode in 0..4 {
        let mut bad = f.clone();
        match mode {
            0 => bad.fields.get_mut(&key()).unwrap().value = InputValue::ExternalOptions(vec![]),
            1 => bad.fields.get_mut(&key()).unwrap().raw["placeholder"]["text"] = json!("Forged"),
            2 => bad.fields.get_mut(&key()).unwrap().optional = false,
            _ => bad.wire.get_mut(&key()).unwrap()["selected_options"] = json!([]),
        }
        let before = bad.clone();
        assert!(bad.state().is_err());
        assert!(bad.multi_external_input(&key(), vec![]).is_err());
        assert_eq!(bad, before);
    }
    let mut native = f.clone();
    native
        .set_native_error(&key(), "unseen".into(), "Safety".into())
        .unwrap();
    assert!(native.multi_external_parent(&key()).is_err());
    assert!(native.state().is_err());
    let mut single = raw(true, true);
    single["blocks"][0]["element"] = json!({"type":"external_select","action_id":"a"});
    let mut single = HomeForm::parse(&single).unwrap();
    assert!(single.multi_external_input(&key(), vec![]).is_err());
    assert!(single.multi_external_parent(&key()).is_err());
}

#[test]
fn quiet_commits_valid_arrays_but_keeps_overmax_local_and_initial_overmax_serializable() {
    let mut v = raw(false, false);
    v["blocks"][0]["element"]["max_selected_items"] = json!(1);
    let mut f = HomeForm::parse(&v).unwrap();
    assert!(
        f.multi_external_input(&key(), vec![option("b", "B")])
            .unwrap()
            .is_none()
    );
    let before = f.clone();
    assert!(
        f.multi_external_input(&key(), vec![option("a", "A"), option("b", "B")])
            .unwrap()
            .is_none()
    );
    assert_eq!(f, before);
    v["blocks"][0]["element"]["initial_options"] =
        json!([option("a", "A").raw(), option("b", "B").raw()]);
    let initial = HomeForm::parse(&v).unwrap();
    assert!(initial.fields[&key()].error.is_some());
    assert!(initial.state().is_ok());
    assert_eq!(
        initial.multi_external_initial_seed(&key()).unwrap().len(),
        2
    );
}
