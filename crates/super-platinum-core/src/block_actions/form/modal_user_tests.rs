//! HvtU / 1hVl: normal modal input users_select, with local full-state ownership.
use super::*;

const U1: &str = "U12345678";
const U2: &str = "W12345678";

fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "person".into(),
    }
}
fn block(id: &str, optional: bool, extra: Value) -> Value {
    let mut element = json!({"type":"users_select","action_id":"person"});
    element
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    json!({"type":"input","block_id":id,"optional":optional,
        "label":{"type":"plain_text","text":"Reviewer"},"element":element})
}
fn view(blocks: Vec<Value>) -> Value {
    json!({"type":"modal","id":"V_USER","title":{"type":"plain_text","text":"Review"},
        "submit":{"type":"plain_text","text":"Save"},"blocks":blocks})
}
fn raw(optional: bool, extra: Value) -> Value {
    view(vec![block("review", optional, extra)])
}
fn selected(form: &ModalForm) -> &InputValue {
    &form.fields[&key("review")].value
}

#[test]
fn modal_user_defaults_and_typed_clear_serialize_null_without_self_default() {
    for extra in [
        json!({}),
        json!({"initial_user":null}),
        json!({"initial_user":""}),
        json!({"placeholder":null}),
    ] {
        let form = ModalForm::parse(&raw(true, extra)).unwrap();
        assert!(form.unsupported.is_empty());
        assert_eq!(selected(&form), &InputValue::User(None));
        assert_eq!(
            form.submission().unwrap(),
            json!({"values":{"review":{"person":{"type":"users_select","selected_user":null}}}})
        );
    }
    for user in [U1, U2, "USLACKBOT"] {
        let mut form = ModalForm::parse(&raw(true, json!({"initial_user":user}))).unwrap();
        assert_eq!(selected(&form), &InputValue::User(Some(user.into())));
        form.set_value(&key("review"), InputValue::User(None))
            .unwrap();
        assert!(form.fields[&key("review")].dirty);
        assert_eq!(
            form.submission().unwrap()["values"]["review"]["person"]["selected_user"],
            Value::Null
        );
    }
}

#[test]
fn modal_user_requiredness_belongs_to_input_and_validation_remains_correctable() {
    let optional = ModalForm::parse(&raw(true, json!({"required":true}))).unwrap();
    assert!(optional.submission().is_ok());
    let mut required = raw(false, json!({"required":false}));
    required["blocks"][0]
        .as_object_mut()
        .unwrap()
        .remove("optional");
    let mut form = ModalForm::parse(&required).unwrap();
    assert!(matches!(form.submission(), Err(FormError::Validation(_))));
    form.set_value(&key("review"), InputValue::User(Some(U1.into())))
        .unwrap();
    assert!(form.submission().is_ok());
    form.set_value(&key("review"), InputValue::User(None))
        .unwrap();
    assert!(matches!(form.submission(), Err(FormError::Validation(_))));
    form.set_value(&key("review"), InputValue::User(Some(U2.into())))
        .unwrap();
    form.apply_server_errors(&BTreeMap::from([(
        "review".into(),
        "Choose another reviewer".into(),
    )]));
    assert!(form.submission().is_err());
    form.set_value(&key("review"), InputValue::User(Some(U1.into())))
        .unwrap();
    assert!(form.submission().is_ok());
}

#[test]
fn malformed_modal_user_shapes_and_identities_cannot_submit() {
    for extra in [
        json!({"initial_user":123}),
        json!({"initial_user":[]}),
        json!({"initial_user":"U1"}),
        json!({"initial_user":" U12345678"}),
        json!({"initial_user":"C12345678"}),
        json!({"placeholder":{"type":"mrkdwn","text":"user"}}),
        json!({"placeholder":{"type":"plain_text","text":123}}),
    ] {
        assert!(
            ModalForm::parse(&raw(true, extra.clone())).is_err(),
            "{extra}"
        );
    }
    for saved in [
        json!({"type":"users_select","selected_user":[]}),
        json!({"type":"users_select","selected_user":123}),
        json!({"type":"multi_users_select","selected_users":[U1]}),
    ] {
        let mut payload = raw(true, json!({}));
        payload["state"] = json!({"values":{"review":{"person":saved}}});
        assert!(ModalForm::parse(&payload).is_err());
    }
    for id in ["", "U1", "U12345678 ", "C12345678"] {
        let mut payload = raw(true, json!({}));
        payload["state"] =
            json!({"values":{"review":{"person":{"type":"users_select","selected_user":id}}}});
        let mut form = ModalForm::parse(&payload).unwrap();
        assert!(
            matches!(form.submission(), Err(FormError::Validation(_))),
            "{id:?}"
        );
        form.set_value(&key("review"), InputValue::User(None))
            .unwrap();
        assert!(form.submission().is_ok());
        form.set_value(&key("review"), InputValue::User(Some(id.into())))
            .unwrap();
        assert!(form.submission().is_err());
    }
}

#[test]
fn saved_null_overrides_initial_and_full_state_retains_scoped_siblings() {
    let mut payload = view(vec![
        block("review", true, json!({"initial_user":U1})),
        block("other", true, json!({"initial_user":U2})),
        json!({"type":"input","block_id":"note","label":{"type":"plain_text","text":"Note"},"element":{"type":"plain_text_input","action_id":"person","initial_value":"Keep sibling"}}),
    ]);
    // Typed modal full state owns explicit null even with a nonempty initial.
    // HvtU's local clear emits null; do not resurrect the initial on updates.
    payload["state"] =
        json!({"values":{"review":{"person":{"type":"users_select","selected_user":null}}}});
    let form = ModalForm::parse(&payload).unwrap();
    assert_eq!(selected(&form), &InputValue::User(None));
    assert_eq!(
        form.submission().unwrap(),
        json!({"values":{
            "review":{"person":{"type":"users_select","selected_user":null}},
            "other":{"person":{"type":"users_select","selected_user":U2}},
            "note":{"person":{"type":"plain_text_input","value":"Keep sibling"}}
        }})
    );
    payload["state"]["values"]["unknown"] =
        json!({"person":{"type":"users_select","selected_user":U1}});
    assert!(matches!(
        ModalForm::parse(&payload).unwrap().submission(),
        Err(FormError::Unsupported(_))
    ));
    payload["blocks"][1]["block_id"] = json!("review");
    assert!(ModalForm::parse(&payload).is_err());
}

#[test]
fn modal_user_pristine_updates_dirty_clear_and_transactional_conflicts() {
    let original = raw(true, json!({"initial_user":U1}));
    let mut form = ModalForm::parse(&original).unwrap();
    form.reconcile(&raw(true, json!({"initial_user":U2})))
        .unwrap();
    assert_eq!(selected(&form), &InputValue::User(Some(U2.into())));
    assert_eq!(form.fields[&key("review")].revision, 1);
    let before = form.clone();
    assert_eq!(
        form.set_value(&key("review"), InputValue::Users(vec![U1.into()])),
        Err(FormError::ValueTypeMismatch)
    );
    assert_eq!(form, before);
    form.set_value(&key("review"), InputValue::User(None))
        .unwrap();
    form.reconcile(&original).unwrap();
    assert_eq!(selected(&form), &InputValue::User(None));
    form.reconcile(&raw(false, json!({"initial_user":U1})))
        .unwrap();
    assert_eq!(selected(&form), &InputValue::User(None));
    assert!(form.submission().is_err());
    let before = form.clone();
    for changed in [
        view(vec![]),
        raw(true, json!({"type":"multi_users_select"})),
        {
            let mut changed = original.clone();
            changed["id"] = json!("OTHER_VIEW");
            changed
        },
    ] {
        assert_eq!(form.reconcile(&changed), Err(FormError::ConflictingUpdate));
        assert_eq!(form, before);
    }
    form.set_value(&key("review"), InputValue::User(Some(U2.into())))
        .unwrap();
    form.reconcile_submitted(&original).unwrap();
    assert_eq!(selected(&form), &InputValue::User(Some(U2.into())));
    assert!(form.submission().is_ok());
}

#[test]
fn user_select_support_stays_bounded_to_normal_submit_only_inputs() {
    for (name, value) in [
        ("confirm", json!({})),
        ("dispatch_action_config", json!({})),
        ("visible_to_user_ids", json!([U1])),
    ] {
        let mut payload = raw(true, json!({"initial_user":U1}));
        payload["blocks"][0]["element"][name] = value;
        assert!(matches!(
            ModalForm::parse(&payload).unwrap().submission(),
            Err(FormError::Unsupported(_))
        ));
    }
    let mut dispatch = raw(true, json!({}));
    dispatch["blocks"][0]["dispatch_action"] = json!(true);
    let mut special = raw(true, json!({}));
    special["type"] = json!("app_notification_subscription_configuration");
    for payload in [
        dispatch,
        special,
        view(vec![
            json!({"type":"section","block_id":"review","accessory":{"type":"users_select","action_id":"person"}}),
        ]),
        view(vec![
            json!({"type":"actions","block_id":"review","elements":[{"type":"users_select","action_id":"person"}]}),
        ]),
        raw(
            true,
            json!({"type":"multi_conversations_select","default_to_current_conversation":true}),
        ),
    ] {
        assert!(matches!(
            ModalForm::parse(&payload).unwrap().submission(),
            Err(FormError::Unsupported(_))
        ));
    }
    // Null placeholders for earlier controls retain their existing validation.
    assert!(
        ModalForm::parse(&raw(
            true,
            json!({"type":"plain_text_input","placeholder":null})
        ))
        .is_err()
    );
}
