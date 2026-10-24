//! Unknown acquisition uses the existing typed directory and generic protocols.
//! Every dispatch and response remains bound to the exact current owner.
use super::*;
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};
use super_platinum_core::{
    block_actions::directory_endpoint::{
        DirectoryEndpoint,
        channel_info::{Page, Read},
    },
    slack::transport::{ChannelReadAttempt, ChannelReadResponse},
};
pub(super) async fn execute(
    state: Signal<ShellState>,
    attempt: &Attempt,
    read: ChannelReadAttempt,
) -> Result<ChannelReadResponse, Error> {
    execute_guarded(state, attempt, read, || true).await
}
async fn execute_guarded(
    mut state: Signal<ShellState>,
    attempt: &Attempt,
    read: ChannelReadAttempt,
    eligible: impl Fn() -> bool,
) -> Result<ChannelReadResponse, Error> {
    if crate::fixture::is_fixture() {
        return if fixture::installed(&state.peek()) && attempt.dispatch(&state.peek()) {
            Ok(ChannelReadResponse::Setup(
                serde_json::json!({"ok":true,"url":format!("https://edgeapi.slack.com/cache/{}",attempt.owner.context.session.team_id),"ttl_seconds":300}),
            ))
        } else {
            Err(Error::Offline("closed channel fixture".into()))
        };
    }
    let result = attempt
        .owner
        .context
        .transport
        .execute_channel_read_current(read, || {
            let allowed = attempt.dispatch(&state.peek()) && eligible();
            if !allowed {
                attempt.refused_dispatch.set(true);
            }
            allowed
        })
        .await;
    if let Err(error) = &result
        && !attempt.refused_dispatch.get()
    {
        // A still-current Offline wrapper error is a local failure too. A
        // refused final-dispatch fence is cancellation, not terminal evidence.
        failure(&mut state.write(), attempt, error.clone());
    }
    if !attempt.current(&state.peek()) {
        return Err(unavailable());
    }
    result
}
struct Chunk {
    ids: Vec<String>,
    pages: Vec<Page>,
    failed: bool,
}
struct Fallback {
    raw: Option<Value>,
}
fn versions(state: Signal<ShellState>, attempt: &Attempt) -> Result<BTreeMap<String, u64>, Error> {
    versions_in(&state.peek(), attempt)
}
pub(super) fn versions_in(
    shell: &ShellState,
    attempt: &Attempt,
) -> Result<BTreeMap<String, u64>, Error> {
    observed_attempt(shell, attempt)?;
    let shadow = attempt.shadow.borrow();
    attempt
        .flight
        .ids()
        .iter()
        .map(|id| {
            let updated = shadow.conditional_version(id).map_err(|_| unavailable())?;
            Ok((id.clone(), updated))
        })
        .collect()
}

/// Capture wrapper-retained IDs from the current owned dispatch snapshot. A
/// present runtime_unknown is still a miss; a Joined-only receipt is no result.
pub(super) fn requested(shell: &ShellState, attempt: &Attempt) -> Result<Vec<String>, Error> {
    observed_attempt(shell, attempt)?;
    let shadow = attempt.shadow.borrow();
    let mut requested = Vec::new();
    let mut retained = Vec::new();
    for id in attempt.flight.ids() {
        if shadow.row(id).map_err(|_| unavailable())?.kind
            == super_platinum_core::state::directory_channels::projection::ClassificationKind::Absent
            || shadow.is_unknown(id).map_err(|_| unavailable())?
        {
            requested.push(id.clone());
        } else {
            retained.push(id.clone());
        }
    }
    attempt.accepted_ids.borrow_mut().extend(retained);
    Ok(requested)
}

pub(super) async fn acquire(state: Signal<ShellState>, attempt: &Attempt) -> Result<(), Error> {
    let updated = versions(state, attempt)?;
    let ingress = crate::bootstrap::posting_ingress::capture(
        &state,
        &attempt.owner.context.transport,
        &attempt.owner.context.session,
    )
    .for_channels(attempt.flight.ids());
    let endpoint = Rc::new(RefCell::new(None));
    let requested = requested(&state.peek(), attempt)?;
    let reads = requested
        .chunks(100)
        .map(|ids| {
            Read::capture(
                ids.to_vec(),
                Some(ids.iter().map(|id| (id.clone(), updated[id])).collect()),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut failed = Vec::new();
    let mut resolved = Vec::new();
    let mut returned = Vec::new();
    for pair in reads.chunks(2) {
        let (a, b) = tokio::join!(
            chunk_publish(state, attempt, &ingress, pair[0].clone(), endpoint.clone()),
            async {
                match pair.get(1) {
                    Some(r) => Some(
                        chunk_publish(state, attempt, &ingress, r.clone(), endpoint.clone()).await,
                    ),
                    None => None,
                }
            }
        );
        // Each branch publishes before returning. Inspect both even when one
        // refused locally; a later unsent chunk cannot erase an accepted sibling.
        let mut results = vec![a];
        if let Some(b) = b {
            results.push(b);
        }
        let mut refused = false;
        for result in results {
            match result {
                Ok(chunk) => {
                    if chunk.failed {
                        failed.extend(chunk.ids);
                        continue;
                    }
                    let bad: BTreeSet<_> = chunk
                        .pages
                        .iter()
                        .flat_map(|p| p.failed_ids())
                        .cloned()
                        .collect();
                    resolved.extend(chunk.ids.into_iter().filter(|id| !bad.contains(id)));
                    failed.extend(bad);
                    for page in chunk.pages {
                        for row in page.rows() {
                            returned.push(row["id"].as_str().ok_or_else(unavailable)?.to_owned());
                        }
                    }
                }
                Err(_) => refused = true,
            }
        }
        observed_attempt(&state.peek(), attempt)?;
        if refused {
            break;
        }
    }
    let mut seen = BTreeSet::new();
    failed.retain(|id| seen.insert(id.clone()));
    let chunks = failed.chunks(250).collect::<Vec<_>>();
    for pair in chunks.chunks(2) {
        let (a, b) = tokio::join!(
            fallback_publish(state, attempt, &ingress, pair[0], &updated),
            async {
                match pair.get(1) {
                    Some(ids) => {
                        Some(fallback_publish(state, attempt, &ingress, ids, &updated).await)
                    }
                    None => None,
                }
            }
        );
        let mut results = vec![a];
        if let Some(b) = b {
            results.push(b);
        }
        let mut refused = false;
        for result in results {
            match result {
                Ok((ids, rows)) => {
                    resolved.extend(ids);
                    returned.extend(rows);
                }
                Err(_) => refused = true,
            }
        }
        observed_attempt(&state.peek(), attempt)?;
        if refused {
            break;
        }
    }
    publish::unchanged(state, attempt, &resolved, &returned)
}

async fn chunk_publish(
    state: Signal<ShellState>,
    attempt: &Attempt,
    ingress: &crate::bootstrap::posting_ingress::Ingress,
    read: Read,
    endpoint: Rc<RefCell<Option<Rc<DirectoryEndpoint>>>>,
) -> Result<Chunk, Error> {
    let chunk = chunk(state, attempt, read, endpoint).await?;
    if chunk.failed {
        return Ok(chunk);
    }
    let mut rows = Vec::new();
    let mut membership = Vec::new();
    for page in &chunk.pages {
        rows.extend_from_slice(page.rows());
        membership.extend_from_slice(page.membership_ids());
    }
    if !membership.is_empty() {
        for row in &mut rows {
            let id = row["id"].as_str().ok_or_else(unavailable)?;
            row["is_member"] = Value::Bool(
                membership.iter().any(|m| m == id) && (!truth(&row["is_archived"]) || private(row)),
            );
        }
    }
    publish::rows(state, attempt, ingress, &rows, Some(&membership))?;
    Ok(chunk)
}
async fn fallback_publish(
    state: Signal<ShellState>,
    attempt: &Attempt,
    ingress: &crate::bootstrap::posting_ingress::Ingress,
    ids: &[String],
    updated: &BTreeMap<String, u64>,
) -> Result<(Vec<String>, Vec<String>), Error> {
    let result = fallback(state, attempt, ids, updated).await?;
    let Some(raw) = result.raw else {
        return Ok((Vec::new(), Vec::new()));
    };
    let rows = raw["channels"].as_array().cloned().unwrap_or_default();
    let returned: Vec<String> = rows
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect();
    let mut resolved = returned.clone();
    resolved.extend(
        raw["unchanged_channel_ids"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|id| id.as_str().unwrap().to_owned()),
    );
    publish::rows(state, attempt, ingress, &rows, None)?;
    Ok((resolved, returned))
}
async fn chunk(
    mut state: Signal<ShellState>,
    attempt: &Attempt,
    mut read: Read,
    endpoint: Rc<RefCell<Option<Rc<DirectoryEndpoint>>>>,
) -> Result<Chunk, Error> {
    let mut out = Chunk {
        ids: read.ids().to_vec(),
        pages: Vec::new(),
        failed: false,
    };
    let mut markers = BTreeSet::new();
    for _ in 0..50 {
        if !attempt.dispatch(&state.peek()) {
            return Err(unavailable());
        }
        let refresh = endpoint
            .borrow()
            .as_ref()
            .is_none_or(|e| !e.current(&attempt.owner.context.session, Instant::now()));
        if refresh {
            let at = Instant::now();
            let result = execute(
                state,
                attempt,
                ChannelReadAttempt::discovery(
                    &attempt.owner.context.client,
                    &attempt.owner.context.session,
                )?,
            )
            .await;
            if !attempt.current(&state.peek()) {
                return Err(unavailable());
            }
            match result {
                Ok(ChannelReadResponse::Setup(raw)) => {
                    match DirectoryEndpoint::decode(&attempt.owner.context.session, &raw, at) {
                        Ok(value) => *endpoint.borrow_mut() = Some(Rc::new(value)),
                        Err(error) => {
                            failure(&mut state.write(), attempt, error);
                            out.failed = true;
                            return Ok(out);
                        }
                    }
                }
                Ok(_) => return Err(unavailable()),
                Err(_) => {
                    out.failed = true;
                    return Ok(out);
                }
            }
        }
        // Retain the exact discovery observation that built this request.
        // A sibling replacing the cache cannot extend an old request's TTL.
        let exact_endpoint = endpoint
            .borrow()
            .as_ref()
            .cloned()
            .ok_or_else(unavailable)?;
        let request = exact_endpoint.channel_info(
            &attempt.owner.context.client,
            &attempt.owner.context.session,
            Instant::now(),
            &read,
        )?;
        // Discovery can finish after a sibling429. This is a new dispatch.
        if !attempt.dispatch(&state.peek()) {
            return Err(unavailable());
        }
        let read_attempt = ChannelReadAttempt::directory(request)?;
        let decoded = if crate::fixture::is_fixture() {
            let versions = read
                .ids()
                .iter()
                .map(|id| {
                    Ok((
                        id.clone(),
                        attempt
                            .shadow
                            .borrow()
                            .conditional_version(id)
                            .map_err(|_| unavailable())?,
                    ))
                })
                .collect::<Result<BTreeMap<_, _>, Error>>()?;
            let receive = fixture::hold(
                &mut state.write(),
                attempt,
                "channels.info",
                read.ids(),
                versions,
            )?;
            match receive.await.map_err(|_| unavailable())? {
                Ok(raw) => Page::decode(&read, raw, None),
                Err(error) => Err(error),
            }
        } else {
            match execute_guarded(state, attempt, read_attempt, || {
                exact_endpoint.current(&attempt.owner.context.session, Instant::now())
            })
            .await
            {
                Ok(ChannelReadResponse::Directory(response)) => {
                    let (raw, keys) = response.into_parts();
                    Page::decode(&read, raw, keys)
                }
                Ok(_) => Err(unavailable()),
                Err(error) => Err(error),
            }
        };
        if !attempt.current(&state.peek()) {
            return Err(unavailable());
        }
        let page = match decoded {
            Ok(page) => page,
            Err(error) => {
                failure(&mut state.write(), attempt, error);
                out.failed = true;
                return Ok(out);
            }
        };
        let next = page.next_marker().map(str::to_owned);
        out.pages.push(page);
        match next {
            None => return Ok(out),
            Some(next) if markers.insert(next.clone()) => read = read.next_page(&next)?,
            _ => {
                failure(&mut state.write(), attempt, unavailable());
                out.failed = true;
                return Ok(out);
            }
        }
    }
    failure(&mut state.write(), attempt, unavailable());
    out.failed = true;
    Ok(out)
}
async fn fallback(
    mut state: Signal<ShellState>,
    attempt: &Attempt,
    ids: &[String],
    updated: &BTreeMap<String, u64>,
) -> Result<Fallback, Error> {
    if !attempt.dispatch(&state.peek()) {
        return Err(unavailable());
    }
    let versions = ids.iter().map(|id| (id.clone(), updated[id])).collect();
    let read_attempt = ChannelReadAttempt::generic(
        &attempt.owner.context.client,
        &attempt.owner.context.session,
        &versions,
    );
    let result = if crate::fixture::is_fixture() {
        let receive = fixture::hold(
            &mut state.write(),
            attempt,
            "conversations.genericInfo",
            ids,
            versions,
        )?;
        receive
            .await
            .map_err(|_| unavailable())?
            .map(ChannelReadResponse::Generic)
    } else {
        execute(state, attempt, read_attempt).await
    };
    if !attempt.current(&state.peek()) {
        return Err(unavailable());
    }
    let raw = match result {
        Ok(ChannelReadResponse::Generic(raw)) if valid_fallback(&raw, ids) => Some(raw),
        Ok(_) => {
            failure(&mut state.write(), attempt, unavailable());
            None
        }
        Err(_) => None,
    };
    Ok(Fallback { raw })
}
pub(super) fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_some_and(|v| v != 0.0),
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}
fn private(v: &Value) -> bool {
    (truth(&v["is_private"]) || truth(&v["is_group"])) && !truth(&v["is_mpim"])
}

fn valid_fallback(raw: &Value, ids: &[String]) -> bool {
    raw["ok"] == true
        && raw.get("error").is_none_or(Value::is_null)
        && raw.get("channels").is_none_or(|v| {
            v.as_array().is_some_and(|rows| {
                rows.iter().all(|row| {
                    row["id"]
                        .as_str()
                        .is_some_and(|id| ids.iter().any(|wanted| wanted == id))
                })
            })
        })
        && raw.get("unchanged_channel_ids").is_none_or(|v| {
            v.as_array().is_some_and(|rows| {
                rows.iter().all(|row| {
                    row.as_str()
                        .is_some_and(|id| ids.iter().any(|wanted| wanted == id))
                })
            })
        })
}
