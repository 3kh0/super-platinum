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
        "element":{"type":"conversations_select","action_id":"pick","initial_conversation":"CINITIAL1",
        "response_url_enabled":true,"placeholder":{"type":"plain_text","text":"Not action metadata"}}}]})
}
fn row(f: &HomeForm) -> Value {
    f.state().unwrap()["values"]["channel"]["pick"].clone()
}
fn retain(f: &mut HomeForm, raw: &Value, parents: &[ConversationParent]) -> Result<(), FormError> {
    f.reconcile_with_all_lookup_parents(raw, &[], &[], &[], &[], &[], parents)
}

#[test]
fn pristine_conversation_wire_omits_response_bit_and_empty_selection() {
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
                e.remove("initial_conversation");
                if let Some(initial) = &initial {
                    e.insert("initial_conversation".into(), initial.clone());
                }
                let mut f = HomeForm::parse(&v).unwrap();
                let mut expected = json!({"type":"conversations_select"});
                if initial == Some(json!("CINITIAL1")) {
                    expected["selected_conversation"] = json!("CINITIAL1");
                }
                assert_eq!(row(&f), expected);
                let before = row(&f);
                assert_eq!(
                    f.conversation_seed(&key()).unwrap().as_deref(),
                    (initial == Some(json!("CINITIAL1"))).then_some("CINITIAL1")
                );
                assert_eq!(row(&f), before);
                assert_eq!(
                    f.conversation_input(&key(), None).unwrap().is_some(),
                    optional && !quiet
                );
                assert_eq!(
                    row(&f),
                    json!({"type":"conversations_select","selected_conversation":null,"response_url_enabled":true})
                );
                assert_eq!(f.fields[&key()].error.is_some(), !optional);
            }
        }
    }
}

#[test]
fn conversation_callback_metadata_repeat_and_response_flag_lifetime() {
    let mut v = raw(true, false);
    let mut f = HomeForm::parse(&v).unwrap();
    let a = f
        .conversation_input(&key(), Some("GSELECT01".into()))
        .unwrap()
        .unwrap();
    assert_eq!(
        a.action(),
        &json!({"type":"conversations_select","block_id":"channel","action_id":"pick",
        "selected_conversation":"GSELECT01","initial_conversation":"CINITIAL1","response_url_enabled":true})
    );
    assert_eq!(
        f.conversation_input(&key(), Some("GSELECT01".into()))
            .unwrap()
            .unwrap(),
        a
    );
    let token = f.conversation_parent(&key()).unwrap();
    v["blocks"][0]["element"]["initial_conversation"] = json!("CNEWINIT1");
    v["blocks"][0]["element"]["response_url_enabled"] = json!(false);
    retain(&mut f, &v, &[token]).unwrap();
    assert_eq!(
        row(&f),
        json!({"type":"conversations_select","selected_conversation":"GSELECT01","response_url_enabled":true})
    );
    let b = f
        .conversation_input(&key(), Some("ZSELECT01".into()))
        .unwrap()
        .unwrap();
    assert_eq!(
        b.action(),
        &json!({"type":"conversations_select","block_id":"channel","action_id":"pick",
        "selected_conversation":"ZSELECT01","initial_conversation":"CNEWINIT1"})
    );
    assert!(row(&f).get("response_url_enabled").is_none());
    assert_eq!(a.action()["initial_conversation"], "CINITIAL1");
    assert!(a.same_view(&raw(true, false)));
    assert!(!a.same_view(&v));
}

#[test]
fn initialized_conversation_parent_preserves_pristine_dirty_and_omitted_wire() {
    for mode in 0..3 {
        let mut v = raw(true, false);
        if mode == 2 {
            v["blocks"][0]["element"]
                .as_object_mut()
                .unwrap()
                .remove("initial_conversation");
        }
        let mut f = HomeForm::parse(&v).unwrap();
        let token = f.conversation_parent(&key()).unwrap();
        if mode == 1 {
            f.conversation_input(&key(), None).unwrap();
        }
        let before = row(&f);
        v["blocks"][0]["element"]["initial_conversation"] = json!("CNEWINIT1");
        v["blocks"][0]["element"]["response_url_enabled"] = json!(false);
        v["state"] = json!({"values":{"channel":{"pick":{"type":"conversations_select","selected_conversation":"CSAVED001"}}}});
        retain(&mut f, &v, &[token.clone()]).unwrap();
        assert_eq!(row(&f), before);
        assert_eq!(f.fields[&key()].dirty, mode == 1);
        assert_eq!(
            f.conversation_seed(&key()).unwrap().as_deref(),
            Some(if mode == 0 { "CINITIAL1" } else { "CNEWINIT1" })
        );
        assert_eq!(row(&f), before); // Fresh displayed fallback never commits.
        v["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_conversation");
        retain(&mut f, &v, &[token]).unwrap();
        assert_eq!(row(&f), before);
        assert_eq!(
            f.conversation_seed(&key()).unwrap().as_deref(),
            (mode == 0).then_some("CINITIAL1")
        );
    }
    let mut f = HomeForm::parse(&raw(true, false)).unwrap();
    let mut changed = raw(true, false);
    changed["blocks"][0]["element"]["initial_conversation"] = json!("CNEWINIT1");
    f.reconcile_with_initialized_parents(&changed, &[], &[], &[], &[])
        .unwrap();
    assert_eq!(
        row(&f),
        json!({"type":"conversations_select","selected_conversation":"CNEWINIT1"})
    );
}

#[test]
fn existing_raw_saved_policy_stays_separate_from_mapped_constructor() {
    for saved in [
        json!({"type":"conversations_select"}),
        json!({"type":"conversations_select","selected_conversation":null}),
        json!({"type":"conversations_select","selected_conversation":"CSAVED001"}),
    ] {
        let mut v = raw(true, false);
        v["state"] = json!({"values":{"channel":{"pick":saved}}});
        let f = HomeForm::parse(&v).unwrap();
        assert_eq!(
            row(&f)["selected_conversation"],
            saved["selected_conversation"]
        );
        assert!(row(&f).get("selected_conversation").is_some());
        // Existing generic raw hydration uses current source response flag.
        assert_eq!(row(&f)["response_url_enabled"], true);
        assert_eq!(
            f.conversation_seed(&key()).unwrap().as_deref(),
            Some(
                saved["selected_conversation"]
                    .as_str()
                    .unwrap_or("CINITIAL1")
            )
        );
    }
    for saved in [json!(""), json!("Cbad"), json!("TSELECT01"), json!(7)] {
        let mut v = raw(true, false);
        v["state"] = json!({"values":{"channel":{"pick":{"type":"conversations_select","selected_conversation":saved}}}});
        if let Ok(f) = HomeForm::parse(&v) {
            assert!(f.state().is_err());
            assert!(f.conversation_seed(&key()).is_err());
        }
    }
}

#[test]
fn conversation_invalid_candidate_and_public_or_private_corruption_refuse_atomically() {
    let f = HomeForm::parse(&raw(true, false)).unwrap();
    for id in ["", "C1", "Cbad", "TSELECT01", "C SPACE01"] {
        let mut candidate = f.clone();
        assert!(
            candidate
                .conversation_input(&key(), Some(id.into()))
                .is_err()
        );
        assert_eq!(candidate, f);
    }
    for mode in 0..9 {
        let mut corrupt = f.clone();
        match mode {
            0 => corrupt.fields.get_mut(&key()).unwrap().value = InputValue::Conversation(None),
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
        assert!(corrupt.conversation_seed(&key()).is_err());
        let before = corrupt.clone();
        assert!(corrupt.conversation_input(&key(), None).is_err());
        assert_eq!(corrupt, before);
    }
}

#[test]
fn conversation_parent_identity_duplicates_and_kind_changes_refuse_without_mutation() {
    let v = raw(true, false);
    let f = HomeForm::parse(&v).unwrap();
    let token = f.conversation_parent(&key()).unwrap();
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
fn conversation_ids_and_action_metadata_do_not_create_directory_or_default_authority() {
    let mut v = raw(true, false);
    v["blocks"][0]["element"]["default_to_current_conversation"] = json!(true);
    v["blocks"][0]["element"]["filter"] = json!({"include":["private","im"],"only_from_team_id":"EENTER001","exclude_bot_users":true});
    v["blocks"][0]["element"]
        .as_object_mut()
        .unwrap()
        .remove("initial_conversation");
    let mut f = HomeForm::parse(&v).unwrap();
    assert_eq!(f.conversation_seed(&key()).unwrap(), None);
    assert_eq!(row(&f), json!({"type":"conversations_select"}));
    for id in [
        "CSELECT01",
        "GSELECT01",
        "DSELECT01",
        "ZSELECT01",
        "UMEMBER01",
    ] {
        let event = f
            .conversation_input(&key(), Some(id.into()))
            .unwrap()
            .unwrap();
        assert_eq!(
            event.action(),
            &json!({"type":"conversations_select","block_id":"channel",
            "action_id":"pick","selected_conversation":id,"response_url_enabled":true})
        );
        assert_eq!(f.conversation_seed(&key()).unwrap().as_deref(), Some(id));
    }
    assert_eq!(f.raw_view(), &v);
    let filter = ConversationFilter::parse(&f.fields[&key()].raw).unwrap();
    assert!(!filter.allows("public")); // Syntax is not directory eligibility.
    assert!(filter.allows("im"));
    assert_eq!(filter.only_from_team_id.as_deref(), Some("EENTER001"));
}

#[test]
fn single_conversation_api_does_not_grant_multi_or_noninput_dispatch() {
    let mut v = raw(true, false);
    v["blocks"][0]["element"] = json!({"type":"multi_conversations_select","action_id":"pick"});
    for dispatch in [true, false] {
        v["blocks"][0]["dispatch_action"] = json!(dispatch);
        let mut f = HomeForm::parse(&v).unwrap();
        assert!(f.conversation_field(&key()).is_ok());
        assert!(f.state().is_ok());
        assert!(f.conversation_seed(&key()).is_err());
        assert!(f.conversation_parent(&key()).is_err());
        let before = f.clone();
        assert!(f.conversation_input(&key(), None).is_err());
        assert_eq!(f, before);
        assert_eq!(
            f.multi_conversation_input(&key(), vec!["UMEMBER01".into()])
                .unwrap()
                .is_some(),
            dispatch
        );
    }
    for setting in ["confirm", "dispatch_action_config", "visible_to_user_ids"] {
        let mut v = raw(true, false);
        v["blocks"][0]["element"][setting] = json!({});
        let mut f = HomeForm::parse(&v).unwrap();
        assert!(f.state().is_err());
        assert!(f.conversation_input(&key(), None).is_err());
    }
    let mut v = raw(true, false);
    v["blocks"][0]["type"] = json!("section");
    v["blocks"][0]["accessory"] = v["blocks"][0]["element"].clone();
    assert!(HomeForm::parse(&v).unwrap().state().is_err());
}

#[test]
fn distinct_lookup_parent_tokens_retain_only_their_own_initialized_fields() {
    let mut v = raw(true, false);
    v["blocks"].as_array_mut().unwrap().extend([
        json!({"type":"input","block_id":"public","optional":true,"element":{"type":"channels_select","action_id":"pick","initial_channel":"CINITIAL1"}}),
        json!({"type":"input","block_id":"user","optional":true,"element":{"type":"users_select","action_id":"pick","initial_user":"UINITIAL1"}}),
    ]);
    let public = InputKey {
        block_id: "public".into(),
        action_id: "pick".into(),
    };
    let user = InputKey {
        block_id: "user".into(),
        action_id: "pick".into(),
    };
    let mut f = HomeForm::parse(&v).unwrap();
    assert!(f.channel_parent(&key()).is_err());
    assert!(f.conversation_parent(&public).is_err());
    let conversation = f.conversation_parent(&key()).unwrap();
    let channel = f.channel_parent(&public).unwrap();
    let member = f.user_parent(&user).unwrap();
    let before = f.state().unwrap();
    v["blocks"][0]["element"]["initial_conversation"] = json!("GNEWINIT1");
    v["blocks"][1]["element"]["initial_channel"] = json!("CNEWINIT1");
    v["blocks"][2]["element"]["initial_user"] = json!("UNEWINIT1");
    f.reconcile_with_all_lookup_parents(&v, &[], &[], &[], &[member], &[channel], &[conversation])
        .unwrap();
    assert_eq!(f.state().unwrap(), before);
    assert!(!f.fields[&key()].dirty && !f.fields[&public].dirty && !f.fields[&user].dirty);
}
