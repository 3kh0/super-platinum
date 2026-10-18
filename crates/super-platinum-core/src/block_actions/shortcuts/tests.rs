use super::*;
use crate::slack::client::RequestBody;
fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    }
}
fn catalog() -> Catalog {
    Catalog::decode(json!({"ok":true,"cache_ts":"opaque","commands":[],"app_actions":[
    {"app_id":"A1","app_name":"Same name","teams":["T1","T2"],"unknown":{"preserve":true},"actions":[
        {"action_id":"Aa1","type":"global_action","name":"Review"},{"action_id":"Aa2","type":"message_action","name":"Review"}]},
    {"app_id":"A2","app_name":"Same name","is_workspace_agnostic":true,"teams":["T9"],"actions":[{"action_id":"Aa1","type":"global_action","name":"Review"}]}]})).unwrap()
}
#[test]
fn exact_global_and_message_forms_preserve_context_without_invented_thread() {
    let c = catalog();
    let client = SlackClient::default();
    let related = vec!["T1".into(), "T2".into()];
    for (i, ctx, team) in [
        (
            0,
            Context {
                channel: Some("C1".into()),
                message_ts: None,
            },
            Some("T2"),
        ),
        (
            1,
            Context {
                channel: Some("C1".into()),
                message_ts: Some("1700.000001".into()),
            },
            None,
        ),
        (
            2,
            Context {
                channel: None,
                message_ts: None,
            },
            None,
        ),
    ] {
        let r = prepare(
            &client,
            &session(),
            &c,
            &c.actions[i],
            &ctx,
            &related,
            team,
            "web-1",
        )
        .unwrap();
        assert!(r.url.contains("/api/apps.actions.v2.execute?"));
        assert!(!r.retry_safe());
        let RequestBody::Form(f) = r.body else {
            panic!()
        };
        let fields: std::collections::BTreeMap<_, _> =
            f.into_iter().filter(|(k, _)| k != "token").collect();
        assert_eq!(fields.len(), if team.is_some() { 6 } else { 5 });
        assert_eq!(fields["action_id"], c.actions[i].id);
        assert_eq!(
            fields["_x_reason"],
            "app-global-actions-api/execute-app-action"
        );
        let context: Value = serde_json::from_str(&fields["context"]).unwrap();
        assert_eq!(
            context.get("channel_id").and_then(Value::as_str),
            ctx.channel.as_deref()
        );
        assert_eq!(
            context.get("message_ts").and_then(Value::as_str),
            ctx.message_ts.as_deref()
        );
        assert!(!fields.contains_key("thread_ts"));
        assert!(!fields.contains_key("type"));
    }
}
#[test]
fn team_choice_and_catalog_identity_are_rechecked_at_capture() {
    let c = catalog();
    let a = &c.actions[0];
    let ctx = Context {
        channel: Some("C1".into()),
        message_ts: None,
    };
    let related = vec!["T1".into(), "T2".into()];
    assert!(
        prepare(
            &SlackClient::default(),
            &session(),
            &c,
            a,
            &ctx,
            &related,
            None,
            "web-1"
        )
        .is_err()
    );
    assert!(
        prepare(
            &SlackClient::default(),
            &session(),
            &c,
            a,
            &ctx,
            &related,
            Some("T9"),
            "web-1"
        )
        .is_err()
    );
    let mut changed = a.clone();
    changed.raw_action["new"] = json!(true);
    assert!(
        prepare(
            &SlackClient::default(),
            &session(),
            &c,
            &changed,
            &ctx,
            &related,
            Some("T1"),
            "web-1"
        )
        .is_err()
    );
    assert_eq!(c.actions.len(), 3);
    assert_eq!(a.raw_app["unknown"], json!({"preserve":true}));
    assert!(!c.actions[1].eligible(&["T9".into()]));
    assert!(c.actions[2].eligible(&["T1".into()]));
}
#[test]
fn ambiguous_or_malformed_catalog_and_wrong_context_never_admit() {
    let c = catalog();
    let mut raw = c.raw.clone();
    let duplicate = raw["app_actions"][0]["actions"][0].clone();
    raw["app_actions"][0]["actions"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert!(Catalog::decode(raw).is_err());
    let ctx = Context {
        channel: None,
        message_ts: Some("1700.1".into()),
    };
    assert!(!ctx.validate(Kind::Message));
    assert!(!ctx.validate(Kind::Global));
    for value in [
        json!({"ok":true}),
        json!({"ok":false}),
        json!({"ok":true,"app_actions":null}),
    ] {
        assert!(Catalog::decode(value).is_err());
    }
}
#[test]
fn acknowledgement_is_distinct_from_unknown_or_restricted() {
    assert_eq!(decode(&json!({"ok":true})), Outcome::Acknowledged);
    assert_eq!(
        decode(&json!({"ok":false,"error":"app_access_restricted"})),
        Outcome::Restricted
    );
    for v in [
        json!({}),
        json!({"ok":"true"}),
        json!({"ok":false,"error":"dispatch_failed"}),
    ] {
        assert_eq!(decode(&v), Outcome::Unknown);
    }
}

#[test]
fn global_zero_intersection_omits_team_and_special_calendar_never_routes_to_v2() {
    let c = catalog();
    let related = vec!["T9".into()];
    let a = &c.actions[0];
    assert!(a.eligible(&related));
    assert!(a.applicable_teams(&related).is_empty());
    assert!(
        prepare(
            &SlackClient::default(),
            &session(),
            &c,
            a,
            &Context {
                channel: Some("C1".into()),
                message_ts: None
            },
            &related,
            None,
            "web-1"
        )
        .is_ok()
    );
    for app in [
        "A0CNP2EP5",
        "ADZ494LHY",
        "A07H429LK",
        "AFV5ECLBZ",
        "A0608DQ3P",
        "A1S0QJ0SQ",
        "A012Z6404M8",
        "A01NK8UJRGC",
    ] {
        let mut c = c.clone();
        c.actions[0].app = app.into();
        assert!(!c.actions[0].eligible(&related));
    }
}
