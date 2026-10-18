//! D8Wa raw fetch: ordered directory pages, failed-ID fallback, first row wins.
use super::*;
use crate::profile_edit::sections::fixture::group_recovery as fixture;
use std::{
    future::{Future, poll_fn},
    task::Poll,
    time::Instant,
};
use super_platinum_core::{
    block_actions::directory_endpoint::{
        DirectoryEndpoint,
        usergroup_info::{Fallback, FallbackPage, Page, Read},
    },
    slack::{
        client::PreparedRequest,
        transport::{ChannelReadAttempt, ChannelReadResponse},
    },
};

pub(super) struct Received {
    pub(super) rows: Vec<UserGroup>,
    // Original successful response payloads and failures survive until adoption.
    pub(super) raw: Vec<Value>,
    pub(super) errors: Vec<Arc<Error>>,
}
#[derive(Default)]
struct Part {
    rows: Vec<Value>,
    failed: Vec<String>,
    raw: Vec<Value>,
    errors: Vec<Arc<Error>>,
}
impl Part {
    fn page(&mut self, page: &Page) {
        self.rows.extend_from_slice(page.rows());
        self.failed.extend_from_slice(page.failed_ids());
        // D8Wa deliberately does not turn pending/invalid into fallback IDs.
    }
    fn append(&mut self, other: Self) {
        self.rows.extend(other.rows);
        self.failed.extend(other.failed);
        self.raw.extend(other.raw);
        self.errors.extend(other.errors);
    }
    fn finish(mut self) -> Received {
        let mut seen = BTreeSet::new();
        let mut rows = Vec::new();
        for raw in self.rows {
            let Some(id) = raw["id"].as_str() else {
                self.errors.push(Arc::new(unavailable()));
                continue;
            };
            if !seen.insert(id.to_owned()) {
                continue;
            }
            match UserGroup::from_observed(&raw) {
                Ok(row) => rows.push(row),
                Err(_) => self.errors.push(Arc::new(unavailable())),
            }
        }
        Received {
            rows,
            raw: self.raw,
            errors: self.errors,
        }
    }
}
type Endpoint = tokio::sync::Mutex<Option<Arc<DirectoryEndpoint>>>;
fn current(
    state: Signal<ShellState>,
    attempt: &Attempt,
    consumer: &impl Fn(&ShellState) -> bool,
) -> bool {
    attempt.current(&state.peek(), consumer)
}
fn report(
    mut state: Signal<ShellState>,
    attempt: &Attempt,
    consumer: &impl Fn(&ShellState) -> bool,
    error: &Error,
) {
    let mut shell = state.write();
    if attempt.current(&shell, consumer) {
        shell.report_failure(error, "", || "Group names could not be refreshed.".into());
    }
}
async fn discovery(
    state: Signal<ShellState>,
    attempt: &Attempt,
    consumer: &impl Fn(&ShellState) -> bool,
    shared: &Endpoint,
) -> Result<Arc<DirectoryEndpoint>, Error> {
    let mut slot = shared.lock().await;
    let owner = &attempt.request.owner;
    if !current(state, attempt, consumer) {
        return Err(unavailable());
    }
    if let Some(endpoint) = slot
        .as_ref()
        .filter(|e| e.current(&owner.session, Instant::now()))
    {
        return Ok(endpoint.clone());
    }
    let observed = Instant::now();
    let result = if crate::fixture::is_fixture() {
        let request = DirectoryEndpoint::discovery(&owner.client, &owner.session)?;
        fixture::request(state, owner, &request, "discovery").await
    } else {
        let request = ChannelReadAttempt::discovery(&owner.client, &owner.session)?;
        match owner
            .transport
            .execute_channel_read_current(request, || current(state, attempt, consumer))
            .await
        {
            Ok(ChannelReadResponse::Setup(raw)) => Ok(raw),
            Ok(_) => Err(unavailable()),
            Err(error) => Err(error),
        }
    };
    if !current(state, attempt, consumer) {
        return Err(unavailable());
    }
    let endpoint = Arc::new(DirectoryEndpoint::decode(
        &owner.session,
        &result?,
        observed,
    )?);
    *slot = Some(endpoint.clone());
    Ok(endpoint)
}
async fn directory_pages(
    state: Signal<ShellState>,
    attempt: &Attempt,
    consumer: &impl Fn(&ShellState) -> bool,
    mut read: Read,
    shared: &Endpoint,
) -> Result<Part, (Error, Vec<Value>)> {
    let owner = &attempt.request.owner;
    let mut part = Part::default();
    let mut markers = BTreeSet::new();
    loop {
        let result = async {
            let endpoint = discovery(state, attempt, consumer, shared).await?;
            if !current(state, attempt, consumer) {
                return Err(unavailable());
            }
            let request =
                endpoint.usergroup_info(&owner.client, &owner.session, Instant::now(), &read)?;
            let response = if crate::fixture::is_fixture() {
                fixture::request(state, owner, &request, "directory")
                    .await
                    .map(|raw| (raw, None))
            } else {
                owner
                    .transport
                    .execute_directory_usergroup_info_current(request, || {
                        current(state, attempt, consumer)
                    })
                    .await
                    .map(|raw| raw.into_parts())
            };
            if !current(state, attempt, consumer) {
                return Err(unavailable());
            }
            let (raw, keys) = response?;
            part.raw.push(raw.clone());
            Page::decode(&read, raw, keys)
        }
        .await;
        let page = match result {
            Ok(page) => page,
            Err(error) => return Err((error, part.raw)),
        };
        part.page(&page);
        let Some(marker) = page.next_marker() else {
            return Ok(part);
        };
        if !markers.insert(marker.to_owned()) {
            return Err((unavailable(), part.raw));
        }
        read = match read.next_page(marker) {
            Ok(read) => read,
            Err(error) => return Err((error, part.raw)),
        };
    }
}
async fn directory_chunk(
    state: Signal<ShellState>,
    attempt: &Attempt,
    consumer: &impl Fn(&ShellState) -> bool,
    read: Read,
    shared: &Endpoint,
) -> Part {
    let ids = read.ids().to_vec();
    match directory_pages(state, attempt, consumer, read, shared).await {
        Ok(part) => part,
        Err((error, raw)) => {
            report(state, attempt, consumer, &error);
            // 2gUC rejects the entire chunk, including earlier successful pages.
            Part {
                failed: ids,
                raw,
                errors: vec![Arc::new(error)],
                ..Default::default()
            }
        }
    }
}
async fn direct(
    state: Signal<ShellState>,
    attempt: &Attempt,
    consumer: &impl Fn(&ShellState) -> bool,
    read: Fallback,
) -> Part {
    let owner = &attempt.request.owner;
    let mut raw_evidence = Vec::new();
    let result = async {
        if !current(state, attempt, consumer) {
            return Err(unavailable());
        }
        let request: PreparedRequest = read.request(&owner.client, &owner.session)?;
        let response = if crate::fixture::is_fixture() {
            fixture::request(state, owner, &request, "direct").await
        } else {
            owner
                .transport
                .execute_usergroup_fallback_current(request, || current(state, attempt, consumer))
                .await
        };
        if !current(state, attempt, consumer) {
            return Err(unavailable());
        }
        let raw = response?;
        raw_evidence.push(raw.clone());
        FallbackPage::decode(&read, raw)
    }
    .await;
    match result {
        Ok(page) => Part {
            rows: page.rows().to_vec(),
            raw: raw_evidence,
            ..Default::default()
        },
        Err(error) => {
            report(state, attempt, consumer, &error);
            Part {
                raw: raw_evidence,
                errors: vec![Arc::new(error)],
                ..Default::default()
            }
        }
    }
}

// Source fallback uses Promise.all (no directory two-chunk cap). Keep ordered
// outputs without spawning detached tasks or adding a dependency/supervisor.
async fn all<F: Future>(futures: Vec<F>) -> Vec<F::Output> {
    let mut work: Vec<_> = futures
        .into_iter()
        .map(|future| Some(Box::pin(future)))
        .collect();
    let mut done: Vec<Option<F::Output>> = (0..work.len()).map(|_| None).collect();
    poll_fn(|cx| {
        let mut pending = false;
        for (index, slot) in work.iter_mut().enumerate() {
            if let Some(future) = slot {
                match future.as_mut().poll(cx) {
                    Poll::Ready(value) => {
                        done[index] = Some(value);
                        *slot = None;
                    }
                    Poll::Pending => pending = true,
                }
            }
        }
        if pending {
            Poll::Pending
        } else {
            Poll::Ready(
                done.iter_mut()
                    .map(|value| value.take().expect("completed future"))
                    .collect(),
            )
        }
    })
    .await
}

pub(super) async fn acquire(
    state: Signal<ShellState>,
    attempt: &Attempt,
    consumer: &impl Fn(&ShellState) -> bool,
) -> Result<Received, Error> {
    let reads = attempt
        .ids
        .chunks(100)
        .map(|ids| Read::capture(ids.to_vec()))
        .collect::<Result<Vec<_>, _>>()?;
    let endpoint = Endpoint::default();
    let mut aggregate = Part::default();
    for pair in reads.chunks(2) {
        if !current(state, attempt, consumer) {
            return Err(unavailable());
        }
        let (a, b) = tokio::join!(
            directory_chunk(state, attempt, consumer, pair[0].clone(), &endpoint),
            async {
                match pair.get(1) {
                    Some(read) => Some(
                        directory_chunk(state, attempt, consumer, read.clone(), &endpoint).await,
                    ),
                    None => None,
                }
            }
        );
        if !current(state, attempt, consumer) {
            return Err(unavailable());
        }
        aggregate.append(a);
        if let Some(b) = b {
            aggregate.append(b);
        }
    }
    let mut seen = BTreeSet::new();
    let failed: Vec<_> = aggregate
        .failed
        .iter()
        .filter(|id| seen.insert((*id).clone()))
        .cloned()
        .collect();
    let fallbacks = failed
        .chunks(250)
        .map(|ids| Fallback::capture(ids.to_vec(), "fetchRawUserGroupsById"))
        .collect::<Result<Vec<_>, _>>()?;
    let outcomes = all(fallbacks
        .into_iter()
        .map(|read| direct(state, attempt, consumer, read))
        .collect())
    .await;
    if !current(state, attempt, consumer) {
        return Err(unavailable());
    }
    for part in outcomes {
        aggregate.append(part);
    }
    Ok(aggregate.finish())
}

#[cfg(test)]
mod tests;
