//! Normal modal input blocks only: RBm/ inline selection and hGZA local state.
use super::*;

const U1: &str = "U12345678";
const U2: &str = "W12345678";
const U3: &str = "U87654321";

fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "people".into(),
    }
}
fn block(id: &str, optional: bool, extra: Value) -> Value {
    let mut element = json!({"type":"multi_users_select","action_id":"people"});
    element
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    json!({"type":"input","block_id":id,"optional":optional,
        "label":{"type":"plain_text","text":"Reviewers"},"element":element})
}
fn view(blocks: Vec<Value>) -> Value {
    json!({"type":"modal","id":"V_USERS","title":{"type":"plain_text","text":"Review"},
        "submit":{"type":"plain_text","text":"Save"},"blocks":blocks})
}
fn raw(optional: bool, extra: Value) -> Value {
    view(vec![block("review", optional, extra)])
}
fn selected(form: &ModalForm) -> &InputValue {
    &form.fields[&key("review")].value
}

#[test]
fn normal_modal_users_default_and_explicit_clear_serialize_full_empty_array() {
    for extra in [
        json!({}),
        json!({"initial_users":null}),
        json!({"initial_users":[]}),
        json!({"placeholder":null}),
    ] {
        let form = ModalForm::parse(&raw(true, extra)).unwrap();
        assert!(form.unsupported.is_empty());
        assert_eq!(selected(&form), &InputValue::Users(vec![]));
        assert_eq!(
            form.submission().unwrap(),
            json!({"values":{"review":{"people":{"type":"multi_users_select","selected_users":[]}}}})
        );
    }
    let mut form = ModalForm::parse(&raw(true, json!({"initial_users":[U1,U2]}))).unwrap();
    assert_eq!(
        form.fields[&key("review")].initial_value,
        InputValue::Users(vec![U1.into(), U2.into()])
    );
    form.set_value(&key("review"), InputValue::Users(vec![]))
        .unwrap();
    assert!(form.fields[&key("review")].dirty);
    assert_eq!(
        form.submission().unwrap()["values"]["review"]["people"]["selected_users"],
        json!([])
    );
}

#[test]
fn input_optional_is_authority_and_over_limit_selections_remain_correctable() {
    let optional = ModalForm::parse(&raw(true, json!({"required":true}))).unwrap();
    assert!(optional.submission().is_ok());
    let mut required = raw(false, json!({"required":false}));
    required["blocks"][0]
        .as_object_mut()
        .unwrap()
        .remove("optional");
    let mut form = ModalForm::parse(&required).unwrap();
    assert!(matches!(form.submission(), Err(FormError::Validation(_))));
    form.set_value(&key("review"), InputValue::Users(vec![U1.into()]))
        .unwrap();
    assert!(form.submission().is_ok());
    let mut form = ModalForm::parse(&raw(
        false,
        json!({"initial_users":[U1,U2,U3],"max_selected_items":2}),
    ))
    .unwrap();
    assert_eq!(
        selected(&form),
        &InputValue::Users(vec![U1.into(), U2.into(), U3.into()])
    );
    assert!(matches!(form.submission(), Err(FormError::Validation(_))));
    // Inline onAction commits local state even while invalid; submit validates.
    form.set_value(
        &key("review"),
        InputValue::Users(vec![U2.into(), U3.into(), U1.into()]),
    )
    .unwrap();
    assert_eq!(
        selected(&form),
        &InputValue::Users(vec![U2.into(), U3.into(), U1.into()])
    );
    assert!(form.submission().is_err());
    form.set_value(
        &key("review"),
        InputValue::Users(vec![U2.into(), U3.into()]),
    )
    .unwrap();
    assert_eq!(
        form.submission().unwrap()["values"]["review"]["people"]["selected_users"],
        json!([U2, U3])
    );
}

#[test]
fn malformed_users_initial_settings_and_saved_shapes_cannot_submit() {
    for extra in [
        json!({"initial_users":"U12345678"}),
        json!({"initial_users":[1]}),
        json!({"initial_users":[""]}),
        json!({"initial_users":["U1"]}),
        json!({"initial_users":[U1,U1]}),
        json!({"initial_users":[" U12345678"]}),
        json!({"max_selected_items":0}),
        json!({"max_selected_items":null}),
        json!({"max_selected_items":1.5}),
        json!({"placeholder":{"type":"mrkdwn","text":"users"}}),
    ] {
        assert!(
            ModalForm::parse(&raw(true, extra.clone())).is_err(),
            "{extra}"
        );
    }
    for saved in [
        json!({"type":"multi_users_select"}),
        json!({"type":"multi_users_select","selected_users":null}),
        json!({"type":"multi_users_select","selected_users":[123]}),
        json!({"type":"users_select","selected_user":U1}),
        json!({"type":"multi_users_select","selected_users":vec![U1;1001]}),
    ] {
        let mut view = raw(true, json!({"initial_users":[U1]}));
        view["state"] = json!({"values":{"review":{"people":saved}}});
        assert!(ModalForm::parse(&view).is_err());
    }
    for users in [vec!["bad"], vec![U1, U1], vec!["U12345678 "]] {
        let mut view = raw(true, json!({}));
        view["state"] = json!({"values":{"review":{"people":{"type":"multi_users_select","selected_users":users}}}});
        let mut form = ModalForm::parse(&view).unwrap();
        assert!(matches!(form.submission(), Err(FormError::Validation(_))));
        form.set_value(&key("review"), InputValue::Users(vec![]))
            .unwrap();
        assert!(form.submission().is_ok());
    }
}

#[test]
fn saved_users_override_initials_and_full_state_keeps_block_action_ownership() {
    let mut view = view(vec![
        block("review", true, json!({"initial_users":[U1]})),
        block("other", true, json!({"initial_users":[U2]})),
        json!({"type":"input","block_id":"note","label":{"type":"plain_text","text":"Note"},"element":{"type":"plain_text_input","action_id":"people","initial_value":"Keep sibling"}}),
    ]);
    view["state"] =
        json!({"values":{"review":{"people":{"type":"multi_users_select","selected_users":[]}}}});
    let form = ModalForm::parse(&view).unwrap();
    assert_eq!(selected(&form), &InputValue::Users(vec![]));
    assert_eq!(
        form.submission().unwrap(),
        json!({"values":{
            "review":{"people":{"type":"multi_users_select","selected_users":[]}},
            "other":{"people":{"type":"multi_users_select","selected_users":[U2]}},
            "note":{"people":{"type":"plain_text_input","value":"Keep sibling"}}
        }})
    );
    view["state"]["values"]["unknown"] =
        json!({"people":{"type":"multi_users_select","selected_users":[U3]}});
    assert!(matches!(
        ModalForm::parse(&view).unwrap().submission(),
        Err(FormError::Unsupported(_))
    ));
    view["blocks"][1]["block_id"] = json!("review");
    assert!(ModalForm::parse(&view).is_err());
}

#[test]
fn dirty_users_survive_updates_and_max_changes_without_silent_data_loss() {
    let original = raw(true, json!({"initial_users":[U1],"max_selected_items":3}));
    let mut form = ModalForm::parse(&original).unwrap();
    form.set_value(
        &key("review"),
        InputValue::Users(vec![U2.into(), U3.into()]),
    )
    .unwrap();
    let remote = raw(true, json!({"initial_users":[U1],"max_selected_items":1}));
    form.reconcile(&remote).unwrap();
    assert_eq!(
        selected(&form),
        &InputValue::Users(vec![U2.into(), U3.into()])
    );
    assert!(form.submission().is_err());
    form.reconcile(&original).unwrap();
    assert!(form.submission().is_ok());
    let before = form.clone();
    for changed in [
        view(vec![]),
        raw(true, json!({"type":"plain_text_input"})),
        {
            let mut changed = original.clone();
            changed["id"] = json!("OTHER_VIEW");
            changed
        },
    ] {
        assert_eq!(form.reconcile(&changed), Err(FormError::ConflictingUpdate));
        assert_eq!(form, before);
    }
    form.set_value(&key("review"), InputValue::Users(vec![]))
        .unwrap();
    form.reconcile(&original).unwrap();
    assert_eq!(selected(&form), &InputValue::Users(vec![]));
    // An accepted same-field next step retains the committed local selection.
    form.reconcile_submitted(&original).unwrap();
    assert_eq!(selected(&form), &InputValue::Users(vec![]));
}

#[test]
fn pristine_defaults_update_and_wrong_value_types_never_replace_users() {
    let mut form = ModalForm::parse(&raw(true, json!({"initial_users":[U1]}))).unwrap();
    form.reconcile(&raw(true, json!({"initial_users":[U2,U3]})))
        .unwrap();
    assert_eq!(
        selected(&form),
        &InputValue::Users(vec![U2.into(), U3.into()])
    );
    assert_eq!(form.fields[&key("review")].revision, 1);
    let before = form.clone();
    assert_eq!(
        form.set_value(&key("review"), InputValue::User(Some(U1.into()))),
        Err(FormError::ValueTypeMismatch)
    );
    assert_eq!(form, before);
    form.apply_server_errors(&BTreeMap::from([(
        "review".into(),
        "Choose a different reviewer".into(),
    )]));
    assert!(form.submission().is_err());
    form.reconcile(&raw(true, json!({"initial_users":[U2,U3]})))
        .unwrap();
    form.set_value(&key("review"), InputValue::Users(vec![U1.into()]))
        .unwrap();
    assert!(form.submission().is_ok());
}

#[test]
fn multi_users_remain_unsupported_outside_normal_submit_only_input_blocks() {
    for (key, value) in [
        ("confirm", json!({})),
        ("dispatch_action_config", json!({})),
        ("visible_to_user_ids", json!([U1])),
    ] {
        let mut view = raw(true, json!({"initial_users":[U1]}));
        view["blocks"][0]["element"][key] = value;
        assert!(matches!(
            ModalForm::parse(&view).unwrap().submission(),
            Err(FormError::Unsupported(_))
        ));
    }
    let mut dispatch = raw(true, json!({}));
    dispatch["blocks"][0]["dispatch_action"] = json!(true);
    let mut notification = raw(true, json!({}));
    notification["type"] = json!("app_notification_subscription_configuration");
    for unsupported in [
        dispatch,
        notification,
        view(vec![
            json!({"type":"section","block_id":"review","accessory":{"type":"multi_users_select","action_id":"people"}}),
        ]),
        view(vec![
            json!({"type":"actions","block_id":"review","elements":[{"type":"multi_users_select","action_id":"people"}]} ),
        ]),
    ] {
        assert!(matches!(
            ModalForm::parse(&unsupported).unwrap().submission(),
            Err(FormError::Unsupported(_))
        ));
    }
}
