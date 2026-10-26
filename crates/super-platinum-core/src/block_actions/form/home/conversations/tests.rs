use super::*;
use crate::block_actions::home::{Home, View};
use crate::config::WorkspaceSession;
use crate::slack::{SlackClient, client::RequestBody};

fn key() -> InputKey {
    InputKey {
        block_id: "channel".into(),
        action_id: "pick".into(),
    }
}
fn raw(optional: bool) -> Value {
    json!({"type":"home","id":"V1","app_id":"A1","app_installed_team_id":"T2","bot_id":"B1","hash":"one","opaque":{"retained":true},"blocks":[
        {"type":"input","block_id":"channel","optional":optional,"label":{"type":"plain_text","text":"Destination"},"element":{"type":"conversations_select","action_id":"pick","initial_conversation":"CINITIAL1","response_url_enabled":true,"placeholder":{"type":"plain_text","text":"Choose a channel"}}},
        {"type":"actions","block_id":"buttons","elements":[{"type":"button","action_id":"review","text":{"type":"plain_text","text":"Review"}}]}
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
fn action(view: &View, form: &HomeForm, context: Option<&str>) -> BTreeMap<String, String> {
    let workspace = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    };
    let request = view
        .button_with_form("buttons", "review", "U1", form)
        .unwrap()
        .prepare_with_context(
            &SlackClient::default(),
            &workspace,
            view,
            "fixture-action",
            context,
        )
        .unwrap();
    assert!(!request.retry_safe());
    let RequestBody::Form(fields) = request.body else {
        panic!("expected form")
    };
    fields.into_iter().collect()
}
fn selected(form: &HomeForm) -> Value {
    form.state().unwrap()["values"]["channel"]["pick"].clone()
}

#[test]
fn quiet_conversation_selection_enters_only_explicit_button_with_home_context() {
    let raw = raw(false);
    let view = view(raw.clone());
    let mut form = HomeForm::parse(&raw).unwrap();
    assert_eq!(form.dispatch(&key()), Some(Dispatch::Quiet));
    assert!(form.conversation_field(&key()).is_ok());
    assert!(form.user_field(&key()).is_err());
    assert!(form.external_field(&key()).is_err());
    assert!(view.button("buttons", "review", "U1").is_err());
    form.set(&key(), InputValue::Conversation(Some("CSELECT02".into())))
        .unwrap();
    assert_eq!(
        selected(&form),
        json!({"type":"conversations_select","selected_conversation":"CSELECT02","response_url_enabled":true})
    );
    for (context, team) in [(None, "T2"), (Some("TCONTEXT1"), "TCONTEXT1")] {
        let wire = action(&view, &form, context);
        let state: Value = serde_json::from_str(&wire["state"]).unwrap();
        assert_eq!(state["values"]["channel"]["pick"], selected(&form));
        let actions: Value = serde_json::from_str(&wire["actions"]).unwrap();
        assert_eq!(actions.as_array().unwrap().len(), 1);
        assert_eq!(actions[0]["type"], "button");
        assert_eq!(wire["service_team_id"], team);
    }
    assert_eq!(form.raw_view(), &raw);
}

#[test]
fn local_null_and_response_url_flag_are_state_not_submission_authority() {
    for optional in [false, true] {
        for flag in [
            None,
            Some(Value::Null),
            Some(json!(false)),
            Some(json!(true)),
        ] {
            let mut raw = raw(optional);
            raw["blocks"][0]["element"]
                .as_object_mut()
                .unwrap()
                .remove("response_url_enabled");
            if let Some(flag) = &flag {
                raw["blocks"][0]["element"]["response_url_enabled"] = flag.clone();
            }
            let view = view(raw.clone());
            let mut form = HomeForm::parse(&raw).unwrap();
            form.set(&key(), InputValue::Conversation(None)).unwrap();
            assert_eq!(form.fields[&key()].error.is_some(), !optional);
            let value = selected(&form);
            assert!(value["selected_conversation"].is_null());
            assert_eq!(
                value.get("response_url_enabled"),
                (flag == Some(json!(true))).then_some(&Value::Bool(true))
            );
            let wire = action(&view, &form, None);
            let state: Value = serde_json::from_str(&wire["state"]).unwrap();
            assert_eq!(state["values"]["channel"]["pick"], value);
        }
    }
}

#[test]
fn saved_null_wins_initial_and_parent_dirty_state_survives_prop_changes() {
    let mut raw = raw(false);
    raw["state"] = json!({"values":{"channel":{"pick":{"type":"conversations_select","selected_conversation":null}}}});
    let mut form = HomeForm::parse(&raw).unwrap();
    assert_eq!(form.fields[&key()].value, InputValue::Conversation(None));
    form.set(&key(), InputValue::Conversation(Some("CSELECT02".into())))
        .unwrap();
    raw["hash"] = json!("two");
    raw["blocks"][0]["element"]["initial_conversation"] = json!("CNEWINIT1");
    raw["state"]["values"]["channel"]["pick"]["selected_conversation"] = json!("CSAVED001");
    form.reconcile(&raw).unwrap();
    assert_eq!(selected(&form)["selected_conversation"], "CSELECT02");
    assert_eq!(
        form.conversation_field(&key()).unwrap().raw["initial_conversation"],
        "CNEWINIT1"
    );
    // o5++'s independent local display effect is renderer-owned, not an emit.
    let pristine = HomeForm::parse(&raw).unwrap();
    assert_eq!(selected(&pristine)["selected_conversation"], "CSAVED001");
}

#[test]
fn home_default_has_no_sidebar_or_view_origin_and_retains_raw_filter() {
    for default in [false, true] {
        let mut raw = raw(true);
        let element = &mut raw["blocks"][0]["element"];
        element
            .as_object_mut()
            .unwrap()
            .remove("initial_conversation");
        element["default_to_current_conversation"] = json!(default);
        element["filter"] = json!({"include":["private","im"],"only_from_team_id":"EENTER001","exclude_bot_users":true});
        let mut form = HomeForm::parse(&raw).unwrap();
        assert!(selected(&form)["selected_conversation"].is_null());
        assert_eq!(
            form.conversation_field(&key()).unwrap().raw,
            raw["blocks"][0]["element"]
        );
        // A member ID is the selected conversation; serialization does not open a DM.
        form.set(&key(), InputValue::Conversation(Some("UMEMBER01".into())))
            .unwrap();
        let wire = action(&view(raw.clone()), &form, Some("TCONTEXT1"));
        let state: Value = serde_json::from_str(&wire["state"]).unwrap();
        assert_eq!(
            state["values"]["channel"]["pick"]["selected_conversation"],
            "UMEMBER01"
        );
        assert_eq!(wire["service_team_id"], "TCONTEXT1");
        let filter = ConversationFilter::parse(&form.fields[&key()].raw).unwrap();
        assert_eq!(filter.only_from_team_id.as_deref(), Some("EENTER001"));
        assert!(filter.allows("private") && filter.allows("im"));
        assert!(!filter.allows("public"));
    }
}

#[test]
fn single_dispatch_and_quiet_input_field_identity_guards_remain_exact() {
    for setting in ["confirm", "dispatch_action_config", "visible_to_user_ids"] {
        let mut raw = raw(true);
        raw["blocks"][0]["element"][setting] = json!({});
        let form = HomeForm::parse(&raw).unwrap();
        assert!(form.conversation_field(&key()).is_err());
        assert!(form.state().is_err());
    }
    let mut raw = raw(true);
    raw["blocks"][0]["dispatch_action"] = json!(true);
    assert!(
        HomeForm::parse(&raw)
            .unwrap()
            .conversation_field(&key())
            .is_ok()
    );
    raw["blocks"][0]["dispatch_action"] = json!(false);
    raw["blocks"][0]["element"] = json!({"type":"future_lookup_select","action_id":"pick"});
    assert!(HomeForm::parse(&raw).unwrap().state().is_err());
    raw["blocks"][0]["element"] = json!({"type":"conversations_select","action_id":"pick"});
    let mut form = HomeForm::parse(&raw).unwrap();
    assert!(form.channel_field(&key()).is_err());
    assert!(
        form.conversation_field(&InputKey {
            block_id: "foreign".into(),
            action_id: "pick".into()
        })
        .is_err()
    );
    form.fields.get_mut(&key()).unwrap().raw["response_url_enabled"] = json!(true);
    assert!(form.conversation_field(&key()).is_err());
    raw["blocks"][0]["type"] = json!("section");
    raw["blocks"][0]["accessory"] = raw["blocks"][0]["element"].clone();
    assert!(HomeForm::parse(&raw).unwrap().state().is_err());
}

#[test]
fn malformed_values_filters_and_wire_types_cannot_enter_home_state() {
    for id in ["", "Cbad", "C1", "TTEAM0001"] {
        let mut form = HomeForm::parse(&raw(true)).unwrap();
        form.set(&key(), InputValue::Conversation(Some(id.into())))
            .unwrap();
        assert!(form.state().is_err());
    }
    for filter in [
        json!({"include":["public","public"]}),
        json!({"include":["unknown"]}),
        json!({"only_from_team_id":3}),
        json!({"exclude_bot_users":"true"}),
        json!({"foreign":true}),
    ] {
        let mut raw = raw(true);
        raw["blocks"][0]["element"]["filter"] = filter;
        assert!(HomeForm::parse(&raw).is_err());
    }
    let mut form = HomeForm::parse(&raw(true)).unwrap();
    assert!(form.set(&key(), InputValue::Channel(None)).is_err());
    for flag in [json!(1), json!("true"), json!({})] {
        let mut raw = raw(true);
        raw["blocks"][0]["element"]["response_url_enabled"] = flag;
        assert!(HomeForm::parse(&raw).is_err());
    }
}

#[path = "multiple_tests.rs"]
mod multiple;
