//! File acquisition runs alongside the ordinary member/group branch. Cache
//! success and canonical publication remain separate, under the original owner.
use super::*;
use std::time::{SystemTime, UNIX_EPOCH};
use super_platinum_core::slack::api::custom::texty::files;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

/// Establish the credential-owned empty collection only from a current query.
/// An existing poisoned or stale collection is never replaced by empty history.
pub(super) fn initialize(
    shell: &mut ShellState,
    search: &Search<impl Host>,
    names: Arc<member_names::Observation>,
    policy: &file_policy::Observation,
) -> Result<(), Error> {
    if !policy.current(shell, search) || !policy.policy().enabled() {
        return Err(unavailable());
    }
    file_source::Read::capture(shell, search, names, now_ms())?;
    Ok(())
}

pub(super) struct Acquired {
    receipt: file_source::Read,
    response: files::OrderedResponse,
}
impl Acquired {
    /// Install between renderer captures, so a file arrival cannot invalidate
    /// the ordinary branch halfway through its own name/collation observation.
    pub(super) fn publish(
        self,
        shell: &mut ShellState,
        search: &Search<impl Host>,
        policy: &file_policy::Observation,
        query: &str,
    ) -> Result<(), Error> {
        if !policy.current(shell, search) {
            return Err(unavailable());
        }
        let published = self.receipt.publish(shell, search, &self.response)?;
        self.receipt.record_upsert(
            shell,
            search,
            &published,
            query,
            self.response.rows().len(),
            now_ms(),
        )
    }
}

pub(super) async fn remote(
    mut state: Signal<ShellState>,
    search: &Search<impl Host>,
    names: Arc<member_names::Observation>,
    policy: &file_policy::Observation,
    query: &str,
) -> Result<Option<Acquired>, Error> {
    let result = read(state, search, names, policy, query).await;
    if let Err(error) = &result {
        search.lease.retain_failure(&mut state.write(), error);
    }
    result
}

async fn read(
    mut state: Signal<ShellState>,
    search: &Search<impl Host>,
    names: Arc<member_names::Observation>,
    policy: &file_policy::Observation,
    query: &str,
) -> Result<Option<Acquired>, Error> {
    if !policy.current(&state.peek(), search) || !names.current(&state.peek(), search) {
        return Err(unavailable());
    }
    if !policy.policy().remote_eligible(query) {
        return Ok(None);
    }
    let receipt = file_source::Read::capture(&mut state.write(), search, names, now_ms())?;
    let cached = receipt.cached(&mut state.write(), search, query, now_ms())?;
    if cached.exact || cached.in_flight {
        return Ok(None);
    }
    let token = receipt.begin_request(&mut state.write(), search, now_ms())?;
    // The original outer search marks in-flight before the fetcher's prefix
    // short circuit. That shortcut is not an HTTP response or a source upsert.
    if cached.prefix {
        return Ok(None);
    }
    if !search.can_dispatch(&state.peek()) {
        return Err(unavailable());
    }
    let request = files::request(&search.context.client, &search.context.session, query);
    let response = if search.fixture {
        tokio::select! {
            biased;
            _ = search.lease.cancelled() => return Err(unavailable()),
            result = search.lease.fixture_file_request(state, &request) => result?,
        }
    } else {
        tokio::select! {
            biased;
            _ = search.lease.cancelled() => return Err(unavailable()),
            result = search.context.transport.execute_profile_file_search_current(request, || {
                search.can_dispatch(&state.peek())
                    && policy.current(&state.peek(), search)
                    && receipt.current(&state.peek(), search)
            }) => result?,
        }
    };
    if !policy.current(&state.peek(), search) || !receipt.current(&state.peek(), search) {
        return Err(unavailable());
    }
    let mut shell = state.write();
    receipt.record_response(
        &mut shell,
        search,
        query,
        response.raw(),
        response.rows(),
        Some(&token),
        now_ms(),
    )?;
    Ok(Some(Acquired { receipt, response }))
}
