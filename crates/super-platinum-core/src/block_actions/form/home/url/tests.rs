use super::*;
use crate::{
    block_actions::home::{Home, View},
    config::WorkspaceSession,
    slack::{SlackClient, client::RequestBody},
};
fn raw() -> Value {
    json!({"id":"V1","type":"home","app_id":"A1","app_installed_team_id":"T1","blocks":[
        {"type":"input","block_id":"url","dispatch_action":true,"element":{"type":"url_text_input","action_id":"value","initial_value":"https://Initial.test/literal","dispatch_action_config":{"trigger_actions_on":["on_enter_pressed","on_character_entered"]}}},
        {"type":"input","block_id":"text","dispatch_action":true,"optional":true,"element":{"type":"plain_text_input","action_id":"value","dispatch_action_config":{"trigger_actions_on":["on_enter_pressed"]}}}
    ]})
}
fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "value".into(),
    }
}

#[test]
fn home_owned_url_profile_literal_parent_lifecycle_and_buffer_guards() {
    let source = raw();
    let k = key("url");
    let legacy = HomeForm::parse(&source).unwrap();
    assert!(!legacy.has_owned_urls() && legacy.url_field(&k).is_err());
    let mut form = HomeForm::parse_first_owned_urls(&source).unwrap();
    assert!(form.has_owned_urls());
    assert_eq!(
        form.state().unwrap()["values"]["url"]["value"],
        json!({"type":"url_text_input","value":"https://Initial.test/literal"})
    );
    assert_eq!(
        form.url_mount(&k, UrlMount::NewParent, true).unwrap(),
        "https://Initial.test/literal"
    );
    assert!(form.url_mount(&k, UrlMount::NewParent, true).is_err());
    let event = form
        .url_input(&k, "Example.test/Path".into(), true, UrlTrigger::Change)
        .unwrap()
        .unwrap();
    assert_eq!(event.action()["value"], "Example.test/Path");
    assert!(
        form.url_input(&k, "bad host".into(), false, UrlTrigger::Change)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        form.state().unwrap()["values"]["url"]["value"]["value"],
        "bad host"
    );
    assert_eq!(event.action()["value"], "Example.test/Path");
    let domain_error = form.fields[&k].error.clone();
    assert!(domain_error.is_some());
    assert!(form.native_error_text(&k).is_none());
    form.url_mark_native_buffer(&k).unwrap();
    assert!(form.state().is_err());
    form.url_acknowledge_native_buffer(&k).unwrap();
    assert_eq!(form.fields[&k].error, domain_error);
    assert_eq!(
        form.state().unwrap()["values"]["url"]["value"]["value"],
        "bad host"
    );
    let parent = form.url_parent(&k).unwrap();
    let scope = form.owned_url_scope().unwrap();
    let mut changed = source.clone();
    changed["blocks"][0]["element"]["initial_value"] = json!("https://Changed.test");
    assert!(
        form.reconcile_with_url_parents(&changed, &[], &[], &[], &[], &[], &[], &[], &[], &[], &[])
            .is_err()
    );
    form.reconcile_with_url_parents(
        &changed,
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[parent],
    )
    .unwrap();
    assert!(form.same_url_scope(&scope));
    assert_eq!(form.fields[&k].error, domain_error);
    assert_eq!(
        form.state().unwrap()["values"]["url"]["value"]["value"],
        "bad host"
    );
    assert_eq!(form.url_seed(&k).unwrap(), "https://Changed.test");
    // New semantic visit may run its own silent initial effect; DOM remount may not.
    form.url_mount(&k, UrlMount::RetainedParent, true).unwrap();
    assert!(form.fields[&k].error.is_none());
    assert_eq!(
        form.state().unwrap()["values"]["url"]["value"]["value"],
        "https://Changed.test"
    );
    form.url_mark_native_buffer(&k).unwrap();
    assert!(form.state().is_err());
    form.url_acknowledge_native_buffer(&k).unwrap();
    assert_eq!(
        form.state().unwrap()["values"]["url"]["value"]["value"],
        "https://Changed.test"
    );
    let clear = form
        .url_input(&k, String::new(), true, UrlTrigger::Enter)
        .unwrap()
        .unwrap();
    assert_eq!(clear.action()["value"], "");
    assert_eq!(
        form.state().unwrap()["values"]["url"]["value"],
        json!({"type":"url_text_input","value":""})
    );
    let reset = HomeForm::parse_first_owned_urls(&changed).unwrap();
    assert!(!reset.same_url_scope(&scope) && !reset.url_event_current(&clear));
    for mutation in ["saved", "raw_state", "confirm", "unknown_trigger"] {
        let mut invalid = source.clone();
        match mutation {
            "saved" => {
                invalid["state"] =
                    json!({"values":{"url":{"value":{"type":"url_text_input","value":"unowned"}}}})
            }
            "raw_state" => invalid["raw_state"] = json!({}),
            "confirm" => invalid["blocks"][0]["element"]["confirm"] = json!({}),
            _ => {
                invalid["blocks"][0]["element"]["dispatch_action_config"] =
                    json!({"trigger_actions_on":["unknown"]})
            }
        }
        assert!(
            match HomeForm::parse_first_owned_urls(&invalid) {
                Ok(f) => !f.has_owned_urls(),
                Err(_) => true,
            },
            "{mutation}"
        );
    }
    let mut quiet = source.clone();
    quiet["blocks"][0]["dispatch_action"] = json!(false);
    let mut quiet = HomeForm::parse_first_owned_urls(&quiet).unwrap();
    assert!(
        quiet
            .url_input(&k, "literal invalid".into(), false, UrlTrigger::Change)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        quiet.state().unwrap()["values"]["url"]["value"]["value"],
        "literal invalid"
    );
    assert!(quiet.fields[&k].error.is_some() && quiet.native_error_text(&k).is_none());
    for initial in [None, Some("")] {
        let mut empty = source.clone();
        let element = empty["blocks"][0]["element"].as_object_mut().unwrap();
        if let Some(value) = initial {
            element.insert("initial_value".into(), json!(value));
        } else {
            element.remove("initial_value");
        }
        let mut empty = HomeForm::parse_first_owned_urls(&empty).unwrap();
        assert_eq!(
            empty.state().unwrap()["values"]["url"]["value"],
            json!({"type":"url_text_input"})
        );
        assert_eq!(empty.url_mount(&k, UrlMount::NewParent, true).unwrap(), "");
        assert_eq!(
            empty.state().unwrap()["values"]["url"]["value"],
            json!({"type":"url_text_input"})
        );
        empty
            .url_input(&k, String::new(), false, UrlTrigger::Change)
            .unwrap();
        assert_eq!(
            empty.state().unwrap()["values"]["url"]["value"],
            json!({"type":"url_text_input","value":""})
        );
        assert!(empty.fields[&k].error.is_none());
    }
    let mut invalid_initial = source.clone();
    invalid_initial["blocks"][0]["element"]["initial_value"] = json!("bad host");
    let mut invalid_initial = HomeForm::parse_first_owned_urls(&invalid_initial).unwrap();
    assert!(invalid_initial.fields[&k].error.is_none());
    invalid_initial
        .url_mount(&k, UrlMount::NewParent, false)
        .unwrap();
    assert!(invalid_initial.fields[&k].error.is_some());
    assert_eq!(
        invalid_initial.state().unwrap()["values"]["url"]["value"]["value"],
        "bad host"
    );
}

#[test]
fn home_owned_url_action_uses_captured_literal_and_current_siblings_but_refuses_reset() {
    let raw = raw();
    let mut form = HomeForm::parse_first_owned_urls(&raw).unwrap();
    let home = Home {
        channel: "D1".into(),
        app: "A1".into(),
        installed_team: "T1".into(),
        enabled: true,
        messages: true,
        view_id: Some("V1".into()),
    };
    let view = View::decode(&home, raw.clone()).unwrap();
    let workspace = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://fixture.test".into(),
        token: "synthetic".into(),
    };
    let event = form
        .url_input(
            &key("url"),
            "Example.test/path".into(),
            true,
            UrlTrigger::Enter,
        )
        .unwrap()
        .unwrap();
    assert!(view.text_action(&event, "U1").is_err());
    let action = view.text_action_in_owned_form(&event, "U1", &form).unwrap();
    form.url_input(&key("url"), "bad host".into(), false, UrlTrigger::Change)
        .unwrap();
    let sibling = form
        .text_input(&key("text"), "latest sibling".into(), TextTrigger::Enter)
        .unwrap()
        .unwrap();
    let sibling = view
        .text_action_in_owned_form(&sibling, "U1", &form)
        .unwrap();
    for (action, expected) in [(&action, "Example.test/path"), (&sibling, "latest sibling")] {
        let request = action
            .prepare_with_context(
                &SlackClient::default(),
                &workspace,
                &view,
                &form,
                "TOKEN",
                None,
            )
            .unwrap();
        let RequestBody::Form(fields) = request.body else {
            panic!("form")
        };
        let fields = fields.into_iter().collect::<BTreeMap<_, _>>();
        assert_eq!(
            serde_json::from_str::<Value>(&fields["actions"]).unwrap()[0]["value"],
            expected
        );
        assert_eq!(
            serde_json::from_str::<Value>(&fields["state"]).unwrap()["values"]["url"]["value"]["value"],
            "bad host"
        );
    }
    let reset = HomeForm::parse_first_owned_urls(&raw).unwrap();
    assert!(
        action
            .prepare_with_context(
                &SlackClient::default(),
                &workspace,
                &view,
                &reset,
                "TOKEN",
                None
            )
            .is_err()
    );
    assert!(
        view.text_action_in_owned_form(&event, "U1", &reset)
            .is_err()
    );
    form.url_mark_native_buffer(&key("url")).unwrap();
    assert!(
        action
            .prepare_with_context(
                &SlackClient::default(),
                &workspace,
                &view,
                &form,
                "TOKEN",
                None
            )
            .is_err()
    );
}
