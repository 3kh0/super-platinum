//! Shared bounded raw-page policy for the explicitly admitted Edge query routes.
use super::*;
use std::{collections::HashSet, future::Future};

const MAX_REQUESTS: usize = 30;
const MAX_MARKER_BYTES: usize = 4096;

fn marker(value: Option<serde_json::Value>, endpoint: &str) -> Result<Option<String>, Error> {
    use serde_json::Value;
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => Ok(None),
        Some(Value::Number(n)) if n.as_f64() == Some(0.0) => Ok(None),
        Some(Value::String(s)) if s.is_empty() => Ok(None),
        Some(Value::String(s)) if s.len() <= MAX_MARKER_BYTES => Ok(Some(s)),
        _ => Err(Error::Transport(format!(
            "decode {endpoint}: unsupported continuation marker"
        ))),
    }
}

pub(super) async fn collect<T, B, D, F, Fut, C>(
    endpoint: &str,
    limit: usize,
    build: B,
    decode_page: D,
    execute: F,
    current: C,
) -> Result<Option<Vec<T>>, Error>
where
    B: FnMut(Option<&str>) -> Result<Option<PreparedRequest>, Error>,
    D: FnMut(serde_json::Value) -> Result<EdgeResults<T>, Error>,
    F: FnMut(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Option<serde_json::Value>, Error>>,
    C: FnMut() -> bool,
{
    collect_inner(endpoint, limit, None, build, decode_page, execute, current).await
}

/// Seed with an already admitted actual response; it counts toward MAX_REQUESTS.
pub(super) async fn collect_after_first<T, B, D, F, Fut, C>(
    endpoint: &str,
    limit: usize,
    first: serde_json::Value,
    build: B,
    decode_page: D,
    execute: F,
    current: C,
) -> Result<Option<Vec<T>>, Error>
where
    B: FnMut(Option<&str>) -> Result<Option<PreparedRequest>, Error>,
    D: FnMut(serde_json::Value) -> Result<EdgeResults<T>, Error>,
    F: FnMut(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Option<serde_json::Value>, Error>>,
    C: FnMut() -> bool,
{
    collect_inner(
        endpoint,
        limit,
        Some(first),
        build,
        decode_page,
        execute,
        current,
    )
    .await
}

async fn collect_inner<T, B, D, F, Fut, C>(
    endpoint: &str,
    limit: usize,
    mut first: Option<serde_json::Value>,
    mut build: B,
    mut decode_page: D,
    mut execute: F,
    mut current: C,
) -> Result<Option<Vec<T>>, Error>
where
    B: FnMut(Option<&str>) -> Result<Option<PreparedRequest>, Error>,
    D: FnMut(serde_json::Value) -> Result<EdgeResults<T>, Error>,
    F: FnMut(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Option<serde_json::Value>, Error>>,
    C: FnMut() -> bool,
{
    let mut rows = Vec::new();
    let mut next = None;
    let mut seen = HashSet::new();
    for _ in 0..MAX_REQUESTS {
        if !current() {
            return Ok(None);
        }
        let received = if let Some(received) = first.take() {
            // Request1 already reached its original observer. Do not build,
            // redispatch or retrospectively re-admit this actual response.
            received
        } else {
            let Some(request) = build(next.as_deref())? else {
                return Ok(None);
            };
            let received = execute(request).await;
            // Retirement fences errors as well as decoded rows and successors.
            if !current() {
                return Ok(None);
            }
            let Some(received) = received? else {
                return Ok(None);
            };
            received
        };
        let mut page = decode_page(received)?;
        rows.append(&mut page.results);
        // Whole-final-response compatibility is deliberate; domain finalizers
        // still own eligibility, deduplication, ranking and the visible limit.
        if rows.len() >= limit {
            return Ok(Some(rows));
        }
        next = marker(page.extra.remove("next_marker"), endpoint)?;
        let Some(next_marker) = next.as_ref() else {
            return Ok(Some(rows));
        };
        if !seen.insert(next_marker.clone()) {
            return Err(Error::Transport(format!(
                "{endpoint} continuation did not advance"
            )));
        }
    }
    Err(Error::Transport(format!(
        "{endpoint} continuation exceeded its request limit"
    )))
}
