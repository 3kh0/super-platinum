use super::*;
use crate::block_actions::home::{Home, View};
use crate::config::WorkspaceSession;
use crate::slack::{SlackClient, client::RequestBody};

fn key() -> InputKey {
    InputKey {
        block_id: "choice".into(),
        action_id: "pick".into(),
    }
}
fn option(label: &str) -> Value {
    json!({"text":{"type":"plain_text","text":label,"emoji":false},"value":"same-value","description":{"type":"plain_text","text":"Exact description","emoji":true},"opaque":{"label":label,"kept":[1,false,null]},"url":"https://example.test/not-navigation"})
}
fn raw() -> Value {
    json!({"type":"home","id":"V1","app_id":"A1","app_installed_team_id":"T2","bot_id":"B1","hash":"first","opaque":{"kept":1},"blocks":[
        {"type":"input","block_id":"choice","label":{"type":"plain_text","text":"Choice"},"element":{"type":"external_select","action_id":"pick","min_query_length":0,"initial_option":option("Original")}},
        {"type":"actions","block_id":"buttons","elements":[{"type":"button","action_id":"go","text":{"type":"plain_text","text":"Review"}}]}
    ]})
}
fn view(raw: Value) -> View {
    View::decode(
        &Home {
            channel: "D1".into(),
            app: "A1".into(),
            installed_team: "T2".into(),
            enabled: true,
            messages: true,
            view_id: Some("V1".into()),
        },
        raw,
    )
    .unwrap()
}

#[test]
fn exact_option_is_local_state_until_explicit_home_button() {
    let raw = raw();
    let view = view(raw.clone());
    let mut form = HomeForm::parse(&raw).unwrap();
    assert_eq!(form.dispatch(&key()), Some(Dispatch::Quiet));
    assert!(form.external_field(&key()).is_ok());
    assert_eq!(
        form.state().unwrap()["values"]["choice"]["pick"]["selected_option"],
        option("Original")
    );
    assert!(view.button("buttons", "go", "U1").is_err());
    let selected = ExternalOption::parse(&option("Remote 東京")).unwrap();
    form.set(&key(), InputValue::External(Some(selected.clone())))
        .unwrap();
    let button = view.button_with_form("buttons", "go", "U1", &form).unwrap();
    let workspace = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    };
    let request = button
        .prepare(&SlackClient::default(), &workspace, &view, "fixture-action")
        .unwrap();
    let RequestBody::Form(fields) = request.body else {
        panic!("expected form")
    };
    let fields: BTreeMap<_, _> = fields.into_iter().collect();
    let state: Value = serde_json::from_str(&fields["state"]).unwrap();
    assert_eq!(
        state["values"]["choice"]["pick"],
        json!({"type":"external_select","selected_option":selected.raw()})
    );
    let actions: Value = serde_json::from_str(&fields["actions"]).unwrap();
    assert_eq!(actions.as_array().unwrap().len(), 1);
    assert_eq!(actions[0]["type"], "button");
    assert_eq!(fields["service_team_id"], "T2");
    form.set(
        &key(),
        InputValue::External(Some(
            ExternalOption::parse(&option("Changed metadata")).unwrap(),
        )),
    )
    .unwrap();
    assert!(!button.same_form(&form));
    form.set(&key(), InputValue::External(None)).unwrap();
    // App validation owns requiredness; empty Home state is still included.
    assert!(!form.fields[&key()].optional);
    assert_eq!(
        form.state().unwrap()["values"]["choice"]["pick"]["selected_option"],
        Value::Null
    );
    assert!(!button.same_form(&form));
}

#[test]
fn dirty_option_and_wire_survive_compatible_refresh_but_not_removed_identity() {
    let mut form = HomeForm::parse(&raw()).unwrap();
    let selected = ExternalOption::parse(&option("Retained")).unwrap();
    form.set(&key(), InputValue::External(Some(selected.clone())))
        .unwrap();
    let mut next = raw();
    next["hash"] = json!("second");
    next["blocks"][0]["element"]["initial_option"] = option("Remote replacement");
    form.reconcile(&next).unwrap();
    assert_eq!(
        form.fields[&key()].value,
        InputValue::External(Some(selected.clone()))
    );
    assert_eq!(
        form.state().unwrap()["values"]["choice"]["pick"]["selected_option"],
        *selected.raw()
    );
    assert!(form.external_field(&key()).is_ok());
    let before = form.clone();
    next["blocks"].as_array_mut().unwrap().remove(0);
    assert!(form.reconcile(&next).is_err());
    assert_eq!(form, before);
    let mut changed = raw();
    changed["blocks"][0]["element"] = json!({"type":"plain_text_input","action_id":"pick"});
    assert!(form.reconcile(&changed).is_err());
    assert_eq!(form, before);
}

#[test]
fn unsupported_external_input_is_never_omitted_from_state() {
    for mode in 0..9 {
        let mut raw = raw();
        match mode {
            0 => raw["blocks"][0]["dispatch_action"] = json!("true"),
            1 => { raw["blocks"][0]["element"]["type"] = json!("future_lookup_select"); raw["blocks"][0]["element"].as_object_mut().unwrap().remove("initial_option"); },
            2 => raw["blocks"][0]["element"]["confirm"] = json!({}),
            3 => raw["blocks"][0]["element"]["dispatch_action_config"] = json!({}),
            4 => raw["blocks"][0]["element"]["visible_to_user_ids"] = json!(["U1"]),
            5 => { let element = raw["blocks"][0]["element"].clone(); raw["blocks"][0] = json!({"type":"section","block_id":"choice","accessory":element}); },
            6 => { let element = raw["blocks"][0]["element"].clone(); raw["blocks"][0] = json!({"type":"actions","block_id":"choice","elements":[element]}); },
            7 => raw["blocks"].as_array_mut().unwrap().push(json!({"type":"input","block_id":"other","element":{"type":"future_lookup_select","action_id":"user"}})),
            _ => raw["state"] = json!({"values":{"foreign":{"pick":{"type":"external_select","selected_option":option("Foreign")}}}}),
        }
        if mode == 0 {
            assert!(HomeForm::parse(&raw).is_err());
            continue;
        }
        let form = HomeForm::parse(&raw).unwrap();
        assert!(form.external_field(&key()).is_err(), "mode {mode}");
        assert!(form.state().is_err(), "mode {mode}");
    }
    for name in [
        "filter",
        "third_party_auth",
        "agent_prompt",
        "container_actions_payload",
    ] {
        let mut raw = raw();
        raw["blocks"][0]["element"][name] = json!({});
        assert!(HomeForm::parse(&raw).is_err());
    }
}

#[test]
fn shared_field_admission_refuses_foreign_keys_and_mutated_public_field() {
    let mut form = HomeForm::parse(&raw()).unwrap();
    assert!(
        form.external_field(&InputKey {
            block_id: "foreign".into(),
            action_id: "pick".into()
        })
        .is_err()
    );
    form.fields.get_mut(&key()).unwrap().raw["min_query_length"] = json!(9);
    assert!(form.external_field(&key()).is_err());
    let mut raw = raw();
    raw["blocks"][0]["element"]["initial_option"]["text"]["type"] = json!("mrkdwn");
    assert!(HomeForm::parse(&raw).is_err());
    let mut raw = self::raw();
    let duplicate = raw["blocks"][0].clone();
    raw["blocks"].as_array_mut().unwrap().push(duplicate);
    assert!(HomeForm::parse(&raw).is_err());
}

mod multi;
