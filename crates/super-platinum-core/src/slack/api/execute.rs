use super::*;

pub async fn fetch_usergroups_info(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    ids: Vec<String>,
) -> Result<Vec<super::super::models::UserGroup>, Error> {
    let request = client
        .edge_json(
            workspace,
            "usergroups/info",
            serde_json::json!({"ids": ids}),
        )
        .map_err(|e| Error::Transport(e.to_string()))?;
    let page: super::super::models::UserGroupsPage =
        decode(transport.execute(request).await?, "usergroups/info")?;
    Ok(page.results)
}

pub async fn fetch_usergroup_members(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    group: &super::super::models::UserGroup,
) -> Result<Vec<String>, Error> {
    #[derive(serde::Deserialize)]
    struct Members {
        users: Vec<String>,
    }
    let request = client.rest_form(
        workspace,
        "usergroups.users.list",
        vec![
            ("usergroup", group.id.clone()),
            ("team_id", group.team_id.clone()),
        ],
    );
    let page: Members = decode(transport.execute(request).await?, "usergroups.users.list")?;
    Ok(page.users)
}

pub async fn fetch_user_boot(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
) -> Result<BootData, Error> {
    let value = transport.execute(user_boot(client, workspace)).await?;
    decode(value, "client.userBoot")
}

pub async fn fetch_history(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    args: HistoryArgs,
) -> Result<HistoryPage, Error> {
    fetch_history_with(client, workspace, args, |request| {
        transport.execute(request)
    })
    .await
}

pub async fn fetch_replies(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    args: RepliesArgs,
) -> Result<HistoryPage, Error> {
    fetch_replies_with(client, workspace, args, |request| {
        transport.execute(request)
    })
    .await
}

pub async fn mark_channel(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    ts: MessageTs,
) -> Result<(), Error> {
    transport
        .execute(conversations_mark(client, workspace, channel, ts))
        .await?;
    Ok(())
}

pub async fn mark_activity_read(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    kind: String,
    feed_ts: MessageTs,
    key: String,
) -> Result<(), Error> {
    transport
        .execute(activity_mark_read(client, workspace, kind, feed_ts, key))
        .await?;
    Ok(())
}

pub async fn mark_thread(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    root_ts: MessageTs,
    ts: MessageTs,
) -> Result<(), Error> {
    set_thread_read(transport, client, workspace, channel, root_ts, ts, true).await
}

pub async fn set_thread_read(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    root_ts: MessageTs,
    ts: MessageTs,
    read: bool,
) -> Result<(), Error> {
    transport
        .execute(subscriptions_thread_mark_state(
            client, workspace, channel, root_ts, ts, read,
        ))
        .await?;
    Ok(())
}

pub async fn fetch_threads_view(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    limit: u32,
    current_ts: Option<MessageTs>,
    vip_only: bool,
) -> Result<ThreadsViewPage, Error> {
    fetch_threads_view_with(client, workspace, limit, current_ts, vip_only, |request| {
        transport.execute(request)
    })
    .await
}

pub async fn fetch_counts(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
) -> Result<CountsPage, Error> {
    fetch_counts_with(client, workspace, |request| transport.execute(request)).await
}

pub async fn fetch_generic_info(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    updated_channels: &BTreeMap<ChannelId, u64>,
) -> Result<GenericInfoPage, Error> {
    let value = transport
        .execute(conversations_generic_info(
            client,
            workspace,
            updated_channels,
        ))
        .await?;
    decode(value, "conversations.genericInfo")
}

pub async fn fetch_channel_sections(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
) -> Result<ChannelSectionsPage, Error> {
    let value = transport
        .execute(channel_sections(client, workspace))
        .await?;
    decode(value, "users.channelSections.list")
}

pub async fn fetch_sidebar_dms(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
) -> Result<SidebarDmsPage, Error> {
    let value = transport.execute(sidebar_dms(client, workspace)).await?;
    decode(value, "sidebar.dms")
}

pub async fn fetch_client_dms(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    count: u32,
    cursor: Option<String>,
) -> Result<ClientDmsPage, Error> {
    let value = transport
        .execute(client_dms(client, workspace, count, cursor))
        .await?;
    decode(value, "client.dms")
}

pub async fn fetch_activity_feed(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    limit: u32,
    cursor: Option<String>,
    unread_only: bool,
) -> Result<ActivityFeedPage, Error> {
    fetch_activity_feed_with(client, workspace, limit, cursor, unread_only, |request| {
        transport.execute(request)
    })
    .await
}

pub async fn fetch_messages_list(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    message_ids: Vec<(ChannelId, Vec<MessageTs>)>,
) -> Result<MessagesListPage, Error> {
    let value = transport
        .execute(messages_list(client, workspace, &message_ids))
        .await?;
    decode(value, "messages.list")
}

pub async fn fetch_dnd_info(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
) -> Result<DndInfo, Error> {
    let value = transport.execute(dnd_info(client, workspace)).await?;
    decode(value, "dnd.info")
}

pub async fn set_presence(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    presence: String,
) -> Result<(), Error> {
    transport
        .execute(users_set_presence(client, workspace, presence))
        .await?;
    Ok(())
}

pub async fn set_snooze(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    minutes: u32,
) -> Result<DndInfo, Error> {
    let value = transport
        .execute(dnd_set_snooze(client, workspace, minutes))
        .await?;
    decode(value, "dnd.setSnooze")
}

pub async fn end_snooze(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
) -> Result<DndInfo, Error> {
    let value = transport.execute(dnd_end_snooze(client, workspace)).await?;
    decode(value, "dnd.endSnooze")
}

pub async fn set_user_profile(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    profile: String,
) -> Result<UserProfile, Error> {
    let value = transport
        .execute(users_profile_set(client, workspace, profile))
        .await?;
    let page: UserProfilePage = decode(value, "users.profile.set")?;
    Ok(page.profile)
}

pub async fn fetch_users_info(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    user_ids: Vec<String>,
) -> Result<Vec<User>, Error> {
    let request = super::super::edge::users_info(client, workspace, &user_ids)
        .map_err(|e| Error::Transport(format!("build users/info: {e}")))?;
    let value = transport.execute(request).await?;
    let page: EdgeResults<User> = decode(value, "users/info")?;
    Ok(page.results)
}

pub async fn fetch_conversation_teams(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
) -> Result<Vec<Team>, Error> {
    let value = transport
        .execute(conversations_team_connections(client, workspace, channel))
        .await?;
    let page: ChannelTeamConnectionsPage = decode(value, "conversations.teamConnections")?;
    Ok(page
        .connections
        .into_iter()
        .map(|connection| connection.team)
        .collect())
}

pub async fn fetch_user_profile(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    user: UserId,
) -> Result<UserProfile, Error> {
    let value = transport
        .execute(users_profile_get(client, workspace, user))
        .await?;
    let page: UserProfilePage = decode(value, "users.profile.get")?;
    Ok(page.profile)
}

pub async fn fetch_user_profile_extras(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    user: UserId,
) -> Result<ProfileExtrasPage, Error> {
    let value = transport
        .execute(users_profile_get_extras(client, workspace, user))
        .await?;
    decode(value, "users.profile.getExtras")
}

pub async fn fetch_team_profile_fields(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
) -> Result<Vec<TeamProfileField>, Error> {
    let value = transport
        .execute(team_profile_get(client, workspace))
        .await?;
    let page: TeamProfilePage = decode(value, "team.profile.get")?;
    Ok(page.profile.fields)
}

pub async fn add_priority_user(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    user: UserId,
) -> Result<(), Error> {
    transport
        .execute(users_priority_add(client, workspace, user))
        .await?;
    Ok(())
}

pub async fn remove_priority_user(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    user: UserId,
) -> Result<(), Error> {
    transport
        .execute(users_priority_remove(client, workspace, user))
        .await?;
    Ok(())
}

pub async fn fetch_channels_info(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel_ids: Vec<ChannelId>,
) -> Result<Vec<Channel>, Error> {
    let request = super::super::edge::channels_info(client, workspace, &channel_ids)
        .map_err(|e| Error::Transport(format!("build channels/info: {e}")))?;
    let mut channels = match transport.execute(request).await {
        Ok(value) => {
            let page: EdgeResults<Channel> = decode(value, "channels/info")?;
            page.results
        }
        Err(e) => {
            tracing::debug!(error = %e, "edge channels/info failed; falling back to conversations.info");
            Vec::new()
        }
    };

    let found = channels
        .iter()
        .map(|channel| channel.id.clone())
        .collect::<std::collections::HashSet<_>>();
    for channel_id in channel_ids
        .into_iter()
        .filter(|channel_id| !found.contains(channel_id))
    {
        match fetch_conversation_info(transport, client, workspace, channel_id.clone()).await {
            Ok(channel) => channels.push(channel),
            Err(e) => {
                tracing::debug!(channel = %channel_id, error = %e, "conversations.info fallback failed")
            }
        }
    }

    Ok(channels)
}

/// The complete workspace catalog, including aliases. Load on demand, not boot.
pub async fn fetch_emoji_catalog(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
) -> Result<Vec<Emoji>, Error> {
    #[derive(serde::Deserialize)]
    struct Catalog {
        emoji: BTreeMap<String, String>,
    }
    let request = client.rest_form(workspace, "emoji.list", Vec::new());
    let page: Catalog = decode(transport.execute(request).await?, "emoji.list")?;
    Ok(page
        .emoji
        .into_iter()
        .map(|(name, value)| Emoji {
            name,
            value,
            ..Default::default()
        })
        .collect())
}

pub async fn fetch_emojis_info(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    names: Vec<String>,
) -> Result<Vec<Emoji>, Error> {
    let request = super::super::edge::emojis_info(client, workspace, &names)
        .map_err(|e| Error::Transport(format!("build emojis/info: {e}")))?;
    let value = transport.execute(request).await?;
    let page: EdgeResults<Emoji> = decode(value, "emojis/info")?;
    Ok(page.results)
}

pub async fn fetch_conversation_info(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel_id: ChannelId,
) -> Result<Channel, Error> {
    #[derive(serde::Deserialize)]
    struct ConversationInfoPage {
        channel: Channel,
    }

    let value = transport
        .execute(conversations_info(client, workspace, channel_id))
        .await?;
    let page: ConversationInfoPage = decode(value, "conversations.info")?;
    Ok(page.channel)
}

pub async fn send_message(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    text: String,
    thread_ts: Option<MessageTs>,
) -> Result<SentMessage, Error> {
    let value = transport
        .execute(chat_post_message(
            client, workspace, channel, text, thread_ts,
        ))
        .await?;
    decode_sent_message(value)
}

/// Legacy syntax still carries the same immutable message identity and thread intent.
#[allow(clippy::too_many_arguments)]
pub async fn send_message_with_broadcast(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    text: String,
    thread_ts: Option<MessageTs>,
    client_msg_id: String,
    reply_broadcast: bool,
) -> Result<SentMessage, Error> {
    if client_msg_id.is_empty() || (reply_broadcast && thread_ts.is_none()) {
        return Err(Error::Api("invalid_arguments".into()));
    }
    let mut request = chat_post_message(client, workspace, channel, text, thread_ts);
    if let super::super::client::RequestBody::Form(fields) = &mut request.body {
        fields.push(("client_msg_id".into(), client_msg_id));
        fields.push(("reply_broadcast".into(), reply_broadcast.to_string()));
    }
    decode_sent_message(transport.execute_draft_mutation(request).await?)
}

fn decode_sent_message(value: serde_json::Value) -> Result<SentMessage, Error> {
    if value.get("ok").and_then(serde_json::Value::as_bool) == Some(false)
        && let Some(code) = value.get("error").and_then(serde_json::Value::as_str)
    {
        return Err(Error::Api(code.to_owned()));
    }
    if value.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        return Err(Error::Transport(
            "Slack did not explicitly confirm the sent message".into(),
        ));
    }
    decode(value, "chat.postMessage")
}

#[cfg(test)]
#[path = "execute_send_tests.rs"]
mod send_tests;

#[path = "execute_upload_file.rs"]
mod upload_file;
pub use upload_file::UploadFile;
use upload_file::stage_files;

/// What to upload and where, separate from the connection context that every
/// call in this module already threads through.
pub struct UploadRequest {
    pub reply_broadcast: bool,
    pub client_msg_id: String,
    pub channel: ChannelId,
    pub thread_ts: Option<MessageTs>,
    pub initial_comment: String,
    /// Local bytes, user-visible filename, and transfer progress stay distinct.
    pub files: Vec<UploadFile>,
    pub canceled: Arc<AtomicBool>,
}

/// Uploading bytes is not message delivery. Only the final completion call can
/// publish the caption/files into the destination conversation.
#[derive(Debug, thiserror::Error)]
pub enum UploadMessageError {
    #[error("upload stopped before message delivery: {0}")]
    BeforeDelivery(Error),
    #[error("upload message completion failed: {0}")]
    Completion(Error),
    #[error("upload sharing response uncertain: {error}")]
    CompletionWithFiles {
        error: Error,
        finalized: CompletedUpload,
    },
}
impl UploadMessageError {
    pub fn error(&self) -> &Error {
        match self {
            Self::BeforeDelivery(error)
            | Self::Completion(error)
            | Self::CompletionWithFiles { error, .. } => error,
        }
    }
    /// Validated private finalization identities, not proof of message delivery.
    /// These permit an exact read-only reconciliation after a lost share ACK.
    pub fn finalized_files(&self) -> Option<&[String]> {
        match self {
            Self::CompletionWithFiles { finalized, .. } if self.delivery_uncertain() => {
                Some(&finalized.file_ids)
            }
            _ => None,
        }
    }
    fn retain_finalized(self, finalized: &CompletedUpload) -> Self {
        if self.delivery_uncertain() {
            Self::CompletionWithFiles {
                error: self.error().clone(),
                finalized: finalized.clone(),
            }
        } else {
            self
        }
    }
    pub fn delivery_uncertain(&self) -> bool {
        match self {
            Self::BeforeDelivery(_) => false,
            Self::Completion(error) | Self::CompletionWithFiles { error, .. } => {
                matches!(error, Error::Offline(_) | Error::Transport(_))
                    || matches!(error, Error::Api(code) if matches!(code.as_str(), "internal_error" | "fatal_error"))
                    || matches!(error, Error::HttpStatus { status, .. } if *status >= 500 || *status == 408)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedUpload {
    pub file_ids: Vec<String>,
}

fn upload_file_ids(values: &[serde_json::Value]) -> Option<std::collections::BTreeSet<String>> {
    let ids = values
        .iter()
        .map(|file| {
            file.get("id")?
                .as_str()
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
        })
        .collect::<Option<std::collections::BTreeSet<_>>>()?;
    (ids.len() == values.len() && !ids.is_empty()).then_some(ids)
}

fn completed_upload(
    value: &serde_json::Value,
    staged: &[serde_json::Value],
) -> Result<CompletedUpload, UploadMessageError> {
    let expected = upload_file_ids(staged);
    let confirmed = value
        .get("files")
        .and_then(serde_json::Value::as_array)
        .and_then(|files| upload_file_ids(files));
    if value.get("ok").and_then(serde_json::Value::as_bool) != Some(true)
        || expected.is_none()
        || confirmed != expected
    {
        return Err(UploadMessageError::Completion(Error::Transport(
            "Slack did not confirm the exact uploaded files".into(),
        )));
    }
    Ok(CompletedUpload {
        file_ids: expected.unwrap().into_iter().collect(),
    })
}

pub async fn upload_files(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    request: UploadRequest,
) -> Result<CompletedUpload, UploadMessageError> {
    upload_files_with_receipt(transport, client, workspace, request, |_| async { Ok(()) }).await
}

/// Await durable receipt storage before any operation capable of delivery.
/// The callback receives staged identities only, not proof of finalization or
/// delivery. Returning an error prevents both completion and sharing. Owners
/// can invalidate the request through its cancel flag while the callback awaits.
pub async fn upload_files_with_receipt<F, Fut>(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    request: UploadRequest,
    receipt: F,
) -> Result<CompletedUpload, UploadMessageError>
where
    F: FnOnce(CompletedUpload) -> Fut,
    Fut: std::future::Future<Output = Result<(), Error>>,
{
    let UploadRequest {
        reply_broadcast,
        client_msg_id,
        channel,
        thread_ts,
        initial_comment,
        files: files_to_upload,
        canceled,
    } = request;

    if reply_broadcast
        && (thread_ts.as_deref().is_none_or(str::is_empty) || client_msg_id.is_empty())
    {
        return Err(UploadMessageError::BeforeDelivery(Error::Api(
            "invalid_arguments".into(),
        )));
    }
    let files = stage_files(transport, client, workspace, files_to_upload, &canceled)
        .await
        .map_err(UploadMessageError::BeforeDelivery)?;
    let staged = CompletedUpload {
        file_ids: upload_file_ids(&files)
            .ok_or_else(|| {
                UploadMessageError::BeforeDelivery(Error::Transport(
                    "Slack did not provide unique nonempty staged file identities".into(),
                ))
            })?
            .into_iter()
            .collect(),
    };
    if canceled.load(Ordering::Relaxed) {
        return Err(UploadMessageError::BeforeDelivery(Error::UploadCanceled));
    }
    receipt(staged)
        .await
        .map_err(UploadMessageError::BeforeDelivery)?;
    if canceled.load(Ordering::Relaxed) {
        return Err(UploadMessageError::BeforeDelivery(Error::UploadCanceled));
    }
    if reply_broadcast {
        return upload_broadcast::complete_and_share(
            transport,
            client,
            workspace,
            &channel,
            thread_ts.as_deref().unwrap(),
            &initial_comment,
            &client_msg_id,
            &files,
            &canceled,
        )
        .await;
    }

    let mut fields = vec![
        (
            "files",
            serde_json::to_string(&files).map_err(|error| {
                UploadMessageError::BeforeDelivery(Error::Transport(format!(
                    "encode upload files: {error}"
                )))
            })?,
        ),
        ("channel_id", channel),
    ];
    if !initial_comment.is_empty() {
        fields.push(("initial_comment", initial_comment));
    }
    if let Some(thread_ts) = thread_ts {
        fields.push(("thread_ts", thread_ts));
    }
    if canceled.load(Ordering::Relaxed) {
        return Err(UploadMessageError::BeforeDelivery(Error::UploadCanceled));
    }
    let value = transport
        .execute(client.rest_form(workspace, "files.completeUploadExternal", fields))
        .await
        .map_err(UploadMessageError::Completion)?;
    completed_upload(&value, &files)
}

#[cfg(test)]
#[path = "execute_upload_tests.rs"]
mod upload_tests;

#[path = "execute_upload_broadcast.rs"]
mod upload_broadcast;

/// Upload and finalize private files without posting to a conversation.
/// Scheduling attaches these ids later, so no early file message is created.
pub async fn upload_unshared_files(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    files_to_upload: Vec<UploadFile>,
    canceled: Arc<AtomicBool>,
) -> Result<Vec<String>, Error> {
    let files = stage_files(transport, client, workspace, files_to_upload, &canceled).await?;
    if canceled.load(Ordering::Relaxed) {
        return Err(Error::UploadCanceled);
    }
    transport
        .execute(client.rest_form(
            workspace,
            "files.completeUploadExternal",
            vec![("files", serde_json::to_string(&files).unwrap())],
        ))
        .await?;
    Ok(files
        .iter()
        .filter_map(|f| f.get("id").and_then(|s| s.as_str()).map(str::to_owned))
        .collect())
}

pub async fn fetch_users_search(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    query: String,
) -> Result<Vec<User>, Error> {
    let request = super::super::edge::users_search(client, workspace, &query, 30)
        .map_err(|e| Error::Transport(format!("build users/search: {e}")))?;
    let value = transport.execute(request).await?;
    let page: EdgeResults<User> = decode(value, "users/search")?;
    Ok(page.results)
}

pub async fn fetch_channels_search(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    query: String,
) -> Result<Vec<Channel>, Error> {
    let request = super::super::edge::channels_search(client, workspace, &query, 30)
        .map_err(|e| Error::Transport(format!("build channels/search: {e}")))?;
    let value = transport.execute(request).await?;
    let page: EdgeResults<Channel> = decode(value, "channels/search")?;
    Ok(page.results)
}

pub async fn open_dm(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    user: UserId,
) -> Result<ChannelId, Error> {
    let value = transport
        .execute(conversations_open(client, workspace, user))
        .await?;
    let opened: OpenedConversation = decode(value, "conversations.open")?;
    Ok(opened.channel.id)
}

pub async fn open_direct_conversation(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    recipients: &crate::direct_messages::Recipients,
) -> Result<Channel, Error> {
    let value = transport
        .execute(conversations_open_recipients(client, workspace, recipients))
        .await?;
    let opened: OpenedConversation = decode(value, "conversations.open")?;
    Ok(opened.channel)
}

pub async fn fetch_search_messages(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    args: SearchArgs,
) -> Result<SearchMessagesPage, Error> {
    let value = transport
        .execute(search_messages(client, workspace, args))
        .await?;
    decode(value, "search.modules.messages")
}

pub async fn fetch_search_inline(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    args: SearchInlineArgs,
) -> Result<SearchInlinePage, Error> {
    let value = transport
        .execute(search_inline(client, workspace, args))
        .await?;
    decode(value, "search.inline")
}

pub async fn edit_message(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    ts: MessageTs,
    text: String,
) -> Result<SentMessage, Error> {
    let value = transport
        .execute(chat_update(client, workspace, channel, ts, text))
        .await?;
    decode(value, "chat.update")
}

pub async fn delete_message(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    ts: MessageTs,
) -> Result<(), Error> {
    transport
        .execute(chat_delete(client, workspace, channel, ts))
        .await?;
    Ok(())
}

pub async fn add_reaction(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    timestamp: MessageTs,
    name: String,
) -> Result<(), Error> {
    transport
        .execute(reactions_add(client, workspace, channel, timestamp, name))
        .await?;
    Ok(())
}

pub async fn remove_reaction(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    timestamp: MessageTs,
    name: String,
) -> Result<(), Error> {
    transport
        .execute(reactions_remove(
            client, workspace, channel, timestamp, name,
        ))
        .await?;
    Ok(())
}
