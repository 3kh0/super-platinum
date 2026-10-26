use super::*;
fn key() -> InputKey {
    InputKey {
        block_id: "channels".into(),
        action_id: "pick".into(),
    }
}
fn raw(optional: bool, quiet: bool) -> Value {
    json!({"type":"home","id":"V1","app_id":"A1","app_installed_team_id":"T1","bot_id":"B1","blocks":[{"type":"input","block_id":"channels","optional":optional,"dispatch_action":!quiet,"element":{"type":"multi_channels_select","action_id":"pick","initial_channels":["CINITIAL1"],"max_selected_items":2,"response_url_enabled":true,"placeholder":{"type":"plain_text","text":"Ignored"}}}]})
}
fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).into()).collect()
}
fn row(f: &HomeForm) -> Value {
    f.state().unwrap()["values"]["channels"]["pick"].clone()
}
fn retain(f: &mut HomeForm, v: &Value, t: &[ChannelParent]) -> Result<(), FormError> {
    f.reconcile_with_lookup_parents(v, &[], &[], &[], &[], t)
}
#[test]
fn pristine_omission_differs_from_initial_and_callback_empty() {
    for quiet in [false, true] {
        for optional in [false, true] {
            for initial in [None, Some(Value::Null), Some(json!([]))] {
                let mut v = raw(optional, quiet);
                let e = v["blocks"][0]["element"].as_object_mut().unwrap();
                e.remove("initial_channels");
                if let Some(ref a) = initial {
                    e.insert("initial_channels".into(), a.clone());
                }
                let mut f = HomeForm::parse(&v).unwrap();
                assert_eq!(
                    row(&f).get("selected_channels").is_some(),
                    initial == Some(json!([]))
                );
                assert_eq!(f.multi_channel_seed(&key()).unwrap(), Vec::<String>::new());
                let event = f.multi_channel_input(&key(), vec![]).unwrap();
                assert_eq!(event.is_some(), optional && !quiet);
                assert_eq!(
                    row(&f),
                    json!({"type":"multi_channels_select","selected_channels":[]})
                );
                assert_eq!(f.fields[&key()].error.is_some(), !optional);
            }
        }
    }
}
#[test]
fn ordered_callbacks_metadata_and_repeat_are_exact() {
    let mut v = raw(true, false);
    let mut f = HomeForm::parse(&v).unwrap();
    let selected = ids(&["GSELECT01", "ZSELECT01"]);
    let first = f
        .multi_channel_input(&key(), selected.clone())
        .unwrap()
        .unwrap();
    assert_eq!(
        first.action(),
        &json!({"type":"multi_channels_select","block_id":"channels","action_id":"pick","selected_channels":selected,"initial_channels":["CINITIAL1"]})
    );
    assert_eq!(
        f.multi_channel_input(&key(), selected).unwrap().unwrap(),
        first
    );
    v["blocks"][0]["element"]["initial_channels"] = json!([]);
    f.reconcile(&v).unwrap();
    let next = f
        .multi_channel_input(&key(), ids(&["CSELECT01"]))
        .unwrap()
        .unwrap();
    assert_eq!(next.action()["initial_channels"], json!([]));
    assert_eq!(first.action()["initial_channels"], json!(["CINITIAL1"]));
    v["blocks"][0]["element"]
        .as_object_mut()
        .unwrap()
        .remove("initial_channels");
    f.reconcile(&v).unwrap();
    assert!(
        f.multi_channel_input(&key(), vec![])
            .unwrap()
            .unwrap()
            .action()
            .get("initial_channels")
            .is_none()
    );
}
#[test]
fn required_and_overmax_store_before_new_event_suppression() {
    for quiet in [false, true] {
        let mut f = HomeForm::parse(&raw(false, quiet)).unwrap();
        let a = f.multi_channel_input(&key(), ids(&["CSELECT01"])).unwrap();
        assert_eq!(a.is_some(), !quiet);
        for v in [ids(&["CSELECT03", "CSELECT01", "CSELECT02"]), vec![]] {
            assert!(f.multi_channel_input(&key(), v.clone()).unwrap().is_none());
            assert_eq!(row(&f)["selected_channels"], json!(v));
            assert!(f.fields[&key()].error.is_some());
        }
        if let Some(a) = a {
            assert_eq!(a.action()["selected_channels"], json!(["CSELECT01"]));
        }
    }
}
#[test]
fn structural_failures_are_atomic_and_limit_is_still_one_thousand() {
    let mut f = HomeForm::parse(&raw(true, false)).unwrap();
    for v in [
        ids(&["CSELECT01", "CSELECT01"]),
        ids(&[""]),
        ids(&["Cbad"]),
        ids(&["UUSER0001"]),
        (0..1001).map(|n| format!("C{n:08}")).collect(),
    ] {
        let before = f.clone();
        assert!(f.multi_channel_input(&key(), v).is_err());
        assert_eq!(f, before);
    }
    let full: Vec<String> = (0..1000).map(|n| format!("C{n:08}")).collect();
    assert!(
        f.multi_channel_input(&key(), full.clone())
            .unwrap()
            .is_none()
    );
    assert_eq!(row(&f)["selected_channels"], json!(full));
    let before = f.clone();
    assert!(
        f.multi_channel_input(
            &InputKey {
                block_id: "foreign".into(),
                action_id: "pick".into()
            },
            vec![]
        )
        .is_err()
    );
    assert_eq!(f, before);
    assert!(f.channel_seed(&key()).is_err());
    assert!(f.channel_input(&key(), None).is_err());
}
#[test]
fn initialized_parent_preserves_pristine_dirty_omission_and_order() {
    for dirty in [false, true] {
        let mut v = raw(true, false);
        v["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_channels");
        let mut f = HomeForm::parse(&v).unwrap();
        let t = f.multi_channel_parent(&key()).unwrap();
        if dirty {
            f.multi_channel_input(&key(), ids(&["CSELECT02", "CSELECT01"]))
                .unwrap();
        }
        let before = row(&f);
        for initial in [
            Some(json!(["CNEWINIT1"])),
            Some(json!([])),
            Some(Value::Null),
            None,
        ] {
            let e = v["blocks"][0]["element"].as_object_mut().unwrap();
            e.remove("initial_channels");
            if let Some(a) = initial {
                e.insert("initial_channels".into(), a);
            }
            v["blocks"][0]["element"]["max_selected_items"] = json!(1);
            v["state"] = json!({"values":{"channels":{"pick":{"type":"multi_channels_select","selected_channels":["CSAVED001"]}}}});
            retain(&mut f, &v, &[t.clone()]).unwrap();
            assert_eq!(row(&f), before);
            assert_eq!(f.fields[&key()].dirty, dirty);
            assert_eq!(f.fields[&key()].error.is_some(), dirty);
        }
    }
}
#[test]
fn omitted_parent_seed_falls_back_but_explicit_empty_wins() {
    let mut v = raw(true, false);
    v["blocks"][0]["element"]
        .as_object_mut()
        .unwrap()
        .remove("initial_channels");
    let mut f = HomeForm::parse(&v).unwrap();
    let t = f.multi_channel_parent(&key()).unwrap();
    v["blocks"][0]["element"]["initial_channels"] = json!(["CNEWINIT1"]);
    retain(&mut f, &v, &[t.clone()]).unwrap();
    assert!(row(&f).get("selected_channels").is_none());
    assert_eq!(f.multi_channel_seed(&key()).unwrap(), ids(&["CNEWINIT1"]));
    f.multi_channel_input(&key(), vec![]).unwrap();
    retain(&mut f, &v, &[t]).unwrap();
    assert_eq!(f.multi_channel_seed(&key()).unwrap(), Vec::<String>::new());
}
#[test]
fn parent_tokens_refuse_foreign_owner_type_removed_and_duplicate() {
    let v = raw(true, false);
    let f = HomeForm::parse(&v).unwrap();
    let t = f.multi_channel_parent(&key()).unwrap();
    for prop in ["id", "app_id", "bot_id", "app_installed_team_id"] {
        let mut n = v.clone();
        n[prop] = json!("OTHER");
        let mut candidate = f.clone();
        assert!(retain(&mut candidate, &n, &[t.clone()]).is_err());
        assert_eq!(candidate, f);
    }
    let mut single = v.clone();
    single["blocks"][0]["element"] = json!({"type":"channels_select","action_id":"pick"});
    assert!(retain(&mut f.clone(), &single, &[t.clone()]).is_err());
    let mut removed = v.clone();
    removed["blocks"] = json!([]);
    assert!(retain(&mut f.clone(), &removed, &[t.clone()]).is_err());
    assert!(retain(&mut f.clone(), &v, &[t.clone(), t]).is_err());
}
#[test]
fn public_or_private_corruption_refuses_seed_parent_event_state() {
    for mode in 0..11 {
        let mut f = HomeForm::parse(&raw(false, false)).unwrap();
        match mode {
            0 => {
                f.fields.get_mut(&key()).unwrap().value = InputValue::Channels(ids(&["COTHER001"]))
            }
            1 => f.fields.get_mut(&key()).unwrap().raw["initial_channels"] = json!([]),
            2 => f.fields.get_mut(&key()).unwrap().optional = true,
            3 => f.fields.get_mut(&key()).unwrap().kind = InputKind::Channel,
            4 => {
                f.fields.remove(&key());
            }
            5 => f.fields.get_mut(&key()).unwrap().error = Some("fabricated".into()),
            6 => {
                f.wire.insert(
                    key(),
                    json!({"type":"multi_channels_select","selected_channels":[]}),
                );
            }
            7 => {
                f.wire.remove(&key());
            }
            8 => f.wire.get_mut(&key()).unwrap()["response_url_enabled"] = json!(true),
            9 => f.wire.get_mut(&key()).unwrap()["selected_channels"] = Value::Null,
            _ => {
                f.fields.get_mut(&key()).unwrap().kind = InputKind::MultiChannel {
                    max_selected_items: Some(3),
                }
            }
        }
        assert!(f.multi_channel_seed(&key()).is_err());
        assert!(f.multi_channel_parent(&key()).is_err());
        assert!(f.multi_channel_input(&key(), vec![]).is_err());
        assert!(f.state().is_err());
    }
}
#[test]
fn raw_saved_ingress_and_unattested_reconcile_remain_existing_policy() {
    for saved in [json!([]), json!(["CSAVED001"])] {
        let mut v = raw(true, false);
        v["state"] = json!({"values":{"channels":{"pick":{"type":"multi_channels_select","selected_channels":saved}}}});
        let f = HomeForm::parse(&v).unwrap();
        assert_eq!(json!(f.multi_channel_seed(&key()).unwrap()), saved);
    }
    for saved in [
        json!({"type":"multi_channels_select"}),
        json!({"type":"multi_channels_select","selected_channels":null}),
    ] {
        let mut v = raw(true, false);
        v["state"] = json!({"values":{"channels":{"pick":saved}}});
        let f = HomeForm::parse(&v).unwrap();
        assert_eq!(row(&f)["selected_channels"], json!(["CINITIAL1"]));
        assert_eq!(f.multi_channel_seed(&key()).unwrap(), ids(&["CINITIAL1"]));
    }
    let mut v = raw(true, false);
    let mut f = HomeForm::parse(&v).unwrap();
    v["blocks"][0]["element"]["initial_channels"] = json!(["CNEWINIT1"]);
    f.reconcile(&v).unwrap();
    assert_eq!(row(&f)["selected_channels"], json!(["CNEWINIT1"]));
}

#[test]
fn single_and_multi_parents_share_vector_without_cross_kind_authority() {
    let mut v = raw(true, false);
    v["blocks"].as_array_mut().unwrap().push(json!({"type":"input","block_id":"single","optional":true,"dispatch_action":true,"element":{"type":"channels_select","action_id":"pick","initial_channel":"CSINGLE01"}}));
    let single = InputKey {
        block_id: "single".into(),
        action_id: "pick".into(),
    };
    let mut f = HomeForm::parse(&v).unwrap();
    let tokens = vec![
        f.multi_channel_parent(&key()).unwrap(),
        f.channel_parent(&single).unwrap(),
    ];
    let before = f.state().unwrap();
    v["blocks"][0]["element"]["initial_channels"] = json!([]);
    v["blocks"][1]["element"]["initial_channel"] = json!("CNEWINIT1");
    retain(&mut f, &v, &tokens).unwrap();
    assert_eq!(f.state().unwrap(), before);
    assert!(f.multi_channel_parent(&single).is_err());
    assert!(f.channel_parent(&key()).is_err());
    assert!(f.multi_channel_input(&single, vec![]).is_err());
    assert!(f.channel_input(&key(), None).is_err());
}

#[test]
fn native_safety_source_flags_and_unproved_max_zero_stay_refused() {
    let mut f = HomeForm::parse(&raw(true, false)).unwrap();
    f.set_native_error(&key(), "".into(), "Unresolved native buffer".into())
        .unwrap();
    assert!(f.multi_channel_seed(&key()).is_err());
    assert!(f.multi_channel_parent(&key()).is_err());
    assert!(f.state().is_err());
    for flag in [
        "confirm",
        "dispatch_action_config",
        "visible_to_user_ids",
        "filter",
        "service_team_id",
        "default_to_current_conversation",
        "show_enterprise_options",
    ] {
        let mut v = raw(true, false);
        v["blocks"][0]["element"][flag] = json!({});
        if let Ok(mut f) = HomeForm::parse(&v) {
            assert!(f.multi_channel_seed(&key()).is_err());
            assert!(f.multi_channel_input(&key(), vec![]).is_err());
        }
    }
    let mut v = raw(true, false);
    v["blocks"][0]["element"]["max_selected_items"] = json!(0);
    assert!(HomeForm::parse(&v).is_err());
    let mut v = raw(true, false);
    v["blocks"][0]["type"] = json!("section");
    v["blocks"][0]["accessory"] = v["blocks"][0]["element"].clone();
    if let Ok(mut f) = HomeForm::parse(&v) {
        assert!(f.multi_channel_input(&key(), vec![]).is_err());
    }
}
