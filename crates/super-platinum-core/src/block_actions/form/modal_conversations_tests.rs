//! QjSI ordered arrays and input-only local submission; no live transport.
use super::*;
const C1: &str = "C12345678";
const C2: &str = "D12345678";
const C3: &str = "U12345678";
fn key() -> InputKey {
    InputKey {
        block_id: "B".into(),
        action_id: "conversations".into(),
    }
}
fn view(optional: bool, extra: Value) -> Value {
    let mut element = json!({"type":"multi_conversations_select", "action_id":"conversations"});
    element
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    json!({"type":"modal","id":"V_CONVERSATIONS","title":{"type":"plain_text","text":"Conversations"},
        "blocks":[{"type":"input","block_id":"B","optional":optional,
            "label":{"type":"plain_text","text":"Conversations"},"element":element},
            {"type":"input","block_id":"note","optional":true,"label":{"type":"plain_text","text":"Note"},
             "element":{"type":"plain_text_input","action_id":"conversations","initial_value":"Keep sibling"}}]})
}
fn saved(raw: &mut Value, value: Value) {
    raw["state"] = json!({"values":{"B":{"conversations":value}}});
}
#[test]
fn multi_conversations_saved_null_falls_back_but_empty_clears_and_wire_omits_response_flag() {
    for initial in [Value::Null, json!([]), json!([C2, C1])] {
        let mut raw = view(
            true,
            json!({"initial_conversations":initial,"response_url_enabled":true}),
        );
        let expected = if initial.is_null() {
            json!([])
        } else {
            initial.clone()
        };
        for state in [
            json!({"type":"multi_conversations_select"}),
            json!({"type":"multi_conversations_select","selected_conversations":null}),
        ] {
            saved(&mut raw, state);
            let form = ModalForm::parse_with_origin(&raw, Some("C87654321")).unwrap();
            let values = form.submission().unwrap();
            assert_eq!(
                values["values"]["B"]["conversations"],
                json!({"type":"multi_conversations_select","selected_conversations":expected})
            );
            assert_eq!(
                values["values"]["note"]["conversations"]["value"],
                "Keep sibling"
            );
        }
        saved(
            &mut raw,
            json!({"type":"multi_conversations_select","selected_conversations":[]}),
        );
        let form = ModalForm::parse(&raw).unwrap();
        assert_eq!(form.fields[&key()].value, InputValue::Conversations(vec![]));
        assert_eq!(
            form.submission().unwrap()["values"]["B"]["conversations"]["selected_conversations"],
            json!([])
        );
    }
}
#[test]
fn multi_conversations_requiredness_maximum_and_order_remain_correctable() {
    assert!(
        ModalForm::parse(&view(true, json!({"required":true})))
            .unwrap()
            .submission()
            .is_ok()
    );
    assert!(
        ModalForm::parse(&view(false, json!({"required":false})))
            .unwrap()
            .submission()
            .is_err()
    );
    let mut form = ModalForm::parse(&view(
        false,
        json!({"initial_conversations":[C1,C2,C3],"max_selected_items":2}),
    ))
    .unwrap();
    assert_eq!(
        form.fields[&key()].value,
        InputValue::Conversations(vec![C1.into(), C2.into(), C3.into()])
    );
    assert!(form.submission().is_err());
    form.set_value(
        &key(),
        InputValue::Conversations(vec![C3.into(), C2.into(), C1.into()]),
    )
    .unwrap();
    assert!(form.submission().is_err());
    form.set_value(
        &key(),
        InputValue::Conversations(vec![C3.into(), C2.into()]),
    )
    .unwrap();
    assert_eq!(
        form.submission().unwrap()["values"]["B"]["conversations"]["selected_conversations"],
        json!([C3, C2])
    );
    form.set_value(&key(), InputValue::Conversations(vec![]))
        .unwrap();
    assert!(form.submission().is_err());
}
#[test]
fn multi_conversations_malformed_ids_arrays_settings_and_unowned_state_cannot_submit() {
    for extra in [
        json!({"initial_conversations":"C12345678"}),
        json!({"initial_conversations":[1]}),
        json!({"initial_conversations":[""]}),
        json!({"initial_conversations":[C1,C1]}),
        json!({"initial_conversations":["X12345678"]}),
        json!({"initial_conversations":["C1"]}),
        json!({"max_selected_items":0}),
        json!({"max_selected_items":null}),
        json!({"initial_conversations":vec![C1;1001]}),
        json!({"max_selected_items":1.5}),
        json!({"filter":{"unknown":true}}),
    ] {
        assert!(
            ModalForm::parse(&view(true, extra.clone())).is_err(),
            "{extra}"
        );
    }
    for value in [
        json!({"type":"conversations_select","selected_conversation":C1}),
        json!({"type":"multi_conversations_select","selected_conversations":[1]}),
        json!({"type":"multi_conversations_select","selected_conversations":"bad"}),
        json!({"type":"multi_conversations_select","selected_conversations":vec![C1;1001]}),
    ] {
        let mut raw = view(true, json!({}));
        saved(&mut raw, value);
        assert!(ModalForm::parse(&raw).is_err());
    }
    for ids in [vec!["bad"], vec![C1, C1], vec![" C12345678"]] {
        let mut raw = view(true, json!({}));
        saved(
            &mut raw,
            json!({"type":"multi_conversations_select","selected_conversations":ids}),
        );
        let mut form = ModalForm::parse(&raw).unwrap();
        assert!(form.submission().is_err());
        form.set_value(&key(), InputValue::Conversations(vec![]))
            .unwrap();
        assert!(form.submission().is_ok());
    }
    let mut raw = view(true, json!({}));
    raw["state"] = json!({"values":{"OTHER":{"conversations":{"type":"multi_conversations_select","selected_conversations":[]}}}});
    assert!(matches!(
        ModalForm::parse(&raw).unwrap().submission(),
        Err(FormError::Unsupported(_))
    ));
}
#[test]
fn multi_conversations_dirty_pristine_and_kind_ownership_reconcile_transactionally() {
    let original = view(
        true,
        json!({"initial_conversations":[C1],"max_selected_items":3}),
    );
    let mut form = ModalForm::parse(&original).unwrap();
    let changed = view(
        true,
        json!({"initial_conversations":[C2,C3],"max_selected_items":1}),
    );
    form.reconcile(&changed).unwrap();
    assert_eq!(
        form.fields[&key()].value,
        InputValue::Conversations(vec![C2.into(), C3.into()])
    );
    assert!(form.submission().is_err());
    form.set_value(&key(), InputValue::Conversations(vec![]))
        .unwrap();
    form.reconcile(&original).unwrap();
    assert_eq!(form.fields[&key()].value, InputValue::Conversations(vec![]));
    form.reconcile_submitted(&original).unwrap();
    assert_eq!(form.fields[&key()].value, InputValue::Conversations(vec![]));
    let before = form.clone();
    assert_eq!(
        form.set_value(&key(), InputValue::Conversation(Some(C1.into()))),
        Err(FormError::ValueTypeMismatch)
    );
    let mut wrong = original.clone();
    wrong["blocks"][0]["element"] =
        json!({"type":"conversations_select","action_id":"conversations"});
    assert_eq!(form.reconcile(&wrong), Err(FormError::ConflictingUpdate));
    assert_eq!(form, before);
}
#[test]
fn multi_conversations_only_normal_submit_only_modal_inputs_are_supported() {
    for (name, value) in [
        ("confirm", json!({})),
        ("dispatch_action_config", json!({})),
        ("visible_to_user_ids", json!([])),
    ] {
        let mut raw = view(true, json!({}));
        raw["blocks"][0]["element"][name] = value;
        assert!(matches!(
            ModalForm::parse(&raw).unwrap().submission(),
            Err(FormError::Unsupported(_))
        ));
    }
    for raw in [
        {
            let mut v = view(true, json!({}));
            v["blocks"][0]["dispatch_action"] = json!(true);
            v
        },
        {
            let mut v = view(true, json!({}));
            v["type"] = json!("app_notification_subscription_configuration");
            v
        },
        {
            let mut v = view(true, json!({}));
            v["blocks"][0] = json!({"type":"actions","block_id":"B","elements":[{"type":"multi_conversations_select","action_id":"conversations"}]});
            v
        },
    ] {
        assert!(matches!(
            ModalForm::parse(&raw).unwrap().submission(),
            Err(FormError::Unsupported(_))
        ));
    }
}

#[test]
fn multi_conversation_filters_parse_and_origin_default_is_explicitly_unsupported() {
    for filter in [
        Value::Null,
        json!({}),
        json!({"include":[]}),
        json!({"include":["public","private","mpim","im"],"exclude_bot_users":true,"exclude_external_shared_channels":true}),
        json!({"only_from_team_id":"T12345678"}),
    ] {
        let raw = view(
            true,
            json!({"filter":filter,"initial_conversations":[C2,C3],"default_to_current_conversation":false}),
        );
        let field = parse_message_conversations(&raw["blocks"][0]["element"]).unwrap();
        assert_eq!(
            field.value,
            InputValue::Conversations(vec![C2.into(), C3.into()])
        );
        assert_eq!(field.raw["filter"], filter);
        assert!(
            ModalForm::parse_with_origin(&raw, Some(C1))
                .unwrap()
                .submission()
                .is_ok()
        );
    }
    for filter in [
        json!(false),
        json!({"include":["private","private"]}),
        json!({"include":["unknown"]}),
        json!({"exclude_bot_users":"true"}),
        json!({"only_from_team_id":" T1"}),
    ] {
        assert!(
            parse_message_conversations(
                &json!({"type":"multi_conversations_select","filter":filter})
            )
            .is_err()
        );
    }
    for initial in [Value::Null, json!([]), json!([C1])] {
        let raw = view(
            true,
            json!({"default_to_current_conversation":true,"initial_conversations":initial}),
        );
        assert!(parse_message_conversations(&raw["blocks"][0]["element"]).is_err());
        for origin in [None, Some(C2)] {
            assert!(matches!(
                ModalForm::parse_with_origin(&raw, origin)
                    .unwrap()
                    .submission(),
                Err(FormError::Unsupported(_))
            ));
        }
    }
    for flag in [Value::Null, json!("true"), json!(1)] {
        assert!(
            ModalForm::parse(&view(true, json!({"default_to_current_conversation":flag}))).is_err()
        );
    }
}
