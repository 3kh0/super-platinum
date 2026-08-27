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
            query: "deploy failed".into(),
            count: 20,
            page: 2,
        },
    );
    let fields = form_fields(&request);

    assert!(request.url.contains("/api/search.modules.messages?"));
    assert!(fields.contains(&("module".into(), "messages".into())));
    assert!(fields.contains(&("query".into(), "deploy failed".into())));
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
    assert!(fields.contains(&("max_ts".into(), "1783372400.222222".into())));

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
