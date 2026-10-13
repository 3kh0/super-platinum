use super::*;
use crate::config::WorkspaceSession;

use super::super::client::RequestBody;

fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "E09V59WQY1E".into(),
        enterprise_id: Some("E09V59WQY1E".into()),
        user_id: "U080A3QP42C".into(),
        name: "Hack Club".into(),
        url: "https://hackclub.enterprise.slack.com".into(),
        token: "xoxc-test-token".into(),
    }
}

fn form_fields(req: &PreparedRequest) -> &Vec<(String, String)> {
    match &req.body {
        RequestBody::Form(fields) => fields,
        other => panic!("expected form body, got {other:?}"),
    }
}

#[test]
fn channel_details_writes_allow_clearing_and_reads_page_explicitly() {
    let client = SlackClient::default();
    let session = workspace();
    for field in [ChannelTextField::Topic, ChannelTextField::Description] {
        let request = conversation_set_text(&client, &session, "C1", field, "");
        assert!(form_fields(&request).contains(&(field.key().into(), String::new())));
        assert!(!request.retry_safe());
    }
    let request = conversation_members(&client, &session, "C1", Some("cursor-one"));
    assert!(request.retry_safe());
    assert!(form_fields(&request).contains(&("cursor".into(), "cursor-one".into())));
    let edge =
        conversation_members_edge(&client, &session, "C1", Some("next-person"), 500).unwrap();
    assert!(edge.retry_safe());
    let RequestBody::Json(body) = edge.body else {
        panic!("edge members use JSON")
    };
    assert_eq!(body["channels"], serde_json::json!(["C1"]));
    assert_eq!(body["marker"], "next-person");
    assert_eq!(body["filter"], "people");
    assert_eq!(body["count"], 100);
    let mut members = ConversationMembers::default();
    let page = |ids: Vec<&str>, cursor: &str| {
        serde_json::from_value(
            serde_json::json!({"members":ids,"response_metadata":{"next_cursor":cursor}}),
        )
        .unwrap()
    };
    assert!(members.apply(page(vec!["U1", "U2"], "next")));
    assert!(!members.apply(page(vec!["U2", "U3"], "next")));
    assert_eq!(members.ids, vec!["U1", "U2", "U3"]);
    assert!(members.cursor.is_none());
    let channel: Channel = serde_json::from_value(
        serde_json::json!({"id":"C1","topic":{"value":"Topic 🚀"},"purpose":"Description"}),
    )
    .unwrap();
    assert_eq!(ChannelTextField::Topic.value(&channel), "Topic 🚀");
    assert_eq!(ChannelTextField::Description.value(&channel), "Description");
}

#[test]
fn member_mutations_preserve_pagination_and_can_remove_then_readd() {
    let mut members = ConversationMembers::default();
    members.apply(
        serde_json::from_value(
            serde_json::json!({"members":["U1","U2"],"response_metadata":{"next_cursor":"next"}}),
        )
        .unwrap(),
    );
    members.confirm_member("U2", false);
    members.confirm_member("U3", true);
    members.confirm_member("U3", true);
    assert_eq!(members.ids, vec!["U1", "U3"]);
    assert_eq!(members.cursor.as_deref(), Some("next"));
    members.confirm_member("U2", true);
    assert!(
        members.apply(serde_json::from_value(serde_json::json!({"members":["U2","U4"]})).unwrap())
    );
    assert_eq!(members.ids, vec!["U1", "U3", "U2", "U4"]);
}

#[test]
fn history_request_targets_enterprise_host_with_channel_and_limit() {
    let request = conversations_history(
        &SlackClient::default(),
        &workspace(),
        HistoryArgs {
            channel: "C0159TSJVH8".into(),
            limit: Some(50),
            ..Default::default()
        },
    );
    let fields = form_fields(&request);

    assert!(request.url.contains("/api/conversations.history?"));
    assert!(
        request
            .url
            .starts_with("https://hackclub.enterprise.slack.com")
    );
    assert!(fields.contains(&("channel".into(), "C0159TSJVH8".into())));
    assert!(fields.contains(&("limit".into(), "50".into())));
    assert!(fields.contains(&("token".into(), "xoxc-test-token".into())));
    assert!(!request.redacted_debug().contains("xoxc-test-token"));
}

#[test]
fn replies_request_includes_thread_target() {
    let request = conversations_replies(
        &SlackClient::default(),
        &workspace(),
        RepliesArgs {
            channel: "C0159TSJVH8".into(),
            ts: "1783372360.741769".into(),
            oldest: Some("1783372400.000001".into()),
            latest: Some("1783372500.000001".into()),
            limit: Some(200),
            inclusive: true,
            ..Default::default()
        },
    );
    let fields = form_fields(&request);

    assert!(request.url.contains("/api/conversations.replies?"));
    assert!(fields.contains(&("channel".into(), "C0159TSJVH8".into())));
    assert!(fields.contains(&("ts".into(), "1783372360.741769".into())));
    assert!(fields.contains(&("oldest".into(), "1783372400.000001".into())));
    assert!(fields.contains(&("latest".into(), "1783372500.000001".into())));
    assert!(fields.contains(&("limit".into(), "200".into())));
    assert!(fields.contains(&("inclusive".into(), "true".into())));
}

#[test]
fn team_connections_request_includes_channel() {
    let request =
        conversations_team_connections(&SlackClient::default(), &workspace(), "C0ANA97M17Y".into());
    let fields = form_fields(&request);

    assert!(request.url.contains("/api/conversations.teamConnections?"));
    assert!(fields.contains(&("channel".into(), "C0ANA97M17Y".into())));
}

#[test]
fn activity_mark_read_sends_type_feed_ts_and_key() {
    let request = activity_mark_read(
        &SlackClient::default(),
        &workspace(),
        "thread_v2".into(),
        "1787818475.024439".into(),
        "thread_v2-C07TM4C0AQ5-1787817119.776049".into(),
    );
    let fields = form_fields(&request);

    assert!(request.url.contains("/api/activity.markRead?"));
    assert!(fields.contains(&("type".into(), "thread_v2".into())));
    assert!(fields.contains(&("feed_ts".into(), "1787818475.024439".into())));
    assert!(fields.contains(&(
        "key".into(),
        "thread_v2-C07TM4C0AQ5-1787817119.776049".into()
    )));
}

#[test]
fn send_request_includes_channel_and_text() {
    let request = chat_post_message(
        &SlackClient::default(),
        &workspace(),
        "C0159TSJVH8".into(),
        "hello from super-platinum".into(),
        None,
    );
    let fields = form_fields(&request);

    assert!(request.url.contains("/api/chat.postMessage?"));
    assert!(fields.contains(&("channel".into(), "C0159TSJVH8".into())));
    assert!(fields.contains(&("text".into(), "hello from super-platinum".into())));
}

#[test]
fn send_thread_reply_includes_thread_ts() {
    let request = chat_post_message(
        &SlackClient::default(),
        &workspace(),
        "C0159TSJVH8".into(),
        "reply from super-platinum".into(),
        Some("1783372360.741769".into()),
    );
    let fields = form_fields(&request);

    assert!(request.url.contains("/api/chat.postMessage?"));
    assert!(fields.contains(&("channel".into(), "C0159TSJVH8".into())));
    assert!(fields.contains(&("text".into(), "reply from super-platinum".into())));
    assert!(fields.contains(&("thread_ts".into(), "1783372360.741769".into())));
}

#[test]
fn search_request_targets_messages_module_with_query_and_page() {
    let request = search_messages(
        &SlackClient::default(),
        &workspace(),
        SearchArgs {
            sort: crate::search::SearchSort::Oldest,
            query: "deploy failed".into(),
            count: 20,
            page: 2,
        },
    );
    let fields = form_fields(&request);

    assert!(request.url.contains("/api/search.modules.messages?"));
    assert!(fields.contains(&("module".into(), "messages".into())));
    assert!(fields.contains(&("query".into(), "deploy failed".into())));
    assert!(fields.contains(&("sort".into(), "timestamp".into())));
    assert!(fields.contains(&("sort_dir".into(), "asc".into())));
    assert!(fields.contains(&("count".into(), "20".into())));
    assert!(fields.contains(&("page".into(), "2".into())));
    assert!(fields.iter().any(|(k, _)| k == "client_req_id"));
    assert!(fields.iter().any(|(k, _)| k == "search_session_id"));
}

#[test]
fn search_inline_request_targets_channel_with_query() {
    let request = search_inline(
        &SlackClient::default(),
        &workspace(),
        SearchInlineArgs {
            query: "deploy".into(),
            channel: "C0BBMA16677".into(),
            count: 20,
            page: 1,
        },
    );
    let fields = form_fields(&request);

    assert!(request.url.contains("/api/search.inline?"));
    assert!(fields.contains(&("query".into(), "deploy".into())));
    assert!(fields.contains(&("channel".into(), "C0BBMA16677".into())));
    assert!(fields.contains(&("count".into(), "20".into())));
    assert!(fields.iter().any(|(k, _)| k == "client_req_id"));
    assert!(fields.iter().any(|(k, _)| k == "search_session_id"));
}

#[test]
fn update_request_includes_channel_ts_and_text() {
    let request = chat_update(
        &SlackClient::default(),
        &workspace(),
        "C0159TSJVH8".into(),
        "1783372360.741769".into(),
        "edited body".into(),
    );
    let fields = form_fields(&request);

    assert!(request.url.contains("/api/chat.update?"));
    assert!(fields.contains(&("channel".into(), "C0159TSJVH8".into())));
    assert!(fields.contains(&("ts".into(), "1783372360.741769".into())));
    assert!(fields.contains(&("text".into(), "edited body".into())));
}

#[test]
fn delete_request_includes_channel_and_ts() {
    let request = chat_delete(
        &SlackClient::default(),
        &workspace(),
        "C0159TSJVH8".into(),
        "1783372360.741769".into(),
    );
    let fields = form_fields(&request);

    assert!(request.url.contains("/api/chat.delete?"));
    assert!(fields.contains(&("channel".into(), "C0159TSJVH8".into())));
    assert!(fields.contains(&("ts".into(), "1783372360.741769".into())));
}

#[test]
fn mark_request_includes_channel_and_ts() {
    let request = conversations_mark(
        &SlackClient::default(),
        &workspace(),
        "C0159TSJVH8".into(),
        "1783372400.111111".into(),
    );
    let fields = form_fields(&request);

    assert!(request.url.contains("/api/conversations.mark?"));
    assert!(fields.contains(&("channel".into(), "C0159TSJVH8".into())));
    assert!(fields.contains(&("ts".into(), "1783372400.111111".into())));
}

#[test]
fn thread_mark_request_includes_thread_and_latest_reply() {
    let request = subscriptions_thread_mark(
        &SlackClient::default(),
        &workspace(),
        "C0159TSJVH8".into(),
        "1783372300.111111".into(),
        "1783372400.222222".into(),
    );
    let fields = form_fields(&request);

    assert!(request.url.contains("/api/subscriptions.thread.mark?"));
    assert!(fields.contains(&("channel".into(), "C0159TSJVH8".into())));
    assert!(fields.contains(&("thread_ts".into(), "1783372300.111111".into())));
    assert!(fields.contains(&("ts".into(), "1783372400.222222".into())));
    assert!(fields.contains(&("read".into(), "1".into())));
}

#[test]
fn threads_view_request_matches_slack_feed_fields() {
    let request = subscriptions_thread_get_view(
        &SlackClient::default(),
        &workspace(),
        20,
        Some("1783372400.222222".into()),
        false,
    );
    let fields = form_fields(&request);

    assert!(request.url.contains("/api/subscriptions.thread.getView?"));
    assert!(request.retry_safe());
    assert!(fields.contains(&("limit".into(), "20".into())));
    assert!(fields.contains(&("fetch_threads_state".into(), "true".into())));
    assert!(fields.contains(&("priority_mode".into(), "all".into())));
    assert!(fields.contains(&("current_ts".into(), "1783372400.222222".into())));

    let vip = subscriptions_thread_get_view(&SlackClient::default(), &workspace(), 20, None, true);
    assert!(form_fields(&vip).contains(&("priority_mode".into(), "priority".into())));
}

#[test]
fn conversations_info_request_targets_channel() {
    let request = conversations_info(&SlackClient::default(), &workspace(), "C123".into());
    let fields = form_fields(&request);

    assert!(request.url.contains("/api/conversations.info?"));
    assert!(fields.contains(&("channel".into(), "C123".into())));
    assert!(fields.contains(&("include_num_members".into(), "false".into())));
}

#[test]
fn reaction_request_includes_message_target() {
    let request = reactions_add(
        &SlackClient::default(),
        &workspace(),
        "C0159TSJVH8".into(),
        "1783372360.741769".into(),
        "thumbsup".into(),
    );
    let fields = form_fields(&request);

    assert!(request.url.contains("/api/reactions.add?"));
    assert!(fields.contains(&("channel".into(), "C0159TSJVH8".into())));
    assert!(fields.contains(&("timestamp".into(), "1783372360.741769".into())));
    assert!(fields.contains(&("name".into(), "thumbsup".into())));
}

#[test]
fn counts_request_targets_client_counts() {
    let request = client_counts(&SlackClient::default(), &workspace());
    assert!(request.url.contains("/api/client.counts?"));
    assert!(form_fields(&request).contains(&("token".into(), "xoxc-test-token".into())));
}

#[test]
fn sidebar_dms_request_targets_sidebar_dms() {
    let request = sidebar_dms(&SlackClient::default(), &workspace());
    assert!(request.url.contains("/api/sidebar.dms?"));
    assert!(form_fields(&request).contains(&("token".into(), "xoxc-test-token".into())));
}

#[test]
fn activity_request_includes_pagination_cursor() {
    let request = activity_feed(
        &SlackClient::default(),
        &workspace(),
        20,
        Some("older-activity-cursor".into()),
        true,
    );
    let fields = form_fields(&request);

    assert!(request.url.contains("/api/activity.feed?"));
    assert!(fields.contains(&("limit".into(), "20".into())));
    assert!(fields.contains(&("cursor".into(), "older-activity-cursor".into())));
    assert!(fields.contains(&("mode".into(), "chrono_v1".into())));
    assert!(fields.contains(&("unread_only".into(), "true".into())));
    assert!(fields.contains(&("automations_only".into(), "false".into())));
    assert!(fields.contains(&("is_activity_inbox".into(), "true".into())));
    let types = fields
        .iter()
        .find_map(|(name, value)| (name == "types").then_some(value.as_str()))
        .expect("activity types");
    assert!(types.contains("prejoin_dm_welcome_party_alert"));
    assert!(types.contains("quietly_added_to_channel"));
}

#[test]
fn client_dms_request_matches_desktop_args() {
    let request = client_dms(
        &SlackClient::default(),
        &workspace(),
        100,
        Some("older-dms-cursor".into()),
    );
    let fields = form_fields(&request);

    assert!(request.url.contains("/api/client.dms?"));
    assert!(fields.contains(&("count".into(), "100".into())));
    assert!(fields.contains(&("include_closed".into(), "true".into())));
    assert!(fields.contains(&("include_channel".into(), "true".into())));
    assert!(fields.contains(&("exclude_bots".into(), "true".into())));
    assert!(fields.contains(&("priority_mode".into(), "priority".into())));
    assert!(fields.contains(&("cursor".into(), "older-dms-cursor".into())));
}

#[test]
fn set_presence_request_includes_presence_value() {
    let request = users_set_presence(&SlackClient::default(), &workspace(), "away".into());
    let fields = form_fields(&request);
    assert!(request.url.contains("/api/users.setPresence?"));
    assert!(fields.contains(&("presence".into(), "away".into())));
    assert!(fields.contains(&("token".into(), "xoxc-test-token".into())));
}

#[test]
fn set_snooze_request_sends_minutes() {
    let request = dnd_set_snooze(&SlackClient::default(), &workspace(), 60);
    let fields = form_fields(&request);
    assert!(request.url.contains("/api/dnd.setSnooze?"));
    assert!(fields.contains(&("num_minutes".into(), "60".into())));
    assert!(fields.contains(&("token".into(), "xoxc-test-token".into())));
}

#[test]
fn end_snooze_request_is_a_bare_form_post() {
    let request = dnd_end_snooze(&SlackClient::default(), &workspace());
    assert!(request.url.contains("/api/dnd.endSnooze?"));
    assert!(form_fields(&request).contains(&("token".into(), "xoxc-test-token".into())));
}

#[test]
fn profile_set_request_posts_the_profile_json() {
    let profile = r#"{"status_text":"","status_emoji":"","status_expiration":0}"#;
    let request = users_profile_set(&SlackClient::default(), &workspace(), profile.into());
    let fields = form_fields(&request);
    assert!(request.url.contains("/api/users.profile.set?"));
    assert!(fields.contains(&("profile".into(), profile.into())));
    assert!(fields.contains(&("token".into(), "xoxc-test-token".into())));
}

#[test]
fn profile_request_targets_user_with_desktop_token() {
    let request = users_profile_get(&SlackClient::default(), &workspace(), "U_PROFILE".into());
    let fields = form_fields(&request);
    assert!(request.url.contains("/api/users.profile.get?"));
    assert!(fields.contains(&("user".into(), "U_PROFILE".into())));
    assert!(fields.contains(&("token".into(), "xoxc-test-token".into())));
}

#[test]
fn profile_extras_request_matches_slack_recent_dms_contract() {
    let request =
        users_profile_get_extras(&SlackClient::default(), &workspace(), "U_PROFILE".into());
    let fields = form_fields(&request);
    assert!(request.url.contains("/api/users.profile.getExtras?"));
    assert!(fields.contains(&("user".into(), "U_PROFILE".into())));
    assert!(fields.contains(&("keys".into(), "im_mpim_ids".into())));
}

#[test]
fn team_profile_request_fetches_custom_field_schema() {
    let request = team_profile_get(&SlackClient::default(), &workspace());
    assert!(request.url.contains("/api/team.profile.get?"));
    assert!(form_fields(&request).contains(&("token".into(), "xoxc-test-token".into())));
}

#[test]
fn priority_requests_target_user_with_desktop_token() {
    let add = users_priority_add(&SlackClient::default(), &workspace(), "U_VIP".into());
    assert!(add.url.contains("/api/users.priority.add?"));
    assert!(form_fields(&add).contains(&("user".into(), "U_VIP".into())));
    assert!(form_fields(&add).contains(&("token".into(), "xoxc-test-token".into())));

    let remove = users_priority_remove(&SlackClient::default(), &workspace(), "U_VIP".into());
    assert!(remove.url.contains("/api/users.priority.remove?"));
    assert!(form_fields(&remove).contains(&("user".into(), "U_VIP".into())));
}

/// The reachability probe that releases held requests has to aim at the host
/// those requests are going to, which on a grid is not `slack.com`.
#[test]
fn the_probe_host_follows_the_workspace_to_its_grid() {
    assert_eq!(
        crate::slack::api_host(&workspace()),
        "hackclub.enterprise.slack.com"
    );

    let plain = WorkspaceSession {
        enterprise_id: None,
        url: "https://echonet.slack.com/".into(),
        ..workspace()
    };
    assert_eq!(crate::slack::api_host(&plain), "echonet.slack.com");
}

#[test]
fn channel_resources_use_scoped_search_and_server_pagination() {
    let query = crate::files::FilesQuery {
        channel: Some("C1".into()),
        ..Default::default()
    };
    let request = files_search(&SlackClient::default(), &workspace(), &query, 2);
    let fields = form_fields(&request);
    assert!(fields.contains(&("query".into(), "in:<#C1>".into())));
    assert!(fields.contains(&("sort".into(), "score".into())));
    assert!(fields.contains(&("search_context".into(), "desktop_files_channel_tab".into())));
    assert!(fields.contains(&("page".into(), "2".into())));
    let request =
        channel_links_search(&SlackClient::default(), &workspace(), "C1", "  design  ", 3);
    let fields = form_fields(&request);
    assert!(request.url.contains("/api/conversations.searchLinks?"));
    assert!(fields.contains(&("channel_id".into(), "C1".into())));
    assert!(fields.contains(&("query".into(), "design".into())));
    assert!(fields.contains(&("page".into(), "3".into())));
}

#[test]
fn pins_requests_use_channel_and_message_timestamp() {
    let client = SlackClient::default();
    let workspace = workspace();
    let list = pins_list(&client, &workspace, "C_PRIVATE");
    assert!(list.url.contains("/api/pins.list?"));
    assert!(form_fields(&list).contains(&("channel".into(), "C_PRIVATE".into())));
    for (pinned, method) in [(true, "pins.add"), (false, "pins.remove")] {
        let request = pins_change(&client, &workspace, "C_PRIVATE", "123.000456", pinned);
        assert!(request.url.contains(&format!("/api/{method}?")));
        assert!(form_fields(&request).contains(&("channel".into(), "C_PRIVATE".into())));
        assert!(form_fields(&request).contains(&("timestamp".into(), "123.000456".into())));
        assert!(!request.redacted_debug().contains("xoxc-test-token"));
    }
}

#[test]
fn forwarding_keeps_source_destination_rich_note_and_client_id_distinct() {
    let client = SlackClient::default();
    let workspace = workspace();
    let blocks = vec![
        serde_json::json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Read this 🚀","style":{"bold":true}}]}]}),
    ];
    let args = ForwardMessage::new(
        "C_SOURCE".into(),
        "12.000345".into(),
        "C_DEST".into(),
        blocks.clone(),
    );
    let req = chat_share_message(&client, &workspace, &args);
    let fields = form_fields(&req);
    assert!(req.url.contains("/api/chat.shareMessage?"));
    for pair in [
        ("channel", "C_SOURCE"),
        ("share_channel", "C_DEST"),
        ("timestamp", "12.000345"),
        ("client_msg_id", &args.client_msg_id),
        ("text", ""),
    ] {
        assert!(fields.contains(&(pair.0.into(), pair.1.into())));
    }
    let body = fields.iter().find(|(key, _)| key == "blocks").unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<serde_json::Value>>(&body.1).unwrap(),
        blocks
    );
    assert!(!req.retry_safe());
    assert!(!req.redacted_debug().contains("xoxc-test-token"));
    let retry = chat_share_message(&client, &workspace, &args);
    assert_eq!(form_fields(&retry), fields);
    let result: ForwardedMessage =
        serde_json::from_value(serde_json::json!({"ok":true,"channel":"C_DEST","ts":"13.0"}))
            .unwrap();
    assert_eq!(result.channel, "C_DEST");
    assert_eq!(result.ts, "13.0");
}
#[test]
fn permalink_request_uses_message_timestamp_without_inventing_thread_links() {
    let req = chat_get_permalink(&SlackClient::default(), &workspace(), "C1", "23.000004");
    assert!(req.url.contains("/api/chat.getPermalink?"));
    assert!(form_fields(&req).contains(&("message_ts".into(), "23.000004".into())));
    assert!(form_fields(&req).contains(&("channel".into(), "C1".into())));
}

#[test]
fn enterprise_permalink_preserves_precision_and_thread_context() {
    let root = workspace_permalink(
        "https://hackclub.enterprise.slack.com/",
        "C123",
        "1791780358.080789",
        None,
    )
    .unwrap();
    assert_eq!(
        root,
        "https://hackclub.slack.com/archives/C123/p1791780358080789"
    );
    let reply = workspace_permalink(
        "https://hackclub.slack.com",
        "C123",
        "1791780400.000009",
        Some("1791780358.080789"),
    )
    .unwrap();
    assert_eq!(
        reply,
        "https://hackclub.slack.com/archives/C123/p1791780400000009?thread_ts=1791780358.080789&cid=C123"
    );
    for url in [
        "https://slack.com.evil.test",
        "https://hackclub.slack.com@evil.test",
        "http://hackclub.slack.com",
        "https://user@hackclub.slack.com",
    ] {
        assert!(workspace_permalink(url, "C123", "1.000001", None).is_none());
    }
    assert!(
        workspace_permalink("https://hackclub.slack.com", "C123/other", "1.000001", None).is_none()
    );
    assert!(workspace_permalink("https://hackclub.slack.com", "C123", "1.00001", None).is_none());
    assert!(
        workspace_permalink(
            "https://hackclub.slack.com",
            "C123",
            "1.000001",
            Some("bad")
        )
        .is_none()
    );
}

#[test]
fn scheduled_drafts_use_session_api_and_preserve_destination_and_files() {
    let client = SlackClient::default();
    let workspace = workspace();
    let mut message = ScheduleMessage::new(
        "Cprivate".into(),
        Some("123.456".into()),
        vec![serde_json::json!({"type":"rich_text","elements":[]})],
        1800000000,
    );
    message.file_ids = vec!["Fprivate".into()];
    let create = drafts_schedule(&client, &workspace, &message, None);
    assert!(create.url.contains("/drafts.create?"));
    assert!(!create.retry_safe());
    let fields = form_fields(&create);
    assert!(fields.contains(&("is_from_composer".into(), "true".into())));
    assert!(fields.contains(&("date_scheduled".into(), "1800000000".into())));
    let destinations: serde_json::Value =
        serde_json::from_str(&fields.iter().find(|(k, _)| k == "destinations").unwrap().1).unwrap();
    assert_eq!(destinations[0]["thread_ts"], "123.456");
    assert!(fields.contains(&("file_ids".into(), "[\"Fprivate\"]".into())));
    let update = drafts_schedule(&client, &workspace, &message, Some("Dr1"));
    assert!(update.url.contains("/drafts.update?"));
    assert!(!update.retry_safe());
    assert!(
        form_fields(&update)
            .iter()
            .any(|(k, v)| k == "client_last_updated_ts" && v.parse::<f64>().unwrap() > 0.0)
    );
    assert!(form_fields(&update).contains(&("draft_id".into(), "Dr1".into())));
    let list = drafts_list(&client, &workspace, 200);
    assert!(list.retry_safe());
    assert!(form_fields(&list).contains(&("is_active".into(), "true".into())));
    let delete = drafts_delete(&client, &workspace, "Dr1");
    assert!(!delete.retry_safe());
    assert!(form_fields(&delete).contains(&("skip_file_deletion".into(), "true".into())));
}

#[test]
fn draft_actions_preserve_content_and_atomically_retire_sent_drafts() {
    let client = SlackClient::default();
    let workspace = workspace();
    let draft:crate::scheduled::ScheduledMessage=serde_json::from_value(serde_json::json!({"id":"Dr1","client_msg_id":"stable-id","date_scheduled":1800000000,"blocks":[{"type":"rich_text","elements":[]}],"file_ids":["F1","F2"],"destinations":[{"channel_id":"C1","thread_ts":"100.001","broadcast":true}]})).unwrap();
    let cancel = drafts_unschedule(&client, &workspace, &draft);
    let fields = form_fields(&cancel);
    assert!(!cancel.retry_safe());
    assert!(fields.contains(&("date_scheduled".into(), "0".into())));
    assert!(fields.contains(&("file_ids".into(), "[\"F1\",\"F2\"]".into())));
    assert!(
        fields
            .iter()
            .any(|(k, v)| k == "blocks" && v.contains("rich_text"))
    );
    assert!(
        fields.iter().any(|(k, v)| k == "client_last_updated_ts"
            && v.parse::<u128>().unwrap() > 1_000_000_000_000)
    );
    let send = drafts_send(&client, &workspace, &draft).unwrap();
    let fields = form_fields(&send);
    assert!(send.url.contains("/files.share?"));
    assert!(!send.retry_safe());
    for (key, value) in [
        ("draft_id", "Dr1"),
        ("client_msg_id", "stable-id"),
        ("files", "F1,F2"),
        ("thread_ts", "100.001"),
        ("broadcast", "true"),
        ("skip_dlp_user_warning", "false"),
    ] {
        assert!(fields.contains(&(key.into(), value.into())));
    }
    assert!(!fields.iter().any(|(key, _)| key == "reply_broadcast"));
    let mut text = draft.clone();
    text.file_ids.clear();
    let send = drafts_send(&client, &workspace, &text).unwrap();
    assert!(send.url.contains("/chat.postMessage?"));
    assert!(form_fields(&send).contains(&("draft_id".into(), "Dr1".into())));
    assert!(form_fields(&send).contains(&("reply_broadcast".into(), "true".into())));
    assert!(!form_fields(&send).iter().any(|(key, _)| key == "broadcast"));
    text.destinations
        .push(serde_json::json!({"channel_id":"C2"}));
    assert!(drafts_send(&client, &workspace, &text).is_err());
    text.destinations.clear();
    assert!(drafts_send(&client, &workspace, &text).is_err());
}

#[test]
fn channel_creation_uses_real_workspace_memberships_not_org_or_external_channels() {
    let mut session = workspace();
    let user:User=serde_json::from_value(serde_json::json!({"id":"U1","profile":{"team":"E1"},"enterprise_user":{"teams":["T2","T1","T2","E1"]}})).unwrap();
    assert_eq!(channel_creation_team_ids(&session, &user), vec!["T1", "T2"]);
    assert!(channel_creation_team_ids(&session, &User::default()).is_empty());
    session.team_id = "T_CURRENT".into();
    assert_eq!(
        channel_creation_team_ids(&session, &user),
        vec!["T_CURRENT"]
    );
}
#[test]
fn channel_management_requests_preserve_visibility_targets_and_never_retry_writes() {
    let client = SlackClient::default();
    let session = workspace();
    for private in [true, false] {
        let request = conversation_create(&client, &session, "T1", "project", private);
        assert!(!request.retry_safe());
        for (key, value) in [
            ("team_id", "T1".to_owned()),
            ("name", "project".to_owned()),
            ("is_private", private.to_string()),
            ("validate_name", "true".to_owned()),
        ] {
            assert!(form_fields(&request).contains(&(key.into(), value)));
        }
    }
    for archive in [true, false] {
        let request = conversation_archive(&client, &session, "C1", archive);
        assert!(!request.retry_safe());
        assert!(request.url.contains(if archive {
            "/conversations.archive?"
        } else {
            "/conversations.unarchive?"
        }));
        assert!(form_fields(&request).contains(&("channel".into(), "C1".into())));
    }
    let request =
        conversation_set_text(&client, &session, "C1", ChannelTextField::Name, "new-name");
    assert!(!request.retry_safe());
    assert!(request.url.contains("/conversations.rename?"));
    assert!(form_fields(&request).contains(&("name".into(), "new-name".into())));
    let channel = Channel {
        name: Some("canonical-name".into()),
        ..Default::default()
    };
    assert_eq!(ChannelTextField::Name.value(&channel), "canonical-name");
}
#[test]
fn channel_names_preview_normalization_and_reject_empty_invalid_or_long_values() {
    assert_eq!(
        normalize_channel_name(" #Project.  Launch ").unwrap(),
        "project-launch"
    );
    assert_eq!(
        normalize_channel_name("Équipe_日本語").unwrap(),
        "équipe_日本語"
    );
    for invalid in ["", "  # ", "---", "team/other", "hi🚀"] {
        assert!(normalize_channel_name(invalid).is_err(), "{invalid}");
    }
    assert!(normalize_channel_name(&"é".repeat(80)).is_ok());
    assert!(normalize_channel_name(&"a".repeat(81)).is_err());
}

#[test]
fn thread_unread_request_keeps_the_selected_reply_timestamp() {
    let request = subscriptions_thread_mark_state(
        &SlackClient::default(),
        &workspace(),
        "C1".into(),
        "100.000000".into(),
        "101.123456".into(),
        false,
    );
    let fields = form_fields(&request);
    assert!(fields.contains(&("read".into(), "0".into())));
    assert!(fields.contains(&("ts".into(), "101.123456".into())));
    assert!(fields.contains(&("thread_ts".into(), "100.000000".into())));
}

#[test]
fn thread_subscription_changes_preserve_the_existing_read_cursor() {
    for active in [true, false] {
        let req = thread_subscription_request(
            &SlackClient::default(),
            &workspace(),
            "C1".into(),
            "10.0".into(),
            active,
            Some("20.0".into()),
        );
        assert!(req.url.contains(if active {
            "subscriptions.thread.add"
        } else {
            "subscriptions.thread.remove"
        }));
        let fields = form_fields(&req);
        assert!(fields.contains(&("channel".into(), "C1".into())));
        assert!(fields.contains(&("thread_ts".into(), "10.0".into())));
        assert!(fields.contains(&("last_read".into(), "20.0".into())));
        assert!(!fields.iter().any(|(k, _)| k == "read"));
    }
    let req = thread_subscription_request(
        &SlackClient::default(),
        &workspace(),
        "C1".into(),
        "10.0".into(),
        true,
        None,
    );
    assert!(!form_fields(&req).iter().any(|(k, _)| k == "last_read"));
}

#[test]
fn group_dm_request_preserves_recipient_order_and_requests_metadata() {
    let session = workspace();
    let recipients =
        crate::direct_messages::Recipients::new(&["U2".into(), "U1".into()], &session.user_id)
            .unwrap();
    let req = conversations_open_recipients(&SlackClient::default(), &session, &recipients);
    assert!(form_fields(&req).contains(&("users".into(), "U2,U1".into())));
    assert!(form_fields(&req).contains(&("return_im".into(), "true".into())));
    assert!(!req.retry_safe());
}

#[test]
fn canvas_rename_preserves_body_and_permissions_and_rejects_stale_names() {
    let c = SlackClient::default();
    let ws = workspace();
    let req = canvas_rename(&c, &ws, "F1", "  New name 🦊  ").unwrap();
    assert!(!req.retry_safe());
    let fields = form_fields(&req);
    assert!(fields.contains(&("file".into(), "F1".into())));
    assert!(fields.contains(&("title".into(), "New name 🦊".into())));
    assert!(
        fields
            .iter()
            .all(|(key, _)| matches!(key.as_str(), "file" | "title" | "token"))
    );
    assert!(canvas_rename(&c, &ws, "F1", " ").is_err());
    let mut file: super::super::models::File = serde_json::from_value(
        serde_json::json!({"id":"F1","title":"Original","filetype":"quip","editable":true}),
    )
    .unwrap();
    assert!(validate_canvas_rename(&file, "Original").is_ok());
    assert!(
        matches!(validate_canvas_rename(&file,"Stale"),Err(Error::Api(code)) if code=="canvas_title_changed")
    );
    file.extra
        .insert("editable".into(), serde_json::json!(false));
    assert!(
        matches!(validate_canvas_rename(&file,"Original"),Err(Error::Api(code)) if code=="canvas_not_editable")
    );
}

#[test]
fn canvas_creation_is_standalone_nonretrying_and_preserves_markdown() {
    let c = SlackClient::default();
    let ws = workspace();
    let body = "## Notes 🦊\n\n- [ ] First task\n- [x] Done\n";
    let request = canvas_create(&c, &ws, "  Project 日本語  ", body).unwrap();
    assert!(!request.retry_safe());
    let fields = form_fields(&request);
    assert!(fields.contains(&("title".into(), "Project 日本語".into())));
    assert!(fields.contains(&("markdown".into(), body.into())));
    assert!(
        fields
            .iter()
            .all(|(key, _)| matches!(key.as_str(), "title" | "markdown" | "token"))
    );
    assert!(canvas_create(&c, &ws, " ", body).is_err());
    assert!(
        canvas_create(
            &c,
            &ws,
            "Title",
            &"x".repeat(crate::canvas::MAX_CANVAS_BYTES + 1)
        )
        .is_err()
    );
    assert_eq!(
        created_canvas_id(&serde_json::json!({"file_id":"F123ABC"})).unwrap(),
        "F123ABC"
    );
    for invalid in [
        serde_json::json!({}),
        serde_json::json!({"file_id":""}),
        serde_json::json!({"file_id":"https://example.com"}),
    ] {
        assert!(created_canvas_id(&invalid).is_err());
    }
}

#[test]
fn bookmark_desktop_requests_preserve_metadata_and_never_retry_mutations() {
    let c = SlackClient::default();
    let w = workspace();
    let list = bookmarks_list(&c, &w, "C1");
    assert!(list.retry_safe());
    let fields = form_fields(&list);
    assert!(fields.contains(&("channel".into(), "C1".into())));
    for id in [None, Some("Bk1")] {
        let request =
            bookmark_save(&c, &w, "C1", id, " Docs 🦊 ", " https://example.com ").unwrap();
        assert!(!request.retry_safe());
        let fields = form_fields(&request);
        assert!(fields.contains(&("title".into(), "Docs 🦊".into())));
        assert!(fields.contains(&("link".into(), "https://example.com".into())));
        assert_eq!(
            fields.iter().any(|(key, _)| key == "bookmark"),
            id.is_some()
        );
        assert!(!fields.iter().any(|(key, _)| matches!(
            key.as_str(),
            "parent_id" | "emoji" | "icon" | "access_level"
        )));
    }
    assert!(!bookmark_remove(&c, &w, "C1", "Bk1").retry_safe());
    assert!(bookmark_save(&c, &w, "C1", None, " ", "https://example.com").is_err());
    assert!(bookmark_save(&c, &w, "C1", None, "X", "javascript:alert(1)").is_err());
}

#[test]
fn bookmark_folder_and_order_requests_use_verified_types_and_omit_root_parent() {
    let c = SlackClient::default();
    let w = workspace();
    let folder: crate::bookmarks::Bookmark =
        serde_json::from_value(serde_json::json!({"id":"BkF","type":"local_folder"})).unwrap();
    let save = bookmark_folder_save(&c, &w, "C", Some("BkF"), " Folder 🦊 ").unwrap();
    let fields = form_fields(&save);
    assert!(fields.contains(&("type".into(), "local_folder".into())));
    assert!(!fields.iter().any(|(k, _)| k == "link"));
    assert!(!save.retry_safe());
    assert!(bookmark_folder_save(&c, &w, "C", None, " ").is_err());
    assert!(!bookmark_folder_remove(&c, &w, "C", "BkF").retry_safe());
    let root = crate::bookmarks::Placement {
        parent: None,
        left: None,
        right: Some("BkNext".into()),
    };
    let req = bookmark_reorder(&c, &w, "C", &folder, &root).unwrap();
    let fields = form_fields(&req);
    assert!(!req.retry_safe());
    assert!(
        !fields
            .iter()
            .any(|(k, _)| matches!(k.as_str(), "parent_id" | "left"))
    );
    assert!(fields.contains(&("bookmark_type".into(), "local_folder".into())));
    assert!(fields.contains(&("right".into(), "BkNext".into())));
}

#[test]
fn file_actions_address_only_the_requested_file_and_never_retry_delete() {
    let c = SlackClient::default();
    let w = workspace();
    let info = file_info(&c, &w, "F1");
    let delete = file_delete(&c, &w, "F1");
    let rename = file_rename(&c, &w, "F1", "  Notes · 日本語  ").unwrap();
    assert!(rename.url.contains("/api/files.edit?"));
    assert!(!rename.retry_safe());
    assert_eq!(
        form_fields(&rename)
            .iter()
            .filter(|(k, _)| k != "token")
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            ("file".into(), "F1".into()),
            ("title".into(), "Notes · 日本語".into())
        ]
    );
    assert!(file_rename(&c, &w, "", "Name").is_err());
    assert!(file_rename(&c, &w, "F1", " \n ").is_err());
    assert!(info.url.contains("/api/files.info?"));
    assert!(info.retry_safe());
    assert!(delete.url.contains("/api/files.delete?"));
    assert!(!delete.retry_safe());
    assert_eq!(
        form_fields(&delete)
            .iter()
            .filter(|(k, _)| k != "token")
            .cloned()
            .collect::<Vec<_>>(),
        vec![("file".into(), "F1".into())]
    );
}
