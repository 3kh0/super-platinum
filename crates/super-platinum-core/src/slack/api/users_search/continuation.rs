//! Complete one bounded Edge lookup; this is not global People page navigation.
use super::*;

/// None means the owner retired. Errors never publish partial success.
/// Keep the final response whole for the existing unique/nonempty UI projection;
/// unlike Slack's generic SDK, this compatibility adapter does not raw-slice it.
pub async fn fetch_users_search_continued_with<F, Fut, C>(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    query: String,
    mut execute: F,
    current: C,
) -> Result<Option<Vec<User>>, Error>
where
    F: FnMut(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
    C: FnMut() -> bool,
{
    super::super::query_continuation::collect(
        "users/search",
        USER_SEARCH_LIMIT as usize,
        |marker| {
            super::super::super::edge::users_search_page(
                client,
                workspace,
                &query,
                USER_SEARCH_LIMIT,
                marker,
            )
            .map(Some)
            .map_err(|e| Error::Transport(format!("build users/search: {e}")))
        },
        decode_users_search_page,
        |request| {
            let work = execute(request);
            async move { work.await.map(Some) }
        },
        current,
    )
    .await
}

#[cfg(test)]
mod tests;
