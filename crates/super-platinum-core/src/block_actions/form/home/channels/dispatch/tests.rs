use super::*;

fn key() -> InputKey {
    InputKey {
        block_id: "channel".into(),
        action_id: "pick".into(),
    }
}
fn raw(optional: bool, quiet: bool) -> Value {
    json!({"type":"home","id":"V1","app_id":"A1","app_installed_team_id":"T1","bot_id":"B1","blocks":[
        {"type":"input","block_id":"channel","optional":optional,"dispatch_action":!quiet,
        "element":{"type":"channels_select","action_id":"pick","initial_channel":"CINITIAL1",
        "response_url_enabled":true,"placeholder":{"type":"plain_text","text":"Not action metadata"}}}]})
}
fn row(f: &HomeForm) -> Value {
    f.state().unwrap()["values"]["channel"]["pick"].clone()
}
fn retain(f: &mut HomeForm, raw: &Value, parents: &[ChannelParent]) -> Result<(), FormError> {
    f.reconcile_with_lookup_parents(raw, &[], &[], &[], &[], parents)
}

#[test]
fn pristine_channel_wire_omits_response_bit_and_empty_selection() {
    for quiet in [false, true] {
        for optional in [false, true] {
            for initial in [
                None,
                Some(Value::Null),
                Some(json!("")),
                Some(json!("CINITIAL1")),
            ] {
                let mut v = raw(optional, quiet);
                let e = v["blocks"][0]["element"].as_object_mut().unwrap();
                e.remove("initial_channel");
                if let Some(initial) = &initial {
                    e.insert("initial_channel".into(), initial.clone());
                }
                let mut f = HomeForm::parse(&v).unwrap();
                let mut expected = json!({"type":"channels_select"});
                if initial == Some(json!("CINITIAL1")) {
                    expected["selected_channel"] = json!("CINITIAL1");
                }
                assert_eq!(row(&f), expected);
                let before = row(&f);
                assert_eq!(
                    f.channel_seed(&key()).unwrap().as_deref(),
                    (initial == Some(json!("CINITIAL1"))).then_some("CINITIAL1")
                );
                assert_eq!(row(&f), before);
                assert_eq!(
                    f.channel_input(&key(), None).unwrap().is_some(),
                    optional && !quiet
                );
                assert_eq!(
                    row(&f),
                    json!({"type":"channels_select","selected_channel":null,"response_url_enabled":true})
                );
                assert_eq!(f.fields[&key()].error.is_some(), !optional);
            }
        }
    }
}

#[test]
fn channel_callback_metadata_repeat_and_response_flag_lifetime() {
    let mut v = raw(true, false);
    let mut f = HomeForm::parse(&v).unwrap();
    let a = f
        .channel_input(&key(), Some("GSELECT01".into()))
        .unwrap()
        .unwrap();
    assert_eq!(
        a.action(),
        &json!({"type":"channels_select","block_id":"channel","action_id":"pick",
        "selected_channel":"GSELECT01","initial_channel":"CINITIAL1","response_url_enabled":true})
    );
    assert_eq!(
        f.channel_input(&key(), Some("GSELECT01".into()))
            .unwrap()
            .unwrap(),
        a
    );
    let token = f.channel_parent(&key()).unwrap();
    v["blocks"][0]["element"]["initial_channel"] = json!("CNEWINIT1");
    v["blocks"][0]["element"]["response_url_enabled"] = json!(false);
    retain(&mut f, &v, &[token]).unwrap();
    assert_eq!(
        row(&f),
        json!({"type":"channels_select","selected_channel":"GSELECT01","response_url_enabled":true})
    );
    let b = f
        .channel_input(&key(), Some("ZSELECT01".into()))
        .unwrap()
        .unwrap();
    assert_eq!(
        b.action(),
        &json!({"type":"channels_select","block_id":"channel","action_id":"pick",
        "selected_channel":"ZSELECT01","initial_channel":"CNEWINIT1"})
    );
    assert!(row(&f).get("response_url_enabled").is_none());
    assert_eq!(a.action()["initial_channel"], "CINITIAL1");
    assert!(a.same_view(&raw(true, false)));
    assert!(!a.same_view(&v));
}

#[test]
fn initialized_channel_parent_preserves_pristine_dirty_and_omitted_wire() {
    for mode in 0..3 {
        let mut v = raw(true, false);
        if mode == 2 {
            v["blocks"][0]["element"]
                .as_object_mut()
                .unwrap()
                .remove("initial_channel");
        }
        let mut f = HomeForm::parse(&v).unwrap();
        let token = f.channel_parent(&key()).unwrap();
        if mode == 1 {
            f.channel_input(&key(), None).unwrap();
        }
        let before = row(&f);
        v["blocks"][0]["element"]["initial_channel"] = json!("CNEWINIT1");
        v["blocks"][0]["element"]["response_url_enabled"] = json!(false);
        v["state"] = json!({"values":{"channel":{"pick":{"type":"channels_select","selected_channel":"CSAVED001"}}}});
        retain(&mut f, &v, &[token.clone()]).unwrap();
        assert_eq!(row(&f), before);
        assert_eq!(f.fields[&key()].dirty, mode == 1);
        assert_eq!(
            f.channel_seed(&key()).unwrap().as_deref(),
            Some(if mode == 0 { "CINITIAL1" } else { "CNEWINIT1" })
        );
        assert_eq!(row(&f), before); // Fresh displayed fallback never commits.
        v["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_channel");
        retain(&mut f, &v, &[token]).unwrap();
        assert_eq!(row(&f), before);
        assert_eq!(
            f.channel_seed(&key()).unwrap().as_deref(),
            (mode == 0).then_some("CINITIAL1")
        );
    }
    let mut f = HomeForm::parse(&raw(true, false)).unwrap();
    let mut changed = raw(true, false);
    changed["blocks"][0]["element"]["initial_channel"] = json!("CNEWINIT1");
    f.reconcile_with_initialized_parents(&changed, &[], &[], &[], &[])
        .unwrap();
    assert_eq!(
        row(&f),
        json!({"type":"channels_select","selected_channel":"CNEWINIT1"})
    );
}

#[test]
fn existing_raw_saved_policy_stays_separate_from_mapped_constructor() {
    for saved in [
        json!({"type":"channels_select"}),
        json!({"type":"channels_select","selected_channel":null}),
        json!({"type":"channels_select","selected_channel":"CSAVED001"}),
    ] {
        let mut v = raw(true, false);
        v["state"] = json!({"values":{"channel":{"pick":saved}}});
        let f = HomeForm::parse(&v).unwrap();
        assert_eq!(row(&f)["selected_channel"], saved["selected_channel"]);
        assert!(row(&f).get("selected_channel").is_some());
        // Existing generic raw hydration uses current source response flag.
        assert_eq!(row(&f)["response_url_enabled"], true);
        assert_eq!(
            f.channel_seed(&key()).unwrap().as_deref(),
            Some(saved["selected_channel"].as_str().unwrap_or("CINITIAL1"))
        );
    }
    for saved in [json!(""), json!("Cbad"), json!("USELECT01"), json!(7)] {
        let mut v = raw(true, false);
        v["state"] = json!({"values":{"channel":{"pick":{"type":"channels_select","selected_channel":saved}}}});
        if let Ok(f) = HomeForm::parse(&v) {
            assert!(f.state().is_err());
            assert!(f.channel_seed(&key()).is_err());
        }
    }
}

#[test]
fn channel_invalid_candidate_and_public_or_private_corruption_refuse_atomically() {
    let f = HomeForm::parse(&raw(true, false)).unwrap();
    for id in ["", "C1", "Cbad", "USELECT01", "C SPACE01"] {
        let mut candidate = f.clone();
        assert!(candidate.channel_input(&key(), Some(id.into())).is_err());
        assert_eq!(candidate, f);
    }
    for mode in 0..9 {
        let mut corrupt = f.clone();
        match mode {
            0 => corrupt.fields.get_mut(&key()).unwrap().value = InputValue::Channel(None),
            1 => corrupt.fields.get_mut(&key()).unwrap().raw["response_url_enabled"] = json!(false),
            2 => corrupt.fields.get_mut(&key()).unwrap().optional = false,
            3 => corrupt.fields.get_mut(&key()).unwrap().kind = InputKind::User,
            4 => corrupt.fields.get_mut(&key()).unwrap().error = Some("forged".into()),
            5 => {
                corrupt.fields.remove(&key());
            }
            6 => {
                corrupt.wire.remove(&key());
            }
            7 => corrupt.wire.get_mut(&key()).unwrap()["response_url_enabled"] = json!(false),
            _ => corrupt.wire.get_mut(&key()).unwrap()["extra"] = json!(true),
        }
        assert!(corrupt.state().is_err(), "mode {mode}");
        assert!(corrupt.channel_seed(&key()).is_err());
        let before = corrupt.clone();
        assert!(corrupt.channel_input(&key(), None).is_err());
        assert_eq!(corrupt, before);
    }
}

#[test]
fn channel_parent_identity_duplicates_and_kind_changes_refuse_without_mutation() {
    let v = raw(true, false);
    let f = HomeForm::parse(&v).unwrap();
    let token = f.channel_parent(&key()).unwrap();
    for mode in 0..7 {
        let mut next = v.clone();
        match mode {
            0 => next["id"] = json!("V2"),
            1 => next["app_id"] = json!("A2"),
            2 => next["app_installed_team_id"] = json!("T2"),
            3 => next["bot_id"] = json!("B2"),
            4 => next["blocks"] = json!([]),
            5 => next["blocks"][0]["element"] = json!({"type":"users_select","action_id":"pick"}),
            _ => next["blocks"][0]["element"]["action_id"] = json!("other"),
        }
        let mut candidate = f.clone();
        assert!(retain(&mut candidate, &next, &[token.clone()]).is_err());
        assert_eq!(candidate, f);
    }
    let mut candidate = f.clone();
    assert!(retain(&mut candidate, &v, &[token.clone(), token]).is_err());
    assert_eq!(candidate, f);
}

#[test]
fn single_channel_admission_does_not_grant_other_lookup_or_message_clear_authority() {
    for kind in ["multi_channels_select", "conversations_select"] {
        let mut v = raw(true, false);
        v["blocks"][0]["element"] = json!({"type":kind,"action_id":"pick"});
        let mut f = HomeForm::parse(&v).unwrap();
        assert!(f.state().is_ok());
        assert!(f.channel_input(&key(), None).is_err());
        v["blocks"][0]["dispatch_action"] = json!(false);
        let mut f = HomeForm::parse(&v).unwrap();
        assert!(f.state().is_ok());
        assert!(f.channel_input(&key(), None).is_err());
    }
    for setting in [
        "confirm",
        "dispatch_action_config",
        "visible_to_user_ids",
        "filter",
        "service_team_id",
    ] {
        let mut v = raw(true, false);
        v["blocks"][0]["element"][setting] = json!({});
        if let Ok(f) = HomeForm::parse(&v) {
            assert!(f.state().is_err());
        }
    }
    let field = parse_message_channel(&raw(true, false)["blocks"][0]["element"]).unwrap();
    assert!(validate_message_channel(&field, None).is_err());
}
