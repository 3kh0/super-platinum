use super::*;
use super_platinum_core::config::WorkspaceSession;

fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T00000001".into(),
        user_id: "U00000000".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    }
}
#[test]
fn first_n_attempt_expires_strictly_after_twelve_hours_even_when_failed() {
    let start = Instant::now();
    let interval = Duration::from_millis(membership::ATTEMPT_INTERVAL_MILLIS);
    assert!(due(None, start));
    assert!(!due(Some(start), start));
    assert!(!due(Some(start), start + interval));
    assert!(due(Some(start), start + interval + Duration::from_nanos(1)));
}
#[test]
fn context_preserves_root_order_and_unknown_vs_negative_without_reply_author_inference() {
    let owner = session();
    let mut cache = membership::Cache::capture(&owner, "CCOMMENT01").unwrap();
    let read = membership::Read::membership(
        &owner,
        "CCOMMENT01",
        vec!["U00000001".into(), "U00000002".into()],
    )
    .unwrap();
    cache.begin(&read).unwrap();
    let root = json!({"channelId":"CCOMMENT01","rootTs":"1700000000.000001",
        "root":{"reply_users":["U00000002","U00000001","U00000002"],"opaque":7},
        "threadUsers":["U00000002","U00000001","U00000002"]});
    let channel = json!({"id":"CCOMMENT01","is_file":true,"is_ext_shared":true});
    let pending = context_value(&root, &channel, &cache);
    assert_eq!(
        pending["membership"]["U00000001"],
        json!({"isKnown":false,"isMember":false})
    );
    assert!(pending["membership"].get("U00000003").is_none());
    let response = membership::Membership::decode(
        &read,
        &owner,
        json!({"members":["U00000002"],"non_members":["U00000001"]}),
    )
    .unwrap();
    cache.accept(&response).unwrap();
    let settled = context_value(&root, &channel, &cache);
    assert_eq!(
        settled["membership"]["U00000001"],
        json!({"isKnown":true,"isMember":false})
    );
    assert_eq!(
        settled["membership"]["U00000002"],
        json!({"isKnown":true,"isMember":true})
    );
    assert_eq!(settled["root"], root["root"]);
    assert_eq!(settled["threadUsers"], root["threadUsers"]);
    assert_eq!(settled["channel"], channel);
    assert_eq!(
        settled["channelSelectors"],
        json!({"isImOrMpim":false,"unknownOrNonexistent":false,"externallyShared":true})
    );
    cache.clear(&read).unwrap();
    cache.accept(&response).unwrap();
    assert_eq!(
        context_value(&root, &channel, &cache)["membership"],
        json!({})
    );
}
#[test]
fn observed_missing_channel_remains_unknown_without_synthetic_list_flags() {
    let cache = membership::Cache::capture(&session(), "CCOMMENT01").unwrap();
    let root = json!({"channelId":"CCOMMENT01","rootTs":"1.000001","root":null,"threadUsers":[]});
    let result = context_value(&root, &json!({"id":"CCOMMENT01","isUnknown":true}), &cache);
    assert_eq!(
        result["channel"],
        json!({"id":"CCOMMENT01","isUnknown":true})
    );
    assert_eq!(result["root"], Value::Null);
    assert_eq!(result["membership"], json!({}));
    assert_eq!(result["channelSelectors"]["unknownOrNonexistent"], true);
}
#[test]
fn original_transport_credentials_account_socket_and_native_self_are_all_required() {
    for change in 0..7 {
        let (mut shell, _, _local) = draft::tests::setup();
        shell.core.active_account = Some("member-context-fixture".into());
        let owner = Fence::capture(&shell).unwrap();
        assert!(owner.current(&shell));
        let team = owner.context.session.team_id.clone();
        match change {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.active_account = Some("successor".into()),
            2 => shell
                .core
                .session
                .as_mut()
                .unwrap()
                .d_cookie
                .push_str("changed"),
            3 => shell
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut(&team)
                .unwrap()
                .token
                .push_str("changed"),
            4 => shell.core.workspaces.get_mut(&team).unwrap().rt_generation += 1,
            5 => shell.core.workspaces.get_mut(&team).unwrap().self_user_id = "OTHER".into(),
            _ => shell.core.transport = None,
        }
        assert!(!owner.current(&shell), "mutation {change}");
    }
}
