//! o5++ / AM6q full state, plus the documented opening-conversation default.
use super::*;

const C1: &str = "C12345678";
const C2: &str = "G12345678";

fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "destination".into(),
    }
}
fn block(id: &str, optional: bool, extra: Value) -> Value {
    let mut element = json!({"type":"conversations_select","action_id":"destination"});
    element
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    json!({"type":"input","block_id":id,"optional":optional,
        "label":{"type":"plain_text","text":"Conversation"},"element":element})
}
fn view(blocks: Vec<Value>) -> Value {
    json!({"type":"modal","id":"V_CONVERSATION","title":{"type":"plain_text","text":"Review"},
        "submit":{"type":"plain_text","text":"Save"},"blocks":blocks})
}
fn raw(optional: bool, extra: Value) -> Value {
    view(vec![block("review", optional, extra)])
}
fn selected(form: &ModalForm) -> &InputValue {
    &form.fields[&key("review")].value
}

#[test]
fn conversation_initial_and_frozen_origin_defaults_have_explicit_precedence() {
    for initial in [None, Some(Value::Null), Some(json!("")), Some(json!(C2))] {
        for default in [
            None,
            Some(Value::Null),
            Some(json!(false)),
            Some(json!(true)),
        ] {
            for origin in [None, Some(C1)] {
                let mut extra = json!({"placeholder":null});
                if let Some(initial) = &initial {
                    extra["initial_conversation"] = initial.clone();
                }
                if let Some(default) = &default {
                    extra["default_to_current_conversation"] = default.clone();
                }
                let payload = raw(true, extra);
                let form = ModalForm::parse_with_origin(&payload, origin).unwrap();
                let expected = if initial == Some(json!(C2)) {
                    Some(C2)
                } else if default == Some(json!(true)) {
                    origin
                } else {
                    None
                };
                assert_eq!(
                    selected(&form),
                    &InputValue::Conversation(expected.map(str::to_owned))
                );
                assert!(form.submission().is_ok());
            }
        }
    }
    // Compatibility parsing has no invented current channel.
    let form =
        ModalForm::parse(&raw(true, json!({"default_to_current_conversation":true}))).unwrap();
    assert_eq!(selected(&form), &InputValue::Conversation(None));
    assert!(ModalForm::parse_with_origin(&raw(true, json!({})), Some("C1")).is_err());
}

#[test]
fn saved_null_and_local_clear_override_defaults_and_preserve_response_url_full_state() {
    let mut payload = view(vec![
        block(
            "review",
            true,
            json!({"initial_conversation":C2,"default_to_current_conversation":true,"response_url_enabled":true}),
        ),
        block(
            "other",
            true,
            json!({"initial_conversation":C1,"response_url_enabled":false}),
        ),
        json!({"type":"input","block_id":"note","label":{"type":"plain_text","text":"Note"},
            "element":{"type":"plain_text_input","action_id":"destination","initial_value":"Keep sibling"}}),
    ]);
    payload["state"] = json!({"values":{"review":{"destination":{"type":"conversations_select","selected_conversation":null}}}});
    let mut form = ModalForm::parse_with_origin(&payload, Some(C1)).unwrap();
    let expected = json!({"values":{
        "review":{"destination":{"type":"conversations_select","selected_conversation":null,"response_url_enabled":true}},
        "other":{"destination":{"type":"conversations_select","selected_conversation":C1}},
        "note":{"destination":{"type":"plain_text_input","value":"Keep sibling"}}
    }});
    assert_eq!(form.submission().unwrap(), expected);
    form.set_value(&key("review"), InputValue::Conversation(Some(C2.into())))
        .unwrap();
    let wire = form.submission().unwrap();
    assert_eq!(
        wire["values"]["review"]["destination"]["selected_conversation"],
        C2
    );
    assert_eq!(
        wire["values"]["review"]["destination"]["response_url_enabled"],
        true
    );
    form.set_value(&key("review"), InputValue::Conversation(None))
        .unwrap();
    assert_eq!(form.submission().unwrap(), expected);
    payload["state"]["values"]["unknown"] =
        json!({"destination":{"type":"conversations_select","selected_conversation":C1}});
    assert!(matches!(
        ModalForm::parse_with_origin(&payload, Some(C1))
            .unwrap()
            .submission(),
        Err(FormError::Unsupported(_))
    ));
    for flag in [None, Some(Value::Null), Some(json!(false))] {
        let mut extra = json!({});
        if let Some(flag) = flag {
            extra["response_url_enabled"] = flag;
        }
        let wire = ModalForm::parse(&raw(true, extra))
            .unwrap()
            .submission()
            .unwrap();
        assert!(
            wire["values"]["review"]["destination"]
                .get("response_url_enabled")
                .is_none()
        );
    }
}

#[test]
fn conversation_requiredness_and_invalid_identities_are_correctable() {
    let optional = ModalForm::parse(&raw(true, json!({"required":true}))).unwrap();
    assert!(optional.submission().is_ok());
    let mut form = ModalForm::parse(&raw(false, json!({"required":false}))).unwrap();
    assert!(matches!(form.submission(), Err(FormError::Validation(_))));
    for id in [C1, "D12345678", "U12345678", "W12345678", "USLACKBOT"] {
        form.set_value(&key("review"), InputValue::Conversation(Some(id.into())))
            .unwrap();
        assert!(form.submission().is_ok());
    }
    form.set_value(&key("review"), InputValue::Conversation(None))
        .unwrap();
    assert!(form.submission().is_err());
    for id in ["", "C1", " C12345678", "C12345678 ", "B12345678"] {
        let mut payload = raw(true, json!({}));
        payload["state"] = json!({"values":{"review":{"destination":{"type":"conversations_select","selected_conversation":id}}}});
        let mut form = ModalForm::parse(&payload).unwrap();
        assert!(
            matches!(form.submission(), Err(FormError::Validation(_))),
            "{id:?}"
        );
        form.set_value(&key("review"), InputValue::Conversation(None))
            .unwrap();
        assert!(form.submission().is_ok());
        form.set_value(&key("review"), InputValue::Conversation(Some(id.into())))
            .unwrap();
        assert!(form.submission().is_err());
    }
}

#[test]
fn malformed_conversation_shapes_and_filter_rules_never_submit() {
    for extra in [
        json!({"initial_conversation":123}),
        json!({"initial_conversation":[]}),
        json!({"initial_conversation":"C1"}),
        json!({"default_to_current_conversation":"true"}),
        json!({"response_url_enabled":1}),
        json!({"filter":[]}),
        json!({"filter":{"include":["public","public"]}}),
        json!({"filter":{"include":["unknown"]}}),
        json!({"filter":{"exclude_bot_users":"true"}}),
        json!({"filter":{"unknown":true}}),
        json!({"placeholder":{"type":"mrkdwn","text":"conversation"}}),
    ] {
        assert!(
            ModalForm::parse_with_origin(&raw(true, extra.clone()), Some(C1)).is_err(),
            "{extra}"
        );
    }
    for saved in [
        json!({"type":"conversations_select","selected_conversation":[]}),
        json!({"type":"conversations_select","selected_conversation":123}),
        json!({"type":"users_select","selected_user":C1}),
    ] {
        let mut payload = raw(true, json!({}));
        payload["state"] = json!({"values":{"review":{"destination":saved}}});
        assert!(ModalForm::parse_with_origin(&payload, Some(C1)).is_err());
    }
    let extra = json!({"filter":{"include":["public","private","mpim","im"],
        "exclude_bot_users":true,"exclude_external_shared_channels":true,"only_from_team_id":"T12345678"}});
    let form = ModalForm::parse(&raw(true, extra.clone())).unwrap();
    assert_eq!(form.fields[&key("review")].raw["filter"], extra["filter"]);
}

#[test]
fn conversation_updates_retain_frozen_origin_and_dirty_clear_transactionally() {
    let initial = raw(true, json!({"initial_conversation":C2}));
    let default = raw(true, json!({"default_to_current_conversation":true}));
    let mut form = ModalForm::parse_with_origin(&initial, Some(C1)).unwrap();
    form.reconcile(&default).unwrap();
    assert_eq!(selected(&form), &InputValue::Conversation(Some(C1.into())));
    assert_eq!(form.fields[&key("review")].revision, 1);
    let mut saved = default.clone();
    saved["state"] = json!({"values":{"review":{"destination":{"type":"conversations_select","selected_conversation":C2}}}});
    form.reconcile(&saved).unwrap();
    assert_eq!(selected(&form), &InputValue::Conversation(Some(C2.into())));
    saved["state"]["values"]["review"]["destination"]["selected_conversation"] = Value::Null;
    form.reconcile(&saved).unwrap();
    assert_eq!(selected(&form), &InputValue::Conversation(None));
    form.set_value(&key("review"), InputValue::Conversation(None))
        .unwrap();
    form.reconcile(&default).unwrap();
    assert_eq!(selected(&form), &InputValue::Conversation(None));
    form.reconcile_submitted(&default).unwrap();
    assert_eq!(selected(&form), &InputValue::Conversation(None));
    let before = form.clone();
    for changed in [view(vec![]), raw(true, json!({"type":"users_select"}))] {
        assert_eq!(form.reconcile(&changed), Err(FormError::ConflictingUpdate));
        assert_eq!(form, before);
    }
    let mut new_step = default;
    new_step["blocks"][0]["block_id"] = json!("next");
    form.reconcile_submitted(&new_step).unwrap();
    assert_eq!(
        form.fields[&key("next")].value,
        InputValue::Conversation(Some(C1.into()))
    );
}

#[test]
fn conversations_support_stays_bounded_to_normal_submit_only_input_blocks() {
    for (name, value) in [
        ("confirm", json!({})),
        ("dispatch_action_config", json!({})),
        ("visible_to_user_ids", json!(["U12345678"])),
    ] {
        let mut payload = raw(true, json!({}));
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
            json!({"type":"section","block_id":"review","accessory":{"type":"conversations_select","action_id":"destination"}}),
        ]),
        view(vec![
            json!({"type":"actions","block_id":"review","elements":[{"type":"conversations_select","action_id":"destination"}]}),
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
}
