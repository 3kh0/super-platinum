//! Scalar selector raw budget. Deliberately distinct from typed People/List pages.
use super::*;
use std::future::Future;

fn decode_rows(value: serde_json::Value) -> Result<EdgeResults<serde_json::Value>, Error> {
    let results = value
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| Error::Api("conversation_search_invalid".into()))?
        .clone();
    // Preserve the selector's existing array-only/item-permissive contract.
    // Transport owns Slack not-ok handling; selector metadata was never typed.
    let mut extra = BTreeMap::new();
    if let Some(marker) = value.get("next_marker") {
        extra.insert("next_marker".into(), marker.clone());
    }
    Ok(EdgeResults {
        ok: true,
        results,
        failed_ids: Vec::new(),
        extra,
    })
}

pub async fn fetch_users_selector_rows_with<B, F, Fut, C>(
    build: B,
    execute: F,
    current: C,
) -> Result<Option<Vec<serde_json::Value>>, Error>
where
    B: FnMut(Option<&str>) -> Result<Option<PreparedRequest>, Error>,
    F: FnMut(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Option<serde_json::Value>, Error>>,
    C: FnMut() -> bool,
{
    fetch_selector_rows_with("users/search", build, execute, current).await
}

/// Ordinary channel selectors share the raw budget, not the typed List decoder.
pub async fn fetch_channels_selector_rows_with<B, F, Fut, C>(
    build: B,
    execute: F,
    current: C,
) -> Result<Option<Vec<serde_json::Value>>, Error>
where
    B: FnMut(Option<&str>) -> Result<Option<PreparedRequest>, Error>,
    F: FnMut(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Option<serde_json::Value>, Error>>,
    C: FnMut() -> bool,
{
    fetch_selector_rows_with("channels/search", build, execute, current).await
}

async fn fetch_selector_rows_with<B, F, Fut, C>(
    endpoint: &'static str,
    build: B,
    execute: F,
    current: C,
) -> Result<Option<Vec<serde_json::Value>>, Error>
where
    B: FnMut(Option<&str>) -> Result<Option<PreparedRequest>, Error>,
    F: FnMut(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Option<serde_json::Value>, Error>>,
    C: FnMut() -> bool,
{
    super::query_continuation::collect(endpoint, 24, build, decode_rows, execute, current)
        .await
        .map(|rows| {
            rows.map(|mut rows| {
                rows.truncate(24);
                rows
            })
        })
}

#[cfg(test)]
mod channel_tests;
#[cfg(test)]
mod tests;
