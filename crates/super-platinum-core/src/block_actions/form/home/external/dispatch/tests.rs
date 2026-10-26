use super::*;

fn key() -> InputKey {
    InputKey {
        block_id: "choice".into(),
        action_id: "pick".into(),
    }
}
fn option(label: &str) -> ExternalOption {
    ExternalOption::parse(
        &json!({"text":{"type":"plain_text","text":label,"emoji":false},
        "value":"same", "description":{"type":"plain_text","text":"Details"},
        "opaque":{"keep":[1,false,null]}}),
    )
    .unwrap()
}
fn raw(optional: bool, dispatch: bool) -> Value {
    json!({"type":"home","id":"V1","app_id":"A1","app_installed_team_id":"T2","bot_id":"B1","blocks":[
        {"type":"input","block_id":"choice","optional":optional,"dispatch_action":dispatch,
         "element":{"type":"external_select","action_id":"pick","min_query_length":0,
          "initial_option":option("Initial").raw(),"placeholder":{"type":"plain_text","text":"Find exact","emoji":false}}}]})
}
fn wire(f: &HomeForm) -> Value {
    f.state().unwrap()["values"]["choice"]["pick"].clone()
}
fn reconcile(f: &mut HomeForm, next: &Value, tokens: &[ExternalParent]) -> Result<(), FormError> {
    f.reconcile_with_external_parents(next, &[], &[], &[], &[], &[], &[], tokens)
}

#[test]
fn exact_descriptor_and_current_metadata_are_sealed_without_value_deduplication() {
    let mut f = HomeForm::parse(&raw(true, true)).unwrap();
    let a = f
        .external_input(&key(), Some(option("First")))
        .unwrap()
        .unwrap();
    let b = f
        .external_input(&key(), Some(option("Second")))
        .unwrap()
        .unwrap();
    assert_eq!(a.action()["selected_option"], *option("First").raw());
    assert_eq!(b.action()["selected_option"], *option("Second").raw());
    assert_eq!(wire(&f)["selected_option"], *option("Second").raw());
    assert_eq!(a.action()["initial_option"], *option("Initial").raw());
    assert_eq!(
        a.action()["placeholder"],
        raw(true, true)["blocks"][0]["element"]["placeholder"]
    );
    assert_eq!(a.action().as_object().unwrap().len(), 6);
    assert_eq!(
        f.external_input(&key(), Some(option("Second")))
            .unwrap()
            .unwrap()
            .action(),
        b.action()
    );
}

#[test]
fn pristine_omission_and_callback_null_are_distinct() {
    for null in [false, true] {
        let mut v = raw(true, true);
        if null {
            v["blocks"][0]["element"]["initial_option"] = Value::Null;
        } else {
            v["blocks"][0]["element"]
                .as_object_mut()
                .unwrap()
                .remove("initial_option");
        }
        let mut f = HomeForm::parse(&v).unwrap();
        assert_eq!(wire(&f), json!({"type":"external_select"}));
        assert!(f.external_parent(&key()).is_ok());
        assert_eq!(f.external_seed(&key()).unwrap(), None);
        let e = f.external_input(&key(), None).unwrap().unwrap();
        assert_eq!(
            wire(&f),
            json!({"type":"external_select","selected_option":null})
        );
        assert!(e.action().get("initial_option").is_none());
    }
}

#[test]
fn required_and_quiet_callbacks_store_before_suppressing_new_action() {
    let mut f = HomeForm::parse(&raw(false, true)).unwrap();
    let queued = f
        .external_input(&key(), Some(option("Queued")))
        .unwrap()
        .unwrap();
    assert!(f.external_input(&key(), None).unwrap().is_none());
    assert!(f.fields[&key()].error.is_some());
    assert_eq!(wire(&f)["selected_option"], Value::Null);
    assert_eq!(queued.action()["selected_option"], *option("Queued").raw());
    let mut quiet = HomeForm::parse(&raw(true, false)).unwrap();
    assert!(
        quiet
            .external_input(&key(), Some(option("Quiet")))
            .unwrap()
            .is_none()
    );
    assert_eq!(wire(&quiet)["selected_option"], *option("Quiet").raw());
    assert!(quiet.external_input(&key(), None).unwrap().is_none());
    assert_eq!(wire(&quiet)["selected_option"], Value::Null);
}

#[test]
fn empty_value_descriptor_is_required_selection_not_clear() {
    let mut v = option("Empty identifier").raw().clone();
    v["value"] = json!("");
    let empty = ExternalOption::parse(&v).unwrap();
    let mut f = HomeForm::parse(&raw(false, true)).unwrap();
    let event = f
        .external_input(&key(), Some(empty.clone()))
        .unwrap()
        .unwrap();
    assert_eq!(event.action()["selected_option"], v);
    assert_eq!(wire(&f)["selected_option"], v);
    assert!(f.fields[&key()].error.is_none());
    assert_eq!(f.external_seed(&key()).unwrap().unwrap().value(), "");
}

#[test]
fn fresh_seed_projects_parent_but_clear_falls_back_full_initial_without_commit() {
    let mut f = HomeForm::parse(&raw(true, true)).unwrap();
    f.external_input(&key(), Some(option("Chosen"))).unwrap();
    let seed = f.external_seed(&key()).unwrap().unwrap();
    assert_eq!(
        seed.raw(),
        &json!({"text":option("Chosen").raw()["text"],"value":"same"})
    );
    assert_eq!(wire(&f)["selected_option"], *option("Chosen").raw());
    f.external_input(&key(), None).unwrap();
    assert_eq!(f.external_seed(&key()).unwrap().unwrap(), option("Initial"));
    assert_eq!(wire(&f)["selected_option"], Value::Null);
}

#[test]
fn initialized_pristine_and_dirty_parent_retention_are_independent_of_props() {
    for dirty in [false, true] {
        let mut f = HomeForm::parse(&raw(true, true)).unwrap();
        let token = f.external_parent(&key()).unwrap();
        if dirty {
            f.external_input(&key(), Some(option("Latest"))).unwrap();
        }
        let before = wire(&f);
        let mut next = raw(true, true);
        next["hash"] = json!("next");
        next["blocks"][0]["element"]["initial_option"] = option("Changed").raw().clone();
        next["blocks"][0]["element"]["placeholder"]["text"] = json!("New prompt");
        reconcile(&mut f, &next, &[token.clone()]).unwrap();
        assert_eq!(wire(&f), before);
        assert_eq!(f.fields[&key()].dirty, dirty);
        let e = f
            .external_input(&key(), Some(option("After")))
            .unwrap()
            .unwrap();
        assert_eq!(e.action()["initial_option"], *option("Changed").raw());
        assert_eq!(e.action()["placeholder"]["text"], "New prompt");
        next["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_option");
        reconcile(&mut f, &next, &[token]).unwrap();
        assert_eq!(wire(&f)["selected_option"], *option("After").raw());
    }
    let mut unmounted = HomeForm::parse(&raw(true, true)).unwrap();
    let mut next = raw(true, true);
    next["blocks"][0]["element"]["initial_option"] = option("New").raw().clone();
    unmounted.reconcile(&next).unwrap();
    assert_eq!(wire(&unmounted)["selected_option"], *option("New").raw());
}

#[test]
fn raw_saved_ingress_remains_separate_and_unchanged() {
    for selected in [None, Some(Value::Null), Some(option("Saved").raw().clone())] {
        let mut v = raw(true, true);
        let mut row = json!({"type":"external_select"});
        if let Some(selected) = selected {
            row["selected_option"] = selected;
        }
        v["state"] = json!({"values":{"choice":{"pick":row}}});
        let f = HomeForm::parse(&v).unwrap();
        let expected = if row["selected_option"].is_object() {
            option("Saved")
        } else {
            option("Initial")
        };
        assert_eq!(wire(&f)["selected_option"], *expected.raw());
        assert_eq!(
            f.external_seed(&key()).unwrap().unwrap().raw(),
            &json!({"text":expected.raw()["text"],"value":"same"})
        );
    }
    let mut v = raw(true, true);
    v["state"] = json!({"values":{"choice":{"pick":{"type":"external_select","selected_option":{"value":"bad"}}}}});
    assert!(HomeForm::parse(&v).is_err());
}

#[test]
fn initialized_omitted_parent_stays_omitted_while_new_seed_falls_back_to_changed_initial() {
    let mut v = raw(true, true);
    v["blocks"][0]["element"]
        .as_object_mut()
        .unwrap()
        .remove("initial_option");
    let mut f = HomeForm::parse(&v).unwrap();
    let token = f.external_parent(&key()).unwrap();
    let next = raw(true, true);
    reconcile(&mut f, &next, &[token]).unwrap();
    assert_eq!(wire(&f), json!({"type":"external_select"}));
    assert!(!f.fields[&key()].dirty);
    assert_eq!(f.external_seed(&key()).unwrap().unwrap(), option("Initial"));
    assert_eq!(wire(&f), json!({"type":"external_select"}));
}

#[test]
fn parent_tokens_refuse_owner_identity_removed_kind_and_duplicates_atomically() {
    let f = HomeForm::parse(&raw(true, true)).unwrap();
    let token = f.external_parent(&key()).unwrap();
    for mode in 0..7 {
        let mut copy = f.clone();
        let mut next = raw(true, true);
        match mode {
            0 => next["id"] = json!("V2"),
            1 => next["app_id"] = json!("A2"),
            2 => next["bot_id"] = json!("B2"),
            3 => next["app_installed_team_id"] = json!("T3"),
            4 => next["blocks"] = json!([]),
            5 => {
                next["blocks"][0]["element"] = json!({"type":"plain_text_input","action_id":"pick"})
            }
            _ => {}
        }
        let tokens = if mode == 6 {
            vec![token.clone(), token.clone()]
        } else {
            vec![token.clone()]
        };
        assert!(reconcile(&mut copy, &next, &tokens).is_err(), "mode {mode}");
        assert_eq!(copy, f);
    }
}

#[test]
fn public_descriptor_source_wire_inventory_and_error_corruption_refuse() {
    let f = HomeForm::parse(&raw(true, true)).unwrap();
    for mode in 0..8 {
        let mut bad = f.clone();
        match mode {
            0 => {
                bad.fields.get_mut(&key()).unwrap().value =
                    InputValue::External(Some(option("Forged")))
            }
            1 => bad.fields.get_mut(&key()).unwrap().raw["min_query_length"] = json!(99),
            2 => bad.fields.get_mut(&key()).unwrap().error = Some("invented".into()),
            3 => {
                bad.fields.remove(&key());
            }
            4 => bad.wire.get_mut(&key()).unwrap()["opaque"] = json!(1),
            5 => {
                bad.wire.remove(&key());
            }
            6 => {
                bad.fields.get_mut(&key()).unwrap().kind = InputKind::ExternalSelect {
                    min_query_length: 999,
                }
            }
            _ => bad.fields.get_mut(&key()).unwrap().optional = false,
        }
        let before = bad.clone();
        assert!(bad.state().is_err());
        assert!(bad.external_seed(&key()).is_err());
        assert!(bad.external_input(&key(), Some(option("Next"))).is_err());
        assert_eq!(bad, before);
    }
    let mut native = f.clone();
    native
        .set_native_error(&key(), "unseen".into(), "Safety".into())
        .unwrap();
    assert!(native.external_parent(&key()).is_err());
    assert!(native.state().is_err());
}

#[test]
fn single_api_does_not_grant_multi_noninput_or_new_descriptor_grammar() {
    let mut v = raw(true, false);
    v["blocks"][0]["element"] = json!({"type":"multi_external_select","action_id":"pick"});
    let mut f = HomeForm::parse(&v).unwrap();
    assert!(f.external_field(&key()).is_ok());
    assert!(f.external_seed(&key()).is_err());
    assert!(f.external_input(&key(), None).is_err());
    assert!(f.state().is_ok());
    v["blocks"][0]["dispatch_action"] = json!(true);
    let mut multi = HomeForm::parse(&v).unwrap();
    assert!(multi.state().is_ok());
    assert!(multi.external_seed(&key()).is_err());
    assert!(multi.external_input(&key(), None).is_err());
    assert!(
        multi
            .multi_external_input(&key(), vec![])
            .unwrap()
            .is_some()
    );
    let mut v = raw(true, true);
    let e = v["blocks"][0]["element"].clone();
    v["blocks"][0] = json!({"type":"section","block_id":"choice","accessory":e});
    assert!(HomeForm::parse(&v).unwrap().state().is_err());
    let mut malformed = option("Bad").raw().clone();
    malformed["text"]["type"] = json!("mrkdwn");
    assert!(ExternalOption::parse(&malformed).is_err());
    malformed = option("Bad").raw().clone();
    malformed["value"] = json!("a".repeat(151));
    assert!(ExternalOption::parse(&malformed).is_err());
}

#[test]
fn first_parent_seed_keeps_full_initial_including_empty_value_while_fresh_parent_projects() {
    for empty in [false, true] {
        let mut v = raw(true, true);
        if empty {
            v["blocks"][0]["element"]["initial_option"]["value"] = json!("");
        }
        let f = HomeForm::parse(&v).unwrap();
        let initial = ExternalOption::parse(&v["blocks"][0]["element"]["initial_option"]).unwrap();
        let before = f.clone();
        assert_eq!(
            f.external_initial_seed(&key()).unwrap(),
            Some(initial.clone())
        );
        assert_eq!(
            f.external_seed(&key()).unwrap().unwrap().raw(),
            &json!({"text":initial.raw()["text"], "value":initial.value()})
        );
        assert_eq!(wire(&f)["selected_option"], *initial.raw());
        assert_eq!(f, before);
    }
    for initial in [None, Some(Value::Null)] {
        let mut v = raw(true, true);
        v["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_option");
        if let Some(initial) = initial {
            v["blocks"][0]["element"]["initial_option"] = initial;
        }
        let f = HomeForm::parse(&v).unwrap();
        assert_eq!(f.external_initial_seed(&key()).unwrap(), None);
        assert_eq!(wire(&f), json!({"type":"external_select"}));
    }
}

#[test]
fn initial_seed_preserves_admitted_saved_ingress_and_never_rewrites_parent() {
    for mode in 0..4 {
        let mut v = raw(true, true);
        let mut row = json!({"type":"external_select"});
        if mode == 1 {
            row["selected_option"] = Value::Null;
        }
        if mode >= 2 {
            row["selected_option"] = option("Saved").raw().clone();
            if mode == 3 {
                row["selected_option"]["value"] = json!("");
            }
        }
        v["state"] = json!({"values":{"choice":{"pick":row}}});
        let f = HomeForm::parse(&v).unwrap();
        let before = f.clone();
        let seed = f.external_initial_seed(&key()).unwrap().unwrap();
        if mode < 2 {
            assert_eq!(seed, option("Initial"));
            assert_eq!(wire(&f)["selected_option"], *option("Initial").raw());
        } else {
            assert_eq!(
                seed.raw(),
                &json!({"text":row["selected_option"]["text"],"value":row["selected_option"]["value"]})
            );
            assert_eq!(wire(&f)["selected_option"], row["selected_option"]);
        }
        assert_eq!(f, before);
    }
    let mut f = HomeForm::parse(&raw(true, true)).unwrap();
    f.fields.get_mut(&key()).unwrap().value = InputValue::External(Some(option("Untrusted")));
    assert!(f.external_initial_seed(&key()).is_err());
    let mut f = HomeForm::parse(&raw(true, true)).unwrap();
    f.set_native_error(&key(), "unseen".into(), "Safety".into())
        .unwrap();
    assert!(f.external_initial_seed(&key()).is_err());
}
