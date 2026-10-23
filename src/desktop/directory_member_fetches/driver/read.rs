//! Conditional users/info scheduler; request order survives concurrent replies.
use super::*;
use crate::profile_edit::sections::fixture::member_recovery as fixture;
use crate::slack_lists::comments::fixture::completion::title_users as list_title;
use super_platinum_core::block_actions::directory_endpoint::{
    DirectoryEndpoint,
    member_info::{Read, project_interaction},
};
use super_platinum_core::slack::transport::{ChannelReadAttempt, ChannelReadResponse};
pub(super) struct Received {
    pub(super) completed: Vec<String>,
    pub(super) terminal: NetworkTerminal,
    pub(super) pages: Vec<Arc<Page>>,
    pub(super) errors: Vec<Arc<Error>>,
}
impl Received {
    pub(super) fn timed_out(completed: Vec<String>) -> Self {
        Self {
            completed,
            terminal: NetworkTerminal::Failed,
            pages: vec![],
            errors: vec![],
        }
    }
}
struct Chunk {
    requested: Vec<String>,
    pages: Vec<Arc<Page>>,
    error: Option<Arc<Error>>,
    terminal: bool,
    cooldown: bool,
}
#[derive(Default)]
struct Collected {
    rows: Vec<Value>,
    interactions: BTreeMap<String, bool>,
    pages: Vec<Arc<Page>>,
    errors: Vec<Arc<Error>>,
    failed: Vec<String>,
    disconnected: Vec<String>,
    nonterminal: bool,
    remaining: BTreeSet<String>,
}
impl Collected {
    fn append(&mut self, chunks: Vec<Chunk>) -> Vec<String> {
        let mut pending = Vec::new();
        for chunk in chunks {
            self.nonterminal |= !chunk.terminal && !chunk.cooldown;
            if chunk.terminal {
                for id in &chunk.requested {
                    self.remaining.remove(id);
                }
            }
            if let Some(error) = chunk.error {
                if !chunk.cooldown {
                    self.failed.extend(chunk.requested);
                    self.errors.push(error);
                }
            } else {
                for page in &chunk.pages {
                    self.rows.extend_from_slice(page.rows());
                    self.interactions.extend(page.interactions().clone());
                    self.failed.extend_from_slice(page.invalid_ids());
                    self.disconnected.extend_from_slice(page.disconnected_ids());
                    self.remaining.extend(page.pending_ids().iter().cloned());
                    pending.extend_from_slice(page.pending_ids());
                }
            }
            self.pages.extend(chunk.pages);
        }
        pending
    }
    fn finish(self, dispatch: &Dispatch) -> Result<Received, Error> {
        if self.nonterminal {
            return Err(unavailable());
        }
        let completed: Vec<_> = dispatch
            .token
            .ids()
            .iter()
            .filter(|id| !self.remaining.contains(*id))
            .cloned()
            .collect();
        let accepted: BTreeSet<_> = completed.iter().map(String::as_str).collect();
        let original: BTreeSet<_> = dispatch
            .capture()
            .ids()
            .iter()
            .map(String::as_str)
            .collect();
        if self
            .rows
            .iter()
            .any(|row| row["id"].as_str().is_none_or(|id| !original.contains(id)))
        {
            return Err(unavailable());
        }

        // Pending rows remain retained in their actual Page, but cannot be
        // terminally projected before their ID has a completed observation.
        let rows: Vec<_> = self
            .rows
            .into_iter()
            .filter(|row| row["id"].as_str().is_some_and(|id| accepted.contains(id)))
            .collect();
        let rows = project_received(dispatch.capture(), &rows, &self.interactions)?;
        Ok(Received {
            completed,
            terminal: NetworkTerminal::Complete(rows),
            pages: self.pages,
            errors: self.errors,
        })
    }
}
type Endpoint = Arc<tokio::sync::Mutex<Option<Arc<DirectoryEndpoint>>>>;
pub(super) async fn acquire(
    state: Signal<ShellState>,
    dispatch: Arc<Dispatch>,
) -> Result<Received, Error> {
    let mut collected = Collected {
        remaining: dispatch.capture().ids().iter().cloned().collect(),
        ..Default::default()
    };
    let mut ids = dispatch.capture().ids().to_vec();
    let endpoint = Endpoint::default();
    'rounds: for round in 0..4u64 {
        if !dispatch.current(&state.peek()) {
            return Err(unavailable());
        }
        if cooling(&state.peek(), &dispatch) {
            break;
        }
        let chunks = dispatch.capture().pending_reads(&ids)?;
        let mut pending = Vec::new();
        for pair in chunks.chunks(2) {
            if !dispatch.current(&state.peek()) {
                return Err(unavailable());
            }
            if cooling(&state.peek(), &dispatch) {
                break 'rounds;
            }
            let (a, b) = tokio::join!(chunk(state, &dispatch, pair[0].clone(), &endpoint), async {
                match pair.get(1) {
                    Some(read) => Some(chunk(state, &dispatch, read.clone(), &endpoint).await),
                    None => None,
                }
            });
            let batch: Vec<_> = std::iter::once(a).chain(b).collect();
            let paused = batch.iter().any(|c| c.cooldown);
            pending.extend(collected.append(batch));
            if !dispatch.current(&state.peek()) {
                return Err(unavailable());
            }
            if paused {
                break 'rounds;
            }
        }
        ids = pending;
        if ids.is_empty() {
            break;
        }
        if round == 3 {
            // Actual exhausted pending retry scope, unlike an unsent cooldown.
            for id in &ids {
                collected.remaining.remove(id);
            }
            collected.failed.extend(ids);
            break;
        }
        if cooling(&state.peek(), &dispatch) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500 * (round + 1))).await;
    }
    if !dispatch.current(&state.peek()) {
        return Err(unavailable());
    }
    collected.finish(&dispatch)
}
pub(crate) fn project_received(
    capture: &Capture,
    rows: &[Value],
    interactions: &BTreeMap<String, bool>,
) -> Result<Vec<Value>, Error> {
    let allowed: BTreeSet<_> = capture.ids().iter().map(String::as_str).collect();
    if rows.iter().any(|row| {
        row.get("id")
            .and_then(Value::as_str)
            .is_none_or(|id| !allowed.contains(id))
    }) {
        return Err(unavailable());
    }
    let mut prior = capture.observations().interactions().clone();
    for id in capture.ids() {
        prior.entry(id.clone()).or_insert(None);
    }
    project_interaction(rows, interactions, &prior)
}
struct ReadFailure {
    error: Arc<Error>,
    cooldown: bool,
}
impl From<Error> for ReadFailure {
    fn from(error: Error) -> Self {
        Self {
            error: Arc::new(error),
            cooldown: false,
        }
    }
}
fn admit(shell: &ShellState, dispatch: &Dispatch) -> Result<(), ReadFailure> {
    if !dispatch.current(shell) {
        return Err(unavailable().into());
    }
    if cooling(shell, dispatch) {
        return Err(ReadFailure {
            error: Arc::new(unavailable()),
            cooldown: true,
        });
    }
    Ok(())
}
fn network_failure(
    mut state: Signal<ShellState>,
    dispatch: &Dispatch,
    error: Error,
) -> ReadFailure {
    let error = Arc::new(error);
    retain_failure(&mut state.write(), dispatch, error.clone());
    ReadFailure {
        error,
        cooldown: false,
    }
}
async fn chunk(
    state: Signal<ShellState>,
    dispatch: &Dispatch,
    read: Read,
    endpoint: &Endpoint,
) -> Chunk {
    let mut chunk = Chunk {
        requested: read.ids().to_vec(),
        pages: vec![],
        error: None,
        terminal: false,
        cooldown: false,
    };
    if let Err(failure) = pages(
        state,
        dispatch,
        read,
        endpoint,
        &mut chunk.pages,
        &mut chunk.terminal,
    )
    .await
    {
        chunk.cooldown = failure.cooldown;
        chunk.error = Some(failure.error);
    }
    chunk
}
async fn endpoint(
    state: Signal<ShellState>,
    dispatch: &Dispatch,
    shared: &Endpoint,
) -> Result<Arc<DirectoryEndpoint>, ReadFailure> {
    let mut value = shared.lock().await;
    let ctx = dispatch.context();
    admit(&state.peek(), dispatch)?;
    if let Some(value) = value
        .as_ref()
        .filter(|v| v.current(&ctx.session, Instant::now()))
    {
        return Ok(value.clone());
    }
    let observed = Instant::now();
    let request = DirectoryEndpoint::discovery(&ctx.client, &ctx.session)?;
    let raw = if crate::fixture::is_fixture() {
        if fixture::enabled() {
            fixture::discovery(state, ctx, &request).await
        } else if list_title::named() {
            list_title::discovery(state, ctx, &request).await
        } else {
            return Err(Error::Offline("closed member fixture".into()).into());
        }
    } else {
        // This existing closed factory is the same DirectoryEndpoint discovery
        // request, with original-owner admission after any offline hold.
        let attempt = ChannelReadAttempt::discovery(&ctx.client, &ctx.session)?;
        match ctx
            .transport
            .execute_channel_read_current(attempt, || {
                dispatch.current(&state.peek()) && !cooling(&state.peek(), dispatch)
            })
            .await
        {
            Ok(ChannelReadResponse::Setup(raw)) => Ok(raw),
            Ok(_) => Err(unavailable()),
            Err(error) => Err(error),
        }
    }
    .map_err(|error| network_failure(state, dispatch, error))?;
    if !dispatch.current(&state.peek()) {
        return Err(unavailable().into());
    }
    let found = Arc::new(DirectoryEndpoint::decode(&ctx.session, &raw, observed)?);
    *value = Some(found.clone());
    Ok(found)
}
async fn pages(
    state: Signal<ShellState>,
    dispatch: &Dispatch,
    mut read: Read,
    shared: &Endpoint,
    out: &mut Vec<Arc<Page>>,
    terminal: &mut bool,
) -> Result<(), ReadFailure> {
    let ctx = dispatch.context();
    let mut markers = BTreeSet::new();
    for _ in 0..50 {
        let endpoint = endpoint(state, dispatch, shared).await?;
        admit(&state.peek(), dispatch)?;
        let request = endpoint.member_info(&ctx.client, &ctx.session, Instant::now(), &read)?;
        let response = if crate::fixture::is_fixture() {
            if fixture::enabled() {
                fixture::member(state, ctx, &request).await
            } else if list_title::named() {
                list_title::member(state, ctx, &request, &read).await
            } else {
                Err(Error::Offline("closed member fixture".into()))
            }
        } else {
            ctx.transport
                .execute_directory_member_info_current(request, || {
                    dispatch.current(&state.peek()) && !cooling(&state.peek(), dispatch)
                })
                .await
                .map(|response| response.into_parts())
        };
        let (raw, keys) = match response {
            Ok(response) => response,
            Err(error) => {
                *terminal = matches!(
                    error,
                    Error::Api(_) | Error::HttpStatus { .. } | Error::RateLimited { .. }
                );
                return Err(network_failure(state, dispatch, error));
            }
        };
        // A sibling429 cannot invalidate this already-sent successful page.
        if !dispatch.current(&state.peek()) || !endpoint.current(&ctx.session, Instant::now()) {
            return Err(unavailable().into());
        }
        let page = Arc::new(Page::decode(&read, raw, keys)?);
        let next = page.next_marker().map(str::to_owned);
        out.push(page);
        let Some(next) = next else {
            *terminal = true;
            return Ok(());
        };
        if !markers.insert(next.clone()) {
            return Err(unavailable().into());
        }
        read = read.next_page(&next)?;
    }
    Err(unavailable().into())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn page(id: &str, pending: Option<&str>) -> Arc<Page> {
        let mut ids = vec![id.to_owned()];
        if let Some(other) = pending {
            ids.push(other.into());
        }
        let read = Read::capture(ids).unwrap();
        let mut raw = json!({"ok":true,"results":[{"id":id}],"can_interact":{(id):true}});
        if let Some(other) = pending {
            raw["pending_ids"] = json!([other]);
        }
        Arc::new(Page::decode(&read, raw, None).unwrap())
    }
    #[test]
    fn ordered_chunks_retain_failed_prefix_without_projecting_it() {
        let first = page("UZFIRST12", None);
        let second = page("UASECOND1", None);
        let error = Arc::new(Error::Api("synthetic_server_error".into()));
        let mut result = Collected::default();
        assert!(
            result
                .append(vec![
                    Chunk {
                        requested: vec!["UZFIRST12".into()],
                        pages: vec![first.clone()],
                        error: None,
                        terminal: true,
                        cooldown: false,
                    },
                    Chunk {
                        requested: vec!["UASECOND1".into()],
                        pages: vec![second.clone()],
                        error: Some(error.clone()),
                        terminal: true,
                        cooldown: false,
                    },
                ])
                .is_empty()
        );
        assert_eq!(result.rows, vec![json!({"id":"UZFIRST12"})]);
        assert_eq!(result.failed, vec!["UASECOND1"]);
        assert!(Arc::ptr_eq(&result.pages[0], &first));
        assert!(Arc::ptr_eq(&result.pages[1], &second));
        assert!(Arc::ptr_eq(&result.errors[0], &error));
        assert!(!result.nonterminal);
    }
    #[test]
    fn actual_pending_order_and_local_refusal_remain_distinct_from_terminal() {
        let mut result = Collected::default();
        let pending = result.append(vec![Chunk {
            requested: vec!["U00000001".into(), "UZPENDING".into()],
            pages: vec![page("U00000001", Some("UZPENDING"))],
            error: None,
            terminal: true,
            cooldown: false,
        }]);
        assert_eq!(pending, vec!["UZPENDING"]);
        result.append(vec![Chunk {
            requested: pending,
            pages: vec![],
            error: Some(Arc::new(unavailable())),
            terminal: false,
            cooldown: false,
        }]);
        assert!(result.nonterminal);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.failed, vec!["UZPENDING"]);
    }
}

/// Drives actual aggregation/error admission with retained synthetic pages;
/// no transport or wall-clock wait is performed by this test seam.
#[cfg(test)]
pub(super) fn mixed_cooldown_fixture(
    state: Signal<ShellState>,
    dispatch: &Dispatch,
    pending_round: bool,
) -> (Received, Arc<Error>) {
    use serde_json::json;
    let ids = dispatch.capture().ids();
    assert_eq!(ids.len(), if pending_round { 101 } else { 201 });
    let (success_ids, failure_ids, deferred) = if pending_round {
        (&ids[..100], &ids[100..], ids[99].clone())
    } else {
        (&ids[100..200], &ids[..100], ids[200].clone())
    };
    let rows: Vec<_> = success_ids
        .iter()
        .filter(|id| !pending_round || **id != deferred)
        .map(|id| json!({"id":id,"name":"Observed","profile":{"real_name":"Observed"}}))
        .collect();
    let interactions: serde_json::Map<String, Value> = success_ids
        .iter()
        .filter(|id| !pending_round || **id != deferred)
        .map(|id| (id.clone(), json!(true)))
        .collect();
    let mut raw = json!({"ok":true,"results":rows,"can_interact":interactions});
    if pending_round {
        raw["pending_ids"] = json!([deferred]);
    }
    let page =
        Arc::new(Page::decode(&Read::capture(success_ids.to_vec()).unwrap(), raw, None).unwrap());
    let failure = network_failure(
        state,
        dispatch,
        Error::RateLimited {
            retry_after_secs: Some(17),
        },
    );
    let original = failure.error.clone();
    let success = Chunk {
        requested: success_ids.to_vec(),
        pages: vec![page.clone()],
        error: None,
        terminal: true,
        cooldown: false,
    };
    let failure = Chunk {
        requested: failure_ids.to_vec(),
        pages: vec![],
        error: Some(failure.error),
        terminal: true,
        cooldown: false,
    };
    let mut collected = Collected {
        remaining: ids.iter().cloned().collect(),
        ..Default::default()
    };
    let pending = collected.append(if pending_round {
        vec![success, failure]
    } else {
        vec![failure, success]
    });
    if pending_round {
        assert_eq!(pending, vec![deferred.clone()]);
    } else {
        assert!(pending.is_empty());
    }
    // The next pair or pending retry has not reached Transport. Its local
    // cooldown refusal must neither overwrite the original Arc nor be missing.
    let refusal = match admit(&state.peek(), dispatch) {
        Err(value) => value,
        Ok(()) => panic!("expected cooldown"),
    };
    assert!(refusal.cooldown);
    collected.append(vec![Chunk {
        requested: vec![deferred.clone()],
        pages: vec![],
        error: Some(refusal.error),
        terminal: false,
        cooldown: true,
    }]);
    assert_eq!(collected.remaining, BTreeSet::from([deferred.clone()]));
    assert!(!collected.nonterminal);
    let result = collected.finish(dispatch).unwrap();
    assert_eq!(
        result.completed,
        ids.iter()
            .filter(|id| **id != deferred)
            .cloned()
            .collect::<Vec<_>>()
    );
    assert!(Arc::ptr_eq(&result.pages[0], &page));
    assert_eq!(result.errors.len(), 1);
    assert!(Arc::ptr_eq(&result.errors[0], &original));
    let shell = state.peek();
    let slot = shell
        .directory_member_fetches
        .slot(&dispatch.context)
        .unwrap();
    assert!(Arc::ptr_eq(slot.last_failure.as_ref().unwrap(), &original));
    (result, original)
}
