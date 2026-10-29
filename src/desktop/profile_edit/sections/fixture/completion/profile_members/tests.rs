use super::*;
use super_platinum_core::slack::api::custom::texty::members::{Query, RequestFacts};

fn owner() -> WorkspaceSession {
    serde_json::from_value(
        json!({"team_id":"TINIT0001","user_id":"UINIT0001","name":"Completion fixture",
        "url":"https://example.test","token":"synthetic-completion-token"}),
    )
    .unwrap()
}
fn planned(client: &SlackClient, w: &WorkspaceSession, sdk: bool) -> PreparedRequest {
    let now = Instant::now();
    let endpoint = DirectoryEndpoint::decode(w, &discovery(w), now).unwrap();
    let query = Query::capture(
        w,
        " @raw\0\n ",
        RequestFacts {
            top_users: Some(json!([])),
            default_workspace: Some(w.team_id.clone()),
        },
    )
    .unwrap();
    if sdk {
        query
            .request_for_profile_long_text(client, w, &endpoint, now, None)
            .unwrap()
    } else {
        query.request(client, w, &endpoint, now, None).unwrap()
    }
}
#[test]
fn named_sdk_validator_preserves_legacy_refusal_exact_auth_and_original_redacted_arguments() {
    let client = SlackClient::default();
    let w = owner();
    let legacy = planned(&client, &w, false);
    let sdk = planned(&client, &w, true);
    assert!(super::super::validate(&client, &w, &legacy).is_ok());
    assert!(validate(&client, &w, &legacy).is_err());
    assert!(super::super::validate(&client, &w, &sdk).is_err());
    let accepted = validate(&client, &w, &sdk).unwrap();
    assert_eq!(accepted.method, "users/search");
    assert_eq!(accepted.args["include_profile_only_users"], true);
    assert_eq!(accepted.args["query"], " @raw\0\n ");
    assert!(accepted.args.get("token").is_none());
    assert!(accepted.args.get("enterprise_token").is_none());
    assert_eq!(
        accepted.reply.0,
        members::reply(&w, Mode::Success).unwrap().0
    );
    let model = Model::default();
    record(&model, &accepted);
    let snapshot = model.snapshot();
    assert_eq!(
        snapshot["requests"][0]["args"]["include_profile_only_users"],
        true
    );
    assert_eq!(snapshot["networkCalls"], 0);
    assert!(!snapshot.to_string().contains(&w.token));
    for flag in [Value::Null, json!(false), json!("true"), json!(1)] {
        let mut bad = sdk.clone();
        let RequestBody::Json(body) = &mut bad.body else {
            unreachable!()
        };
        body["include_profile_only_users"] = flag;
        assert!(validate(&client, &w, &bad).is_err());
    }
    for (key, value) in [
        ("token", json!("different-nonempty")),
        ("enterprise_token", json!("unexpected")),
        ("top_users", json!(["UOTHER001"])),
        ("default_workspace", json!("TOTHER001")),
        ("filter", json!("NOT deactivated")),
        ("count", json!(24)),
        ("marker", json!("unissued")),
    ] {
        let mut bad = sdk.clone();
        let RequestBody::Json(body) = &mut bad.body else {
            unreachable!()
        };
        body[key] = value;
        assert!(validate(&client, &w, &bad).is_err());
    }
    let mut headers = sdk.clone();
    headers.headers[0].1.push('x');
    assert!(validate(&client, &w, &headers).is_err());
    let mut route = sdk;
    route.url = route.url.replace("TINIT0001", "TOTHER001");
    assert!(validate(&client, &w, &route).is_err());
}
