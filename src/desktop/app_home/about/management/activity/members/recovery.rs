//! Actual 2gUC unconditional chunk/pending orchestration. Successful raw
//! chunks retain source request order, regardless of completion order.
//! Store projection/adoption is separate and requires the shared prior/ledger
//! successor; this read alone cannot repair or authorize a source view.
use super::*;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
use super_platinum_core::block_actions::directory_endpoint::{
    DirectoryEndpoint,
    member_info::{Page, Read, unknown::Capture},
};
#[cfg(test)]
mod tests;

pub(super) struct Chunk {
    requested: Vec<String>,
    pages: Vec<Page>,
    error: Option<Error>,
    // True only after all pages, or an actual server rejection. Local
    // admission/TTL/shape/resource refusals cannot finish an unknown manager.
    terminal: bool,
}
pub(super) struct Acquisition {
    pub(super) requested: Vec<String>,
    pub(super) rows: Vec<Value>,
    pub(super) failed: Vec<String>,
    pub(super) disconnected: Vec<String>,
    pub(super) interactions: BTreeMap<String, bool>,
    // Includes each original failed_ids separately from source fetchMembers'
    // final failedIds (which aggregates invalid IDs and exhausted pending IDs).
    rounds: Vec<Vec<Chunk>>,
}

pub(super) async fn acquire(
    state: Signal<ShellState>,
    q: &Query,
    requested: Vec<String>,
) -> Result<Acquisition, Error> {
    acquire_using(state, q, requested, None).await
}
pub(super) async fn acquire_absent(
    state: Signal<ShellState>,
    q: &Query,
    capture: &Capture,
) -> Result<Acquisition, Error> {
    acquire_using(state, q, capture.ids().to_vec(), Some(capture)).await
}
async fn acquire_using(
    state: Signal<ShellState>,
    q: &Query,
    requested: Vec<String>,
    absent: Option<&Capture>,
) -> Result<Acquisition, Error> {
    if !q.current(&state.peek())
        || q.fixture
        || crate::fixture::is_fixture()
        || requested.is_empty()
        || requested.iter().collect::<BTreeSet<_>>().len() != requested.len()
    {
        return Err(unavailable());
    }
    for chunk in requested.chunks(100) {
        Read::capture(chunk.to_vec())?;
    }
    let endpoint = Rc::new(RefCell::new(None));
    let mut result = Acquisition {
        requested: requested.clone(),
        rows: vec![],
        failed: vec![],
        disconnected: vec![],
        interactions: BTreeMap::new(),
        rounds: vec![],
    };
    let mut ids = requested;
    for round in 0..4 {
        let chunks = if let Some(capture) = absent {
            capture.pending_reads(&ids)?
        } else {
            ids.chunks(100)
                .map(|ids| Read::capture(ids.to_vec()))
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut completed = Vec::new();
        // Source concurrency2, retaining chunk order instead of completion order.
        for pair in chunks.chunks(2) {
            let (first, second) =
                tokio::join!(chunk(state, q, pair[0].clone(), endpoint.clone()), async {
                    if let Some(read) = pair.get(1) {
                        Some(chunk(state, q, read.clone(), endpoint.clone()).await)
                    } else {
                        None
                    }
                });
            completed.push(first);
            completed.extend(second);
            if !q.current(&state.peek()) {
                return Err(unavailable());
            }
        }
        ids = result.append_round(completed);
        if ids.is_empty() {
            return Ok(result);
        }
        if round == 3 {
            result.failed.extend(ids);
            return Ok(result);
        }
        // Actual pending schedule, not a retry of a mutation or unknown write.
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(500 * (round + 1))) => {},
            _ = q.retired(state) => return Err(unavailable()),
        }
        if !q.current(&state.peek()) {
            return Err(unavailable());
        }
    }
    unreachable!("four pending rounds return above")
}
impl Acquisition {
    pub(super) fn terminal(&self) -> bool {
        self.rounds.iter().flatten().all(|c| c.terminal)
    }
    fn append_round(&mut self, chunks: Vec<Chunk>) -> Vec<String> {
        let mut pending = Vec::new();
        for chunk in &chunks {
            if chunk.error.is_some() {
                // Actual fetchObjectsById catch classifies this whole chunk;
                // partial page receipts stay retained but aren't upserted.
                self.failed.extend(chunk.requested.iter().cloned());
                continue;
            }
            for page in &chunk.pages {
                self.rows.extend(page.rows().iter().cloned());
                self.failed.extend(page.invalid_ids().iter().cloned());
                self.disconnected
                    .extend(page.disconnected_ids().iter().cloned());
                self.interactions.extend(page.interactions().clone());
                pending.extend(page.pending_ids().iter().cloned());
            }
        }
        self.rounds.push(chunks);
        pending
    }
}
async fn chunk(
    mut state: Signal<ShellState>,
    q: &Query,
    read: Read,
    endpoint: Rc<RefCell<Option<Rc<DirectoryEndpoint>>>>,
) -> Chunk {
    let mut result = Chunk {
        requested: read.ids().to_vec(),
        pages: vec![],
        error: None,
        terminal: false,
    };
    let answer = pages(
        state,
        q,
        read,
        endpoint,
        &mut result.pages,
        &mut result.terminal,
    )
    .await;
    if let Err(Error::RateLimited { retry_after_secs }) = &answer {
        navigation::retain_limit(&mut state.write(), &q.owner.ctx, *retry_after_secs);
    }
    result.error = answer.err();
    result
}
async fn pages(
    state: Signal<ShellState>,
    q: &Query,
    mut read: Read,
    endpoint: Rc<RefCell<Option<Rc<DirectoryEndpoint>>>>,
    out: &mut Vec<Page>,
    terminal: &mut bool,
) -> Result<(), Error> {
    let ctx = &q.owner.ctx;
    let mut markers = BTreeSet::new();
    for _ in 0..50 {
        if !q.can_dispatch(&state.peek()) {
            return Err(unavailable());
        }
        let current = endpoint
            .borrow()
            .as_ref()
            .filter(|e| e.current(&ctx.session, Instant::now()))
            .cloned();
        let current = match current {
            Some(value) => value,
            None => {
                let observed = Instant::now();
                let raw = read::request(
                    state,
                    q,
                    DirectoryEndpoint::discovery(&ctx.client, &ctx.session)?,
                )
                .await?;
                let value = Rc::new(DirectoryEndpoint::decode(&ctx.session, &raw, observed)?);
                *endpoint.borrow_mut() = Some(value.clone());
                value
            }
        };
        if !q.can_dispatch(&state.peek()) {
            return Err(unavailable());
        }
        let request = current.member_info(&ctx.client, &ctx.session, Instant::now(), &read)?;
        native::query(state, q).await?;
        let response = tokio::select! {
            // Preserve an already completed error/response before observing
            // retirement. Dropping this consumer-owned read never cancels
            // the root-lived unknown-member manager's separate IO.
            biased;
            response = ctx.transport.execute_directory_member_info_current(request, || {
                dispatch_current(&state.peek(), q, &current, Instant::now())
            }) => response,
            _ = q.retired(state) => return Err(unavailable()),
        };
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                // A server answer is a real failed chunk. Offline/not-sent and
                // unclassified transport failures are never missing-member evidence.
                *terminal = matches!(
                    error,
                    Error::Api(_) | Error::HttpStatus { .. } | Error::RateLimited { .. }
                );
                return Err(error);
            }
        };
        if !q.current(&state.peek()) || !current.current(&ctx.session, Instant::now()) {
            return Err(unavailable());
        }
        let (raw, keys) = response.into_parts();
        let page = Page::decode(&read, raw, keys)?;
        let next = page.next_marker().map(str::to_owned);
        out.push(page);
        native::query(state, q).await?;
        let Some(next) = next else {
            *terminal = true;
            return Ok(());
        };
        if !markers.insert(next.clone()) {
            return Err(unavailable());
        }
        read = read.next_page(&next)?;
    }
    // Native resource bound is an explicit failed chunk, never truncated success.
    Err(unavailable())
}

// Rechecked by Transport both before and after its offline hold. Cooldown
// prevents a new dispatch, but is deliberately absent from post-response
// ownership checks so an already-issued sibling success remains usable.
fn dispatch_current(s: &ShellState, q: &Query, endpoint: &DirectoryEndpoint, now: Instant) -> bool {
    q.can_dispatch(s) && endpoint.current(&q.owner.ctx.session, now)
}
