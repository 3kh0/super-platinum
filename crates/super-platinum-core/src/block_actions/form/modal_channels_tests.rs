//! z+fW ordered arrays and input-only local submission; no live transport.
use super::*;
const C1: &str = "C12345678";
const C2: &str = "G12345678";
const C3: &str = "Z12345678";
fn key() -> InputKey {
    InputKey {
        block_id: "B".into(),
        action_id: "channels".into(),
    }
}
fn view(optional: bool, extra: Value) -> Value {
    let mut element = json!({"type":"multi_channels_select", "action_id":"channels"});
    element
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    json!({"type":"modal","id":"V_CHANNELS","title":{"type":"plain_text","text":"Channels"},
        "blocks":[{"type":"input","block_id":"B","optional":optional,
            "label":{"type":"plain_text","text":"Channels"},"element":element},
            {"type":"input","block_id":"note","optional":true,"label":{"type":"plain_text","text":"Note"},
             "element":{"type":"plain_text_input","action_id":"channels","initial_value":"Keep sibling"}}]})
}
fn saved(raw: &mut Value, value: Value) {
    raw["state"] = json!({"values":{"B":{"channels":value}}});
}
#[test]
fn multi_channels_saved_null_falls_back_but_empty_clears_and_wire_omits_response_flag() {
    for initial in [Value::Null, json!([]), json!([C2, C1])] {
        let mut raw = view(
            true,
            json!({"initial_channels":initial,"response_url_enabled":true}),
        );
        let expected = if initial.is_null() {
            json!([])
        } else {
            initial.clone()
        };
        for state in [
            json!({"type":"multi_channels_select"}),
            json!({"type":"multi_channels_select","selected_channels":null}),
        ] {
            saved(&mut raw, state);
            let form = ModalForm::parse_with_origin(&raw, Some("C87654321")).unwrap();
            let values = form.submission().unwrap();
            assert_eq!(
                values["values"]["B"]["channels"],
                json!({"type":"multi_channels_select","selected_channels":expected})
            );
            assert_eq!(
                values["values"]["note"]["channels"]["value"],
                "Keep sibling"
            );
        }
        saved(
            &mut raw,
            json!({"type":"multi_channels_select","selected_channels":[]}),
        );
        let form = ModalForm::parse(&raw).unwrap();
        assert_eq!(form.fields[&key()].value, InputValue::Channels(vec![]));
        assert_eq!(
            form.submission().unwrap()["values"]["B"]["channels"]["selected_channels"],
            json!([])
        );
    }
}
#[test]
fn multi_channels_requiredness_maximum_and_order_remain_correctable() {
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
        json!({"initial_channels":[C1,C2,C3],"max_selected_items":2}),
    ))
    .unwrap();
    assert_eq!(
        form.fields[&key()].value,
        InputValue::Channels(vec![C1.into(), C2.into(), C3.into()])
    );
    assert!(form.submission().is_err());
    form.set_value(
        &key(),
        InputValue::Channels(vec![C3.into(), C2.into(), C1.into()]),
    )
    .unwrap();
    assert!(form.submission().is_err());
    form.set_value(&key(), InputValue::Channels(vec![C3.into(), C2.into()]))
        .unwrap();
    assert_eq!(
        form.submission().unwrap()["values"]["B"]["channels"]["selected_channels"],
        json!([C3, C2])
    );
    form.set_value(&key(), InputValue::Channels(vec![]))
        .unwrap();
    assert!(form.submission().is_err());
}
#[test]
fn multi_channels_malformed_ids_arrays_settings_and_unowned_state_cannot_submit() {
    for extra in [
        json!({"initial_channels":"C12345678"}),
        json!({"initial_channels":[1]}),
        json!({"initial_channels":[""]}),
        json!({"initial_channels":[C1,C1]}),
        json!({"initial_channels":["D12345678"]}),
        json!({"initial_channels":["C1"]}),
        json!({"max_selected_items":0}),
        json!({"max_selected_items":null}),
        json!({"initial_channels":vec![C1;1001]}),
        json!({"max_selected_items":1.5}),
        json!({"filter":{}}),
        json!({"default_to_current_conversation":true}),
    ] {
        assert!(
            ModalForm::parse(&view(true, extra.clone())).is_err(),
            "{extra}"
        );
    }
    for value in [
        json!({"type":"channels_select","selected_channel":C1}),
        json!({"type":"multi_channels_select","selected_channels":[1]}),
        json!({"type":"multi_channels_select","selected_channels":"bad"}),
        json!({"type":"multi_channels_select","selected_channels":vec![C1;1001]}),
    ] {
        let mut raw = view(true, json!({}));
        saved(&mut raw, value);
        assert!(ModalForm::parse(&raw).is_err());
    }
    for ids in [vec!["bad"], vec![C1, C1], vec![" C12345678"]] {
        let mut raw = view(true, json!({}));
        saved(
            &mut raw,
            json!({"type":"multi_channels_select","selected_channels":ids}),
        );
        let mut form = ModalForm::parse(&raw).unwrap();
        assert!(form.submission().is_err());
        form.set_value(&key(), InputValue::Channels(vec![]))
            .unwrap();
        assert!(form.submission().is_ok());
    }
    let mut raw = view(true, json!({}));
    raw["state"] = json!({"values":{"OTHER":{"channels":{"type":"multi_channels_select","selected_channels":[]}}}});
    assert!(matches!(
        ModalForm::parse(&raw).unwrap().submission(),
        Err(FormError::Unsupported(_))
    ));
}
#[test]
fn multi_channels_dirty_pristine_and_kind_ownership_reconcile_transactionally() {
    let original = view(
        true,
        json!({"initial_channels":[C1],"max_selected_items":3}),
    );
    let mut form = ModalForm::parse(&original).unwrap();
    let changed = view(
        true,
        json!({"initial_channels":[C2,C3],"max_selected_items":1}),
    );
    form.reconcile(&changed).unwrap();
    assert_eq!(
        form.fields[&key()].value,
        InputValue::Channels(vec![C2.into(), C3.into()])
    );
    assert!(form.submission().is_err());
    form.set_value(&key(), InputValue::Channels(vec![]))
        .unwrap();
    form.reconcile(&original).unwrap();
    assert_eq!(form.fields[&key()].value, InputValue::Channels(vec![]));
    form.reconcile_submitted(&original).unwrap();
    assert_eq!(form.fields[&key()].value, InputValue::Channels(vec![]));
    let before = form.clone();
    assert_eq!(
        form.set_value(&key(), InputValue::Channel(Some(C1.into()))),
        Err(FormError::ValueTypeMismatch)
    );
    let mut wrong = original.clone();
    wrong["blocks"][0]["element"] = json!({"type":"channels_select","action_id":"channels"});
    assert_eq!(form.reconcile(&wrong), Err(FormError::ConflictingUpdate));
    assert_eq!(form, before);
}
#[test]
fn multi_channels_only_normal_submit_only_modal_inputs_are_supported() {
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
            v["blocks"][0] = json!({"type":"actions","block_id":"B","elements":[{"type":"multi_channels_select","action_id":"channels"}]});
            v
        },
    ] {
        assert!(matches!(
            ModalForm::parse(&raw).unwrap().submission(),
            Err(FormError::Unsupported(_))
        ));
    }
}
