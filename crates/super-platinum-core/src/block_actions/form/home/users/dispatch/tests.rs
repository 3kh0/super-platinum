use super::*;

fn key() -> InputKey {
    InputKey {
        block_id: "member".into(),
        action_id: "pick".into(),
    }
}
fn raw(optional: bool, quiet: bool) -> Value {
    json!({"type":"home","id":"V1","app_id":"A1","app_installed_team_id":"T1","bot_id":"B1","blocks":[
        {"type":"input","block_id":"member","optional":optional,"dispatch_action":!quiet,
        "element":{"type":"users_select","action_id":"pick","initial_user":"UINITIAL1",
        "placeholder":{"type":"plain_text","text":"Not action metadata"}}}]})
}
fn row(f: &HomeForm) -> Value {
    f.state().unwrap()["values"]["member"]["pick"].clone()
}
fn retain(f: &mut HomeForm, view: &Value, parents: &[UserParent]) -> Result<(), FormError> {
    f.reconcile_with_initialized_parents(view, &[], &[], &[], parents)
}

#[test]
fn user_initial_omission_and_explicit_null_are_distinct_for_quiet_and_dispatch() {
    for quiet in [false, true] {
        for optional in [false, true] {
            for initial in [None, Some(Value::Null), Some(json!(""))] {
                let mut v = raw(optional, quiet);
                let e = v["blocks"][0]["element"].as_object_mut().unwrap();
                e.remove("initial_user");
                if let Some(initial) = initial {
                    e.insert("initial_user".into(), initial);
                }
                let mut f = HomeForm::parse(&v).unwrap();
                assert_eq!(row(&f), json!({"type":"users_select"}));
                assert_eq!(f.user_seed(&key()).unwrap(), None);
                assert_eq!(
                    f.user_input(&key(), None).unwrap().is_some(),
                    optional && !quiet
                );
                assert_eq!(row(&f), json!({"type":"users_select","selected_user":null}));
                assert_eq!(f.fields[&key()].error.is_some(), !optional);
            }
        }
    }
}

#[test]
fn user_callback_preserves_exact_current_metadata_and_repeated_occurrences() {
    let mut v = raw(true, false);
    let mut f = HomeForm::parse(&v).unwrap();
    let a = f
        .user_input(&key(), Some("WSELECT01".into()))
        .unwrap()
        .unwrap();
    assert_eq!(
        a.action(),
        &json!({"type":"users_select","block_id":"member","action_id":"pick","selected_user":"WSELECT01","initial_user":"UINITIAL1"})
    );
    assert_eq!(
        f.user_input(&key(), Some("WSELECT01".into()))
            .unwrap()
            .unwrap(),
        a
    );
    v["blocks"][0]["element"]["initial_user"] = json!("UNEWINIT1");
    f.reconcile(&v).unwrap();
    let b = f
        .user_input(&key(), Some("USLACKBOT".into()))
        .unwrap()
        .unwrap();
    assert_eq!(a.action()["initial_user"], "UINITIAL1");
    assert_eq!(b.action()["initial_user"], "UNEWINIT1");
    assert!(a.same_view(&raw(true, false)));
    assert!(!a.same_view(&v));
    assert_eq!(
        row(&f),
        json!({"type":"users_select","selected_user":"USLACKBOT"})
    );
}

#[test]
fn user_invalid_native_ids_and_foreign_keys_refuse_atomically() {
    let mut f = HomeForm::parse(&raw(true, false)).unwrap();
    for id in ["", "U1", "Ubad", "CCHANNEL1", "U SPACE00"] {
        let before = f.clone();
        assert!(f.user_input(&key(), Some(id.into())).is_err());
        assert_eq!(f, before);
    }
    let before = f.clone();
    assert!(
        f.user_input(
            &InputKey {
                block_id: "other".into(),
                action_id: "pick".into()
            },
            None
        )
        .is_err()
    );
    assert_eq!(f, before);
}

#[test]
fn initialized_user_parent_retains_pristine_and_dirty_values_through_props() {
    for dirty in [false, true] {
        let mut v = raw(false, false);
        let mut f = HomeForm::parse(&v).unwrap();
        let token = f.user_parent(&key()).unwrap();
        if dirty {
            f.user_input(&key(), None).unwrap();
        }
        let before = row(&f);
        v["blocks"][0]["element"]["initial_user"] = json!("UNEWINIT1");
        v["state"] = json!({"values":{"member":{"pick":{"type":"users_select","selected_user":"USAVED001"}}}});
        retain(&mut f, &v, &[token.clone()]).unwrap();
        assert_eq!(row(&f), before);
        assert_eq!(f.fields[&key()].dirty, dirty);
        v["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_user");
        retain(&mut f, &v, &[token]).unwrap();
        assert_eq!(row(&f), before);
    }
}

#[test]
fn omitted_user_parent_fresh_seed_falls_back_but_explicit_null_does_not() {
    let mut v = raw(true, false);
    v["blocks"][0]["element"]
        .as_object_mut()
        .unwrap()
        .remove("initial_user");
    let mut f = HomeForm::parse(&v).unwrap();
    let token = f.user_parent(&key()).unwrap();
    v["blocks"][0]["element"]["initial_user"] = json!("UNEWINIT1");
    retain(&mut f, &v, &[token.clone()]).unwrap();
    assert_eq!(row(&f), json!({"type":"users_select"}));
    assert_eq!(f.user_seed(&key()).unwrap().as_deref(), Some("UNEWINIT1"));
    f.user_input(&key(), None).unwrap();
    retain(&mut f, &v, &[token]).unwrap();
    assert_eq!(f.user_seed(&key()).unwrap(), None);
}

#[test]
fn existing_saved_ingestion_and_unattested_pristine_reconcile_are_unchanged() {
    for saved in [
        json!({"type":"users_select","selected_user":null}),
        json!({"type":"users_select","selected_user":"USAVED001"}),
        json!({"type":"users_select"}),
    ] {
        let mut v = raw(true, false);
        v["state"] = json!({"values":{"member":{"pick":saved}}});
        let f = HomeForm::parse(&v).unwrap();
        assert_eq!(row(&f)["selected_user"], saved["selected_user"]);
        assert!(row(&f).get("selected_user").is_some());
    }
    let mut f = HomeForm::parse(&raw(true, false)).unwrap();
    let mut changed = raw(true, false);
    changed["blocks"][0]["element"]["initial_user"] = json!("UNEWINIT1");
    f.reconcile(&changed).unwrap();
    assert_eq!(row(&f)["selected_user"], "UNEWINIT1");
}

#[test]
fn user_parent_tokens_refuse_foreign_owner_removed_field_and_duplicates() {
    let f = HomeForm::parse(&raw(true, false)).unwrap();
    let token = f.user_parent(&key()).unwrap();
    for property in ["id", "app_id", "app_installed_team_id", "bot_id"] {
        let mut v = raw(true, false);
        v[property] = json!("DIFFERENT");
        let mut candidate = f.clone();
        assert!(retain(&mut candidate, &v, &[token.clone()]).is_err());
        assert_eq!(candidate, f);
    }
    let mut v = raw(true, false);
    v["blocks"] = json!([]);
    assert!(retain(&mut f.clone(), &v, &[token.clone()]).is_err());
    assert!(retain(&mut f.clone(), &raw(true, false), &[token.clone(), token]).is_err());
}

#[test]
fn user_public_state_corruption_cannot_issue_seed_token_event_or_serialized_state() {
    for mode in 0..6 {
        let mut f = HomeForm::parse(&raw(false, false)).unwrap();
        match mode {
            0 => {
                f.fields.get_mut(&key()).unwrap().value = InputValue::User(Some("UOTHER001".into()))
            }
            1 => f.fields.get_mut(&key()).unwrap().raw["initial_user"] = json!("UOTHER001"),
            2 => f.fields.get_mut(&key()).unwrap().optional = true,
            3 => f.fields.get_mut(&key()).unwrap().kind = InputKind::Channel,
            4 => {
                f.fields.remove(&key());
            }
            _ => f.fields.get_mut(&key()).unwrap().error = Some("fabricated".into()),
        }
        assert!(f.user_seed(&key()).is_err());
        assert!(f.user_parent(&key()).is_err());
        assert!(f.user_input(&key(), Some("USELECT01".into())).is_err());
        assert!(f.state().is_err());
    }
}

#[test]
fn single_callback_stays_single_and_rejects_noninput_or_unsupported_flags() {
    for mode in 0..5 {
        let mut v = raw(true, false);
        match mode {
            0 => {
                v["blocks"][0]["element"] = json!({"type":"multi_users_select","action_id":"pick"});
            }
            1 => {
                let element = v["blocks"][0]["element"].clone();
                v["blocks"][0] = json!({"type":"actions","block_id":"member","elements":[element]});
            }
            2 => v["blocks"][0]["element"]["confirm"] = json!({}),
            3 => v["blocks"][0]["element"]["dispatch_action_config"] = json!({}),
            _ => v["blocks"][0]["element"]["visible_to_user_ids"] = json!([]),
        }
        let mut f = HomeForm::parse(&v).unwrap();
        assert!(f.user_input(&key(), Some("USELECT01".into())).is_err());
        assert_eq!(f.state().is_err(), mode != 0);
    }
}
