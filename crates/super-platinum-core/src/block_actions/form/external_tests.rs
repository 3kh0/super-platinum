use super::*;
fn option(value: &str) -> Value {
    json!({"text":{"type":"plain_text","text":format!("Option {value}"),"emoji":true},"value":value})
}
fn key() -> InputKey {
    InputKey {
        block_id: "b".into(),
        action_id: "a".into(),
    }
}
fn view(initial: bool) -> Value {
    let mut v = json!({"id":"V1","type":"modal","title":{"type":"plain_text","text":"External"},"blocks":[{"type":"input","block_id":"b","optional":true,"label":{"type":"plain_text","text":"Choice"},"element":{"type":"external_select","action_id":"a"}},{"type":"input","block_id":"s","label":{"type":"plain_text","text":"Sibling"},"element":{"type":"plain_text_input","action_id":"text","initial_value":"retained"}}]});
    if initial {
        v["blocks"][0]["element"]["initial_option"] = option("initial");
    }
    v
}
#[test]
fn incoming_null_falls_back_but_local_clear_is_explicit_and_dirty() {
    for initial in [false, true] {
        let mut raw = view(initial);
        raw["state"] =
            json!({"values":{"b":{"a":{"type":"external_select","selected_option":null}}}});
        let mut form = ModalForm::parse(&raw).unwrap();
        assert_eq!(
            form.fields[&key()].value,
            InputValue::External(
                initial.then(|| ExternalOption::parse(&option("initial")).unwrap())
            )
        );
        form.set_value(&key(), InputValue::External(None)).unwrap();
        form.reconcile(&raw).unwrap();
        let state = form.submission().unwrap();
        assert!(state["values"]["b"]["a"]["selected_option"].is_null());
        assert_eq!(state["values"]["s"]["text"]["value"], "retained");
        // A newly mounted incoming form uses wrapper fallback again, not a persistent null override.
        assert_eq!(
            ModalForm::parse(&raw).unwrap().fields[&key()].value,
            InputValue::External(
                initial.then(|| ExternalOption::parse(&option("initial")).unwrap())
            )
        );
    }
}
#[test]
fn external_requiredness_dynamic_raw_saved_and_pristine_updates() {
    let mut raw = view(false);
    raw["blocks"][0]["optional"] = json!(false);
    let mut form = ModalForm::parse(&raw).unwrap();
    assert!(form.submission().is_err());
    let remote = ExternalOption::parse(&option("")).unwrap();
    form.set_value(&key(), InputValue::External(Some(remote.clone())))
        .unwrap();
    assert_eq!(
        form.submission().unwrap()["values"]["b"]["a"]["selected_option"],
        *remote.raw()
    );
    raw["state"] =
        json!({"values":{"b":{"a":{"type":"external_select","selected_option":option("saved")}}}});
    assert_eq!(
        ModalForm::parse(&raw).unwrap().fields[&key()].value,
        InputValue::External(Some(ExternalOption::parse(&option("saved")).unwrap()))
    );
    raw["state"]["values"]["b"]["a"]["selected_option"] = json!({"value":"shape-only"});
    assert!(ModalForm::parse(&raw).is_err());
    let mut pristine = ModalForm::parse(&view(true)).unwrap();
    pristine.reconcile(&view(false)).unwrap();
    assert_eq!(pristine.fields[&key()].value, InputValue::External(None));
}
#[test]
fn unsupported_external_contexts_and_bad_settings_stay_blocked() {
    for (setting, value) in [
        ("confirm", json!({})),
        ("visible_to_user_ids", json!([])),
        ("dispatch_action_config", json!({})),
    ] {
        let mut raw = view(false);
        raw["blocks"][0]["element"][setting] = value;
        assert!(ModalForm::parse(&raw).unwrap().submission().is_err());
    }
    let mut raw = view(false);
    raw["type"] = json!("app_notification_subscription_configuration");
    assert!(ModalForm::parse(&raw).unwrap().submission().is_err());
    for value in [json!(-1), json!(1.5), json!(null), json!("3")] {
        let mut element = json!({"type":"external_select","min_query_length":value});
        assert!(parse_message_external(&element).is_err());
        element["min_query_length"] = json!(0);
        assert!(parse_message_external(&element).is_ok());
    }
}
