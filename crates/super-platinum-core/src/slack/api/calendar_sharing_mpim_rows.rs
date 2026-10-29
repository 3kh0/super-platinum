//! Bounded SINGLE MPIM raw prefix; the actual first response is not redispatched.
use super::*;
use std::future::Future;
mod convert;
pub use convert::convert_calendar_known_open_mpim;
fn invalid() -> Error {
    Error::Api("calendar_sharing_mpim_results_invalid".into())
}
fn decode(raw: serde_json::Value) -> Result<EdgeResults<serde_json::Value>, Error> {
    use serde_json::Value;
    if raw.get("ok").is_some_and(|v| v != &Value::Bool(true))
        || raw
            .get("huddles")
            .is_some_and(|v| !v.as_array().is_some_and(Vec::is_empty))
    {
        return Err(invalid());
    }
    let mut results = Vec::new();
    let mut seen = false;
    // Source SDK appends these two contributions in this exact raw order.
    for key in ["results", "mpims"] {
        if let Some(value) = raw.get(key) {
            let rows = value
                .as_array()
                .filter(|v| v.len() <= 512)
                .ok_or_else(invalid)?;
            results.extend(rows.iter().cloned());
            seen = true;
        }
    }
    if !seen {
        return Err(invalid());
    }
    let mut extra = BTreeMap::new();
    if let Some(marker) = raw.get("next_marker") {
        extra.insert("next_marker".into(), marker.clone());
    }
    Ok(EdgeResults {
        ok: true,
        results,
        failed_ids: Vec::new(),
        extra,
    })
}
pub async fn fetch_calendar_sharing_mpim_rows_after_with<B, F, Fut, C>(
    first: serde_json::Value,
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
    super::query_continuation::collect_after_first(
        "mpims/search",
        100,
        first,
        build,
        decode,
        execute,
        current,
    )
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
