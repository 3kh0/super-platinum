//! Explicit bounded Channel-query continuation; legacy single-call API is intact.
use super::*;
use std::future::Future;

const LIMIT: u32 = 30;

fn decode_page(value: serde_json::Value) -> Result<EdgeResults<Channel>, Error> {
    if value["ok"] == false {
        return Err(Error::Api(
            value["error"]
                .as_str()
                .unwrap_or("channel_search_failed")
                .into(),
        ));
    }
    if !value
        .get("results")
        .is_some_and(serde_json::Value::is_array)
    {
        return Err(Error::Transport(
            "decode channels/search: results must be an array".into(),
        ));
    }
    decode(value, "channels/search")
}

pub async fn fetch_channels_search_continued_with<F, Fut, C>(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    query: String,
    mut execute: F,
    current: C,
) -> Result<Option<Vec<Channel>>, Error>
where
    F: FnMut(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
    C: FnMut() -> bool,
{
    super::query_continuation::collect(
        "channels/search",
        LIMIT as usize,
        |marker| {
            super::super::edge::channels_search_page(client, workspace, &query, LIMIT, marker)
                .map(Some)
                .map_err(|e| Error::Transport(format!("build channels/search: {e}")))
        },
        decode_page,
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
