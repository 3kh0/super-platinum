//! Calendar SINGLE channel provider's raw100 budget, distinct from popup ranking.
use super::*;
use std::future::Future;

fn decode(value: serde_json::Value) -> Result<EdgeResults<serde_json::Value>, Error> {
    let results = value
        .get("results")
        .and_then(serde_json::Value::as_array)
        .filter(|rows| rows.len() <= 512)
        .ok_or_else(|| Error::Api("calendar_sharing_channel_results_invalid".into()))?
        .clone();
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
pub async fn fetch_calendar_sharing_channel_rows_with<B, F, Fut, C>(
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
    super::query_continuation::collect("channels/search", 100, build, decode, execute, current)
        .await
        .map(|rows| {
            rows.map(|mut rows| {
                rows.truncate(100);
                rows
            })
        })
}

#[cfg(test)]
mod tests;
