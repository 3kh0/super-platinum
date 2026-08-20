use super::*;

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
    let value = transport
        .execute(conversations_history(client, workspace, args))
        .await?;
    decode(value, "conversations.history")
}

pub async fn fetch_replies(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    args: RepliesArgs,
) -> Result<HistoryPage, Error> {
    let value = transport
        .execute(conversations_replies(client, workspace, args))
        .await?;
    decode(value, "conversations.replies")
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

pub async fn mark_thread(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: ChannelId,
    root_ts: MessageTs,
    ts: MessageTs,
) -> Result<(), Error> {
    transport
        .execute(subscriptions_thread_mark(
            client, workspace, channel, root_ts, ts,
        ))
        .await?;
    Ok(())
}

pub async fn fetch_threads_view(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    limit: u32,
    max_ts: Option<MessageTs>,
    vip_only: bool,
) -> Result<ThreadsViewPage, Error> {
    let value = transport
        .execute(subscriptions_thread_get_view(
            client, workspace, limit, max_ts, vip_only,
        ))
        .await?;
    decode(value, "subscriptions.thread.getView")
}

pub async fn fetch_counts(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
) -> Result<CountsPage, Error> {
    let value = transport.execute(client_counts(client, workspace)).await?;
    decode(value, "client.counts")
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
    let value = transport
        .execute(activity_feed(client, workspace, limit, cursor, unread_only))
        .await?;
    decode(value, "activity.feed")
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

async fn fetch_conversation_info(
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
    decode(value, "chat.postMessage")
}

#[derive(serde::Deserialize)]
struct UploadUrl {
    upload_url: String,
    file_id: String,
}

/// What to upload and where, separate from the connection context that every
/// call in this module already threads through.
pub struct UploadRequest {
    pub channel: ChannelId,
    pub thread_ts: Option<MessageTs>,
    pub initial_comment: String,
    /// Each file paired with the counter its transfer progress is reported to.
    pub files: Vec<(PathBuf, Arc<AtomicU64>)>,
    pub canceled: Arc<AtomicBool>,
}

pub async fn upload_files(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    request: UploadRequest,
) -> Result<(), Error> {
    let UploadRequest {
        channel,
        thread_ts,
        initial_comment,
        files: files_to_upload,
        canceled,
    } = request;

    let mut files = Vec::with_capacity(files_to_upload.len());
    for (path, progress) in files_to_upload {
        if canceled.load(Ordering::Relaxed) {
            return Err(Error::UploadCanceled);
        }
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| Error::Transport("attachment has no valid filename".to_owned()))?
            .to_owned();
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|error| Error::Transport(format!("read {}: {error}", path.display())))?;
        let value = transport
            .execute(client.rest_form(
                workspace,
                "files.getUploadURLExternal",
                vec![
                    ("filename", filename.clone()),
                    ("length", bytes.len().to_string()),
                ],
            ))
            .await?;
        let ticket: UploadUrl = decode(value, "files.getUploadURLExternal")?;
        transport
            .upload_bytes(&ticket.upload_url, bytes, progress)
            .await?;
        if canceled.load(Ordering::Relaxed) {
            return Err(Error::UploadCanceled);
        }
        files.push(serde_json::json!({ "id": ticket.file_id, "title": filename }));
    }

    let mut fields = vec![
        (
            "files",
            serde_json::to_string(&files)
                .map_err(|error| Error::Transport(format!("encode upload files: {error}")))?,
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
        return Err(Error::UploadCanceled);
    }
    transport
        .execute(client.rest_form(workspace, "files.completeUploadExternal", fields))
        .await?;
    Ok(())
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
