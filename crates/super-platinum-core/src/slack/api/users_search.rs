//! Existing bounded directory lookup, shared by production and offline callers.
use super::*;
use std::future::Future;
mod continuation;
pub use continuation::fetch_users_search_continued_with;

pub const USER_SEARCH_LIMIT: u32 = 30;

pub async fn fetch_users_search_with<F, Fut>(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    query: String,
    execute: F,
) -> Result<Vec<User>, Error>
where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
{
    let request = super::super::edge::users_search(client, workspace, &query, USER_SEARCH_LIMIT)
        .map_err(|e| Error::Transport(format!("build users/search: {e}")))?;
    let value = execute(request).await?;
    Ok(decode_users_search_page(value)?.results)
}

fn decode_users_search_page(value: serde_json::Value) -> Result<EdgeResults<User>, Error> {
    if value["ok"] == false {
        return Err(Error::Api(
            value["error"]
                .as_str()
                .unwrap_or("user_search_failed")
                .into(),
        ));
    }
    // Do not turn a malformed successful envelope into a successful empty list.
    // This requirement belongs to users/search, not shared EdgeResults defaults.
    if !value
        .get("results")
        .is_some_and(serde_json::Value::is_array)
    {
        return Err(Error::Transport(
            "decode users/search: results must be an array".into(),
        ));
    }
    // Preserve the original typed ok/failed_ids validation as well as users.
    let page: EdgeResults<User> = decode(value, "users/search")?;
    Ok(page)
}

#[cfg(test)]
mod tests;
