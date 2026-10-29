//! Calendar SINGLE member raw100 acquisition, before canonical item admission.
use super::*;
use std::future::Future;

fn invalid() -> Error {
    Error::Api("calendar_sharing_member_results_invalid".into())
}
fn decode(value: serde_json::Value) -> Result<EdgeResults<serde_json::Value>, Error> {
    if value
        .get("ok")
        .is_some_and(|ok| ok != &serde_json::Value::Bool(true))
    {
        return Err(invalid());
    }
    // Source can flatten object/auxiliary rows; this closed array-only path cannot.
    for key in ["mpims", "huddles"] {
        if value
            .get(key)
            .is_some_and(|rows| !rows.as_array().is_some_and(Vec::is_empty))
        {
            return Err(invalid());
        }
    }
    let results = value
        .get("results")
        .and_then(serde_json::Value::as_array)
        .filter(|rows| rows.len() <= 512)
        .ok_or_else(invalid)?
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

pub async fn fetch_calendar_sharing_member_rows_with<B, F, Fut, C>(
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
    super::query_continuation::collect("users/search", 100, build, decode, execute, current)
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
