use super::*;
use crate::block_actions::home::Home;
use crate::slack::client::RequestBody;
use std::collections::BTreeMap;

fn raw() -> Value {
    json!({"type":"home","id":"VHOME","app_id":"AHOME","app_installed_team_id":"TINSTALLED","bot_id":"BHOME","hash":"h1","function_execution_id":"must-not-leak","blocks":[{"type":"input","block_id":"choice","dispatch_action":false,"label":{"type":"plain_text","text":"Remote choice"},"element":{"type":"external_select","action_id":"pick","min_query_length":0}}]})
}
fn home() -> Home {
    Home {
        channel: "DHOME".into(),
        app: "AHOME".into(),
        installed_team: "TINSTALLED".into(),
        enabled: true,
        messages: true,
        view_id: Some("VHOME".into()),
    }
}
fn key() -> InputKey {
    InputKey {
        block_id: "choice".into(),
        action_id: "pick".into(),
    }
}
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TACTIVE".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    }
}
fn context(raw: Value) -> Result<SuggestionsContext, Error> {
    SuggestionsContext::home("TACTIVE", "U1", None, &View::decode(&home(), raw)?, &key())
}

#[test]
fn home_suggestions_use_explicit_context_or_installed_team_not_modal_authority() {
    let view = View::decode(&home(), raw()).unwrap();
    for (context_team, expected) in [
        (None, "TINSTALLED"),
        (Some(""), "TINSTALLED"),
        (Some("TCONTEXT"), "TCONTEXT"),
    ] {
        let context =
            SuggestionsContext::home("TACTIVE", "U1", context_team, &view, &key()).unwrap();
        let request = block_suggestions(
            &SlackClient::default(),
            &workspace(),
            &context,
            "  東京😀&x=1 ",
        )
        .unwrap();
        assert!(!request.retry_safe());
        assert!(request.url.contains("/api/blocks.suggestions"));
        let RequestBody::Form(fields) = request.body else {
            panic!("expected form")
        };
        let mut fields: BTreeMap<_, _> = fields.into_iter().collect();
        assert_eq!(fields.remove("token").as_deref(), Some("synthetic"));
        assert_eq!(
            fields,
            BTreeMap::from([
                ("service_id".into(), "BHOME".into()),
                ("app_id".into(), "AHOME".into()),
                ("service_team_id".into(), expected.into()),
                (
                    "container".into(),
                    json!({"type":"view","view_id":"VHOME"}).to_string()
                ),
                ("block_id".into(), "choice".into()),
                ("action_id".into(), "pick".into()),
                ("value".into(), "  東京😀&x=1 ".into())
            ])
        );
    }
    let context = context(raw()).unwrap();
    for wrong in ["team", "user"] {
        let mut workspace = workspace();
        if wrong == "team" {
            workspace.team_id = "TOTHER".into();
        } else {
            workspace.user_id = "U2".into();
        }
        assert!(block_suggestions(&SlackClient::default(), &workspace, &context, "query").is_err());
    }
    assert!(
        block_suggestions(
            &SlackClient::default(),
            &workspace(),
            &context,
            &"😀".repeat(MAX_QUERY_UNITS / 2 + 1)
        )
        .is_err()
    );
}

#[test]
fn missing_malformed_foreign_and_mutated_home_routes_cannot_gain_suggestions_authority() {
    for field in ["id", "app_id", "app_installed_team_id", "bot_id"] {
        let mut raw = raw();
        raw.as_object_mut().unwrap().remove(field);
        assert!(context(raw).is_err(), "missing {field}");
    }
    for kind in ["modal", "message"] {
        let mut raw = raw();
        raw["type"] = json!(kind);
        assert!(context(raw).is_err());
    }
    let view = View::decode(&home(), raw()).unwrap();
    for malformed in [" ", "T\nOTHER"] {
        assert!(SuggestionsContext::home("TACTIVE", "U1", Some(malformed), &view, &key()).is_err());
    }
    for foreign in [
        InputKey {
            block_id: "foreign".into(),
            action_id: "pick".into(),
        },
        InputKey {
            block_id: "choice".into(),
            action_id: "other".into(),
        },
    ] {
        assert!(SuggestionsContext::home("TACTIVE", "U1", None, &view, &foreign).is_err());
    }
    let mut changed = view.clone();
    changed.home.app = "OTHER".into();
    assert!(SuggestionsContext::home("TACTIVE", "U1", None, &changed, &key()).is_err());
    let mut changed = view.clone();
    changed.home.enabled = false;
    assert!(SuggestionsContext::home("TACTIVE", "U1", None, &changed, &key()).is_err());
}

#[test]
fn home_request_admission_and_form_admission_share_single_dispatch_boundary() {
    let mut dispatching = raw();
    dispatching["blocks"][0]["dispatch_action"] = json!(true);
    assert!(context(dispatching.clone()).is_ok());
    assert!(
        HomeForm::parse(&dispatching)
            .unwrap()
            .external_field(&key())
            .is_ok()
    );
    for mode in 0..8 {
        let mut raw = raw();
        match mode {
            0 => raw["blocks"][0]["dispatch_action"] = json!("true"),
            1 => raw["blocks"][0]["element"]["type"] = json!("multi_users_select"),
            2 => raw["blocks"][0]["element"]["confirm"] = Value::Null,
            3 => raw["blocks"][0]["element"]["dispatch_action_config"] = json!({}),
            4 => raw["blocks"][0]["element"]["visible_to_user_ids"] = json!(["U1"]),
            5 => raw["blocks"][0]["element"]["third_party_auth"] = json!({}),
            6 => {
                let block = raw["blocks"][0].clone();
                raw["blocks"].as_array_mut().unwrap().push(block);
            }
            _ => {
                let element = raw["blocks"][0]["element"].clone();
                raw["blocks"][0] =
                    json!({"type":"section","block_id":"choice","accessory":element});
            }
        }
        assert!(context(raw.clone()).is_err(), "route {mode}");
        assert!(
            HomeForm::parse(&raw)
                .and_then(|form| form.external_field(&key()).map(|_| ()))
                .is_err(),
            "form {mode}"
        );
    }
}
