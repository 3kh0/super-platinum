//! Pending integration: add beside execute.rs and reexport through api.rs.
//! Existing fetch_* functions delegate here; only execution is injectable.
use super::*;
use std::future::Future;

pub async fn fetch_counts_with<F, Fut>(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    execute: F,
) -> Result<CountsPage, Error>
where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
{
    decode(
        execute(client_counts(client, workspace)).await?,
        "client.counts",
    )
}

pub async fn fetch_threads_view_with<F, Fut>(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    limit: u32,
    current_ts: Option<MessageTs>,
    vip_only: bool,
    execute: F,
) -> Result<ThreadsViewPage, Error>
where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
{
    decode(
        execute(subscriptions_thread_get_view(
            client, workspace, limit, current_ts, vip_only,
        ))
        .await?,
        "subscriptions.thread.getView",
    )
}

pub async fn fetch_activity_feed_with<F, Fut>(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    limit: u32,
    cursor: Option<String>,
    unread_only: bool,
    execute: F,
) -> Result<ActivityFeedPage, Error>
where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
{
    decode(
        execute(activity_feed(client, workspace, limit, cursor, unread_only)).await?,
        "activity.feed",
    )
}

pub async fn fetch_history_with<F, Fut>(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    args: HistoryArgs,
    execute: F,
) -> Result<HistoryPage, Error>
where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
{
    decode(
        execute(conversations_history(client, workspace, args)).await?,
        "conversations.history",
    )
}

pub async fn fetch_replies_with<F, Fut>(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    args: RepliesArgs,
    execute: F,
) -> Result<HistoryPage, Error>
where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
{
    decode(
        execute(conversations_replies(client, workspace, args)).await?,
        "conversations.replies",
    )
}
