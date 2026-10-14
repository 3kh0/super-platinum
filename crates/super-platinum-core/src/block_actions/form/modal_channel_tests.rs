//! cdYm / rlGy channel input shape; cxJV / AM6q selected_channel full state.
use super::*;

const C1: &str = "C12345678";
const C2: &str = "C87654321";

fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "destination".into(),
    }
}
fn block(id: &str, optional: bool, extra: Value) -> Value {
    let mut element = json!({"type":"channels_select","action_id":"destination"});
    element
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    json!({"type":"input","block_id":id,"optional":optional,
        "label":{"type":"plain_text","text":"Channel"},"element":element})
}
fn view(blocks: Vec<Value>) -> Value {
    json!({"type":"modal","id":"V_CHANNEL","title":{"type":"plain_text","text":"Share"},
        "submit":{"type":"plain_text","text":"Save"},"blocks":blocks})
}
fn raw(optional: bool, extra: Value) -> Value {
    view(vec![block("target", optional, extra)])
}
fn selected(form: &ModalForm) -> &InputValue {
    &form.fields[&key("target")].value
}

#[test]
fn channel_initial_and_empty_values_never_default_to_modal_origin() {
    for extra in [
        json!({}),
        json!({"initial_channel":null}),
        json!({"initial_channel":""}),
        json!({"placeholder":null}),
    ] {
        let mut form = ModalForm::parse_with_origin(&raw(true, extra), Some(C1)).unwrap();
        assert_eq!(form.fields[&key("target")].kind, InputKind::Channel);
        assert_eq!(selected(&form), &InputValue::Channel(None));
        assert_eq!(
            form.submission().unwrap(),
            json!({"values":{"target":{"destination":{"type":"channels_select","selected_channel":null}}}})
        );
        form.reconcile(&raw(true, json!({}))).unwrap();
        assert_eq!(selected(&form), &InputValue::Channel(None));
    }
    // Core validates identity syntax; directory metadata owns public eligibility.
    for id in [C1, "G12345678", "Z12345678"] {
        let form =
            ModalForm::parse_with_origin(&raw(true, json!({"initial_channel":id})), Some(C2))
                .unwrap();
        assert_eq!(selected(&form), &InputValue::Channel(Some(id.into())));
        assert_eq!(
            form.submission().unwrap()["values"]["target"]["destination"]["selected_channel"],
            id
        );
    }
}

#[test]
fn channel_saved_null_and_local_clear_preserve_flag_and_full_sibling_state() {
    let mut payload = view(vec![
        block(
            "target",
            true,
            json!({"initial_channel":C1,"response_url_enabled":true}),
        ),
        block(
            "other",
            true,
            json!({"initial_channel":C2,"response_url_enabled":false}),
        ),
        block(
            "conversation",
            true,
            json!({"type":"conversations_select","initial_conversation":C1,"response_url_enabled":true}),
        ),
        json!({"type":"input","block_id":"note","label":{"type":"plain_text","text":"Note"},"element":{"type":"plain_text_input","action_id":"destination","initial_value":"Keep sibling"}}),
    ]);
    payload["state"] = json!({"values":{"target":{"destination":{"type":"channels_select","selected_channel":null}}}});
    let mut form = ModalForm::parse_with_origin(&payload, Some(C2)).unwrap();
    let expected = json!({"values":{
        "target":{"destination":{"type":"channels_select","selected_channel":null,"response_url_enabled":true}},
        "other":{"destination":{"type":"channels_select","selected_channel":C2}},
        "conversation":{"destination":{"type":"conversations_select","selected_conversation":C1,"response_url_enabled":true}},
        "note":{"destination":{"type":"plain_text_input","value":"Keep sibling"}}
    }});
    assert_eq!(form.submission().unwrap(), expected);
    form.set_value(&key("target"), InputValue::Channel(Some(C2.into())))
        .unwrap();
    let wire = form.submission().unwrap();
    assert_eq!(
        wire["values"]["target"]["destination"],
        json!({"type":"channels_select","selected_channel":C2,"response_url_enabled":true})
    );
    form.set_value(&key("target"), InputValue::Channel(None))
        .unwrap();
    assert_eq!(form.submission().unwrap(), expected);
    for flag in [None, Some(Value::Null), Some(json!(false))] {
        let mut extra = json!({});
        if let Some(flag) = flag {
            extra["response_url_enabled"] = flag;
        }
        let form = ModalForm::parse(&raw(true, extra)).unwrap();
        assert!(
            form.submission().unwrap()["values"]["target"]["destination"]
                .get("response_url_enabled")
                .is_none()
        );
    }
}

#[test]
fn channel_requiredness_and_identity_validation_remain_correctable() {
    assert!(
        ModalForm::parse(&raw(true, json!({"required":true})))
            .unwrap()
            .submission()
            .is_ok()
    );
    let mut form = ModalForm::parse(&raw(false, json!({"required":false}))).unwrap();
    assert!(matches!(form.submission(), Err(FormError::Validation(_))));
    form.set_value(&key("target"), InputValue::Channel(Some(C1.into())))
        .unwrap();
    assert!(form.submission().is_ok());
    form.set_value(&key("target"), InputValue::Channel(None))
        .unwrap();
    assert!(form.submission().is_err());
    for id in [
        "",
        "C1",
        " C12345678",
        "C12345678 ",
        "D12345678",
        "U12345678",
        "W12345678",
        "USLACKBOT",
    ] {
        let mut payload = raw(true, json!({}));
        payload["state"] = json!({"values":{"target":{"destination":{"type":"channels_select","selected_channel":id}}}});
        let mut form = ModalForm::parse(&payload).unwrap();
        assert!(
            matches!(form.submission(), Err(FormError::Validation(_))),
            "{id:?}"
        );
        form.set_value(&key("target"), InputValue::Channel(None))
            .unwrap();
        assert!(form.submission().is_ok());
        form.set_value(&key("target"), InputValue::Channel(Some(id.into())))
            .unwrap();
        assert!(form.submission().is_err());
    }
}

#[test]
fn malformed_channel_shapes_and_unowned_saved_state_cannot_submit() {
    for extra in [
        json!({"initial_channel":123}),
        json!({"initial_channel":[]}),
        json!({"initial_channel":{}}),
        json!({"initial_channel":"C1"}),
        json!({"initial_channel":"D12345678"}),
        json!({"initial_channel":"U12345678"}),
        json!({"response_url_enabled":"true"}),
        json!({"placeholder":{"type":"mrkdwn","text":"Channel"}}),
        json!({"placeholder":{"type":"plain_text","text":123}}),
    ] {
        assert!(
            ModalForm::parse(&raw(true, extra.clone())).is_err(),
            "{extra}"
        );
    }
    for saved in [
        json!({"type":"channels_select","selected_channel":[]}),
        json!({"type":"channels_select","selected_channel":123}),
        json!({"type":"conversations_select","selected_conversation":C1}),
        json!({"type":"users_select","selected_user":"U12345678"}),
    ] {
        let mut payload = raw(true, json!({}));
        payload["state"] = json!({"values":{"target":{"destination":saved}}});
        assert!(ModalForm::parse(&payload).is_err());
    }
    let mut payload = raw(true, json!({}));
    payload["state"] = json!({"values":{"unowned":{"destination":{"type":"channels_select","selected_channel":C1}}}});
    assert!(matches!(
        ModalForm::parse(&payload).unwrap().submission(),
        Err(FormError::Unsupported(_))
    ));
}

#[test]
fn channel_pristine_and_dirty_reconciliation_keep_channel_ownership_distinct() {
    let original = raw(true, json!({"initial_channel":C1}));
    let mut form = ModalForm::parse(&original).unwrap();
    form.reconcile(&raw(true, json!({"initial_channel":C2})))
        .unwrap();
    assert_eq!(selected(&form), &InputValue::Channel(Some(C2.into())));
    assert_eq!(form.fields[&key("target")].revision, 1);
    let mut saved = original.clone();
    saved["state"] = json!({"values":{"target":{"destination":{"type":"channels_select","selected_channel":null}}}});
    form.reconcile(&saved).unwrap();
    assert_eq!(selected(&form), &InputValue::Channel(None));
    form.set_value(&key("target"), InputValue::Channel(None))
        .unwrap();
    form.reconcile(&original).unwrap();
    assert_eq!(selected(&form), &InputValue::Channel(None));
    let before = form.clone();
    assert_eq!(
        form.set_value(&key("target"), InputValue::Conversation(Some(C1.into()))),
        Err(FormError::ValueTypeMismatch)
    );
    assert_eq!(form, before);
    let conversation = raw(
        true,
        json!({"type":"conversations_select","initial_conversation":C2}),
    );
    for changed in [view(vec![]), conversation.clone()] {
        assert_eq!(form.reconcile(&changed), Err(FormError::ConflictingUpdate));
        assert_eq!(form, before);
    }
    form.reconcile_submitted(&original).unwrap();
    assert_eq!(selected(&form), &InputValue::Channel(None));
    form.reconcile_submitted(&conversation).unwrap();
    assert_eq!(selected(&form), &InputValue::Conversation(Some(C2.into())));
    let mut changed_back = ModalForm::parse(&conversation).unwrap();
    changed_back
        .set_value(&key("target"), InputValue::Conversation(None))
        .unwrap();
    assert_eq!(
        changed_back.reconcile(&original),
        Err(FormError::ConflictingUpdate)
    );
}

#[test]
fn channel_support_excludes_unproven_contexts_and_conversation_or_enterprise_flags() {
    for (name, value) in [
        ("filter", json!({"include":["public"]})),
        ("default_to_current_conversation", json!(true)),
        ("requested_scope", json!("channels:read")),
        ("show_enterprise_options", json!(true)),
        ("service_team_id", json!("T12345678")),
        (
            "container_actions_payload",
            json!({"serviceTeamId":"T12345678"}),
        ),
        ("defaultToCurrentConversation", json!(true)),
        ("requestedScope", json!("channels:read")),
        ("showEnterpriseOptions", json!(true)),
        ("serviceTeamId", json!("T12345678")),
        ("containerActionsPayload", json!({})),
        ("confirm", json!({})),
        ("dispatch_action_config", json!({})),
        ("visible_to_user_ids", json!(["U12345678"])),
    ] {
        let mut payload = raw(true, json!({"initial_channel":C1}));
        payload["blocks"][0]["element"][name] = value;
        assert!(
            matches!(
                ModalForm::parse_with_origin(&payload, Some(C2))
                    .unwrap()
                    .submission(),
                Err(FormError::Unsupported(_))
            ),
            "{name}"
        );
    }
    let mut dispatch = raw(true, json!({}));
    dispatch["blocks"][0]["dispatch_action"] = json!(true);
    let mut special = raw(true, json!({}));
    special["type"] = json!("app_notification_subscription_configuration");
    for payload in [
        dispatch,
        special,
        view(vec![
            json!({"type":"section","accessory":{"type":"channels_select","action_id":"destination"}}),
        ]),
        view(vec![
            json!({"type":"actions","elements":[{"type":"channels_select","action_id":"destination"}]}),
        ]),
        raw(true, json!({"type":"multi_external_select"})),
    ] {
        assert!(matches!(
            ModalForm::parse(&payload).unwrap().submission(),
            Err(FormError::Unsupported(_))
        ));
    }
}
