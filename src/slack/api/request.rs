use super::*;

pub fn user_boot(client: &SlackClient, workspace: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(
        workspace,
        "client.userBoot",
        vec![("include_min_version_bump_check", "1".to_owned())],
    )
}

#[derive(Debug, Clone, Default)]
pub struct HistoryArgs {
    pub channel: ChannelId,
    pub cursor: Option<String>,
    pub latest: Option<MessageTs>,
    pub oldest: Option<MessageTs>,
    pub limit: Option<u32>,
    pub inclusive: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RepliesArgs {
    pub channel: ChannelId,
    pub ts: MessageTs,
    pub cursor: Option<String>,
    pub latest: Option<MessageTs>,
    pub oldest: Option<MessageTs>,
    pub limit: Option<u32>,
    pub inclusive: bool,
}

pub fn conversations_history(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    args: HistoryArgs,
) -> PreparedRequest {
    let mut fields = vec![("channel", args.channel)];
    push_opt(&mut fields, "cursor", args.cursor);
    push_opt(&mut fields, "latest", args.latest);
    push_opt(&mut fields, "oldest", args.oldest);
    push_opt(
        &mut fields,
        "limit",
        args.limit.map(|limit| limit.to_string()),
    );
    if args.inclusive {
        fields.push(("inclusive", "true".to_owned()));
    }

    client.rest_form(workspace, "conversations.history", fields)
}

pub fn conversations_replies(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    args: RepliesArgs,
) -> PreparedRequest {
    let mut fields = vec![("channel", args.channel), ("ts", args.ts)];
    push_opt(&mut fields, "cursor", args.cursor);
    push_opt(&mut fields, "latest", args.latest);
    push_opt(&mut fields, "oldest", args.oldest);
    push_opt(
        &mut fields,
        "limit",
        args.limit.map(|limit| limit.to_string()),
    );
    if args.inclusive {
        fields.push(("inclusive", "true".to_owned()));
    }
    client.rest_form(workspace, "conversations.replies", fields)
}

pub fn conversations_mark(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    ts: MessageTs,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "conversations.mark",
        vec![("channel", channel), ("ts", ts)],
    )
}

pub fn subscriptions_thread_mark(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    root_ts: MessageTs,
    ts: MessageTs,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "subscriptions.thread.mark",
        vec![
            ("channel", channel),
            ("thread_ts", root_ts),
            ("ts", ts),
            ("read", "1".to_owned()),
        ],
    )
}

pub fn subscriptions_thread_get_view(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    limit: u32,
    max_ts: Option<MessageTs>,
    vip_only: bool,
) -> PreparedRequest {
    let mut fields = vec![
        ("limit", limit.to_string()),
        ("fetch_threads_state", "true".to_owned()),
        (
            "priority_mode",
            if vip_only { "priority" } else { "all" }.to_owned(),
        ),
    ];
    push_opt(&mut fields, "max_ts", max_ts);
    client.rest_form(workspace, "subscriptions.thread.getView", fields)
}

pub fn conversations_info(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "conversations.info",
        vec![
            ("channel", channel),
            ("include_num_members", "false".to_owned()),
        ],
    )
}

pub fn client_counts(client: &SlackClient, workspace: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(workspace, "client.counts", Vec::new())
}

pub fn channel_sections(client: &SlackClient, workspace: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(workspace, "users.channelSections.list", Vec::new())
}

const ACTIVITY_TYPES: &str = "at_user,at_user_group,at_channel,at_everyone,keyword,\
list_record_assigned,list_user_mentioned,list_todo_notification,list_approval_request,\
list_approval_reviewed,unjoined_channel_mention,thread_v2,message_reaction,bot_dm_bundle,dm,\
internal_channel_invite,external_channel_invite,external_dm_invite,channel,saved_reminder,\
list_record_edited";

pub fn messages_list(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    message_ids: &[(ChannelId, Vec<MessageTs>)],
) -> PreparedRequest {
    let ids: Vec<_> = message_ids
        .iter()
        .map(|(channel, timestamps)| {
            serde_json::json!({ "channel": channel, "timestamps": timestamps })
        })
        .collect();
    let payload = serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_owned());
    client.rest_form(
        workspace,
        "messages.list",
        vec![
            ("message_ids", payload),
            ("org_wide_aware", "true".to_owned()),
            ("cached_latest_updates", "{}".to_owned()),
        ],
    )
}

pub fn activity_feed(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    limit: u32,
    cursor: Option<String>,
    unread_only: bool,
) -> PreparedRequest {
    let mut fields = vec![
        ("limit", limit.to_string()),
        ("types", ACTIVITY_TYPES.to_owned()),
        ("mode", "chrono_v1".to_owned()),
    ];
    push_opt(&mut fields, "cursor", cursor);
    fields.extend([
        ("archive_only", "false".to_owned()),
        ("unread_only", unread_only.to_string()),
        ("priority_only", "false".to_owned()),
        ("only_salesforce_channels", "false".to_owned()),
        ("exclude_automations", "false".to_owned()),
    ]);
    client.rest_form(workspace, "activity.feed", fields)
}

pub fn sidebar_dms(client: &SlackClient, workspace: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(workspace, "sidebar.dms", Vec::new())
}

pub fn client_dms(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    count: u32,
    cursor: Option<String>,
) -> PreparedRequest {
    let mut fields = vec![
        ("count", count.to_string()),
        ("include_closed", "true".to_owned()),
        ("include_channel", "true".to_owned()),
        ("exclude_bots", "true".to_owned()),
        ("priority_mode", "priority".to_owned()),
    ];
    push_opt(&mut fields, "cursor", cursor);
    client.rest_form(workspace, "client.dms", fields)
}

pub fn users_set_presence(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    presence: String,
) -> PreparedRequest {
    client.rest_form(workspace, "users.setPresence", vec![("presence", presence)])
}

pub fn users_profile_get(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    user: UserId,
) -> PreparedRequest {
    client.rest_form(workspace, "users.profile.get", vec![("user", user)])
}

pub fn users_profile_get_extras(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    user: UserId,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "users.profile.getExtras",
        vec![("user", user), ("keys", "im_mpim_ids".to_owned())],
    )
}

pub fn team_profile_get(client: &SlackClient, workspace: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(workspace, "team.profile.get", Vec::new())
}

pub fn conversations_open(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    user: UserId,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "conversations.open",
        vec![("users", user), ("return_im", "true".to_owned())],
    )
}

pub fn chat_post_message(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    text: String,
    thread_ts: Option<MessageTs>,
) -> PreparedRequest {
    let mut fields = vec![("channel", channel), ("text", text)];
    push_opt(&mut fields, "thread_ts", thread_ts);
    client.rest_form(workspace, "chat.postMessage", fields)
}

#[derive(Debug, Clone)]
pub struct SearchArgs {
    pub query: String,
    pub count: u32,
    pub page: u32,
}

impl SearchArgs {
    pub fn new(query: impl Into<String>) -> Self {
        SearchArgs {
            query: query.into(),
            count: 20,
            page: 1,
        }
    }
}

pub fn search_messages(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    args: SearchArgs,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "search.modules.messages",
        vec![
            ("module", "messages".to_owned()),
            ("query", args.query),
            ("count", args.count.to_string()),
            ("page", args.page.to_string()),
            ("sort", "timestamp".to_owned()),
            ("sort_dir", "desc".to_owned()),
            ("highlight", "true".to_owned()),
            ("extracts", "true".to_owned()),
            ("extra_message_data", "true".to_owned()),
            ("client_req_id", uuid::Uuid::new_v4().to_string()),
            ("search_session_id", uuid::Uuid::new_v4().to_string()),
        ],
    )
}

#[derive(Debug, Clone)]
pub struct SearchInlineArgs {
    pub query: String,
    pub channel: ChannelId,
    pub count: u32,
    pub page: u32,
}

impl SearchInlineArgs {
    pub fn new(query: impl Into<String>, channel: ChannelId) -> Self {
        SearchInlineArgs {
            query: query.into(),
            channel,
            count: 20,
            page: 1,
        }
    }
}

pub fn search_inline(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    args: SearchInlineArgs,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "search.inline",
        vec![
            ("query", args.query),
            ("channel", args.channel),
            ("count", args.count.to_string()),
            ("page", args.page.to_string()),
            ("sort", "timestamp".to_owned()),
            ("sort_dir", "desc".to_owned()),
            ("highlight", "true".to_owned()),
            ("extracts", "true".to_owned()),
            ("client_req_id", uuid::Uuid::new_v4().to_string()),
            ("search_session_id", uuid::Uuid::new_v4().to_string()),
        ],
    )
}

pub fn chat_update(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    ts: MessageTs,
    text: String,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "chat.update",
        vec![("channel", channel), ("ts", ts), ("text", text)],
    )
}

pub fn chat_delete(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    ts: MessageTs,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "chat.delete",
        vec![("channel", channel), ("ts", ts)],
    )
}

pub fn reactions_add(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    timestamp: MessageTs,
    name: String,
) -> PreparedRequest {
    reaction(client, workspace, "reactions.add", channel, timestamp, name)
}

pub fn reactions_remove(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    timestamp: MessageTs,
    name: String,
) -> PreparedRequest {
    reaction(
        client,
        workspace,
        "reactions.remove",
        channel,
        timestamp,
        name,
    )
}

fn reaction(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    endpoint: &str,
    channel: ChannelId,
    timestamp: MessageTs,
    name: String,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        endpoint,
        vec![
            ("channel", channel),
            ("timestamp", timestamp),
            ("name", name),
        ],
    )
}

fn push_opt(fields: &mut Vec<(&str, String)>, key: &'static str, value: Option<String>) {
    if let Some(value) = value {
        fields.push((key, value));
    }
}

pub(super) fn decode<T: serde::de::DeserializeOwned>(
    value: serde_json::Value,
    what: &str,
) -> Result<T, Error> {
    serde_json::from_value(value).map_err(|e| Error::Transport(format!("decode {what}: {e}")))
}
