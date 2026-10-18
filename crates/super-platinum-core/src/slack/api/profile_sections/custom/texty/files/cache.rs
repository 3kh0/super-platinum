//! keuO cache operations. The owner keeps one cache per original native account
//! and enterprise || team scope, outside individual queries/Profile visits.
//! Time is supplied in Date.now milliseconds; no timers or IO run here.
use std::{collections::BTreeSet, sync::Arc};

const EXACT_MS: i128 = 28_800_000;
const PREFIX_MS: i128 = 900_000;
const IN_FLIGHT_MS: i128 = 400;

#[derive(Clone, Debug)]
pub struct RequestToken(Arc<()>);

#[derive(Clone)]
pub struct QueryCache {
    exact: BTreeSet<String>,
    prefixes: BTreeSet<String>,
    updated_ms: i64,
    prefix_updated_ms: i64,
    in_flight: Option<(RequestToken, i64)>,
}
impl QueryCache {
    pub fn new(now_ms: i64) -> Self {
        Self {
            exact: BTreeSet::new(),
            prefixes: BTreeSet::new(),
            updated_ms: now_ms,
            prefix_updated_ms: now_ms,
            in_flight: None,
        }
    }
    fn clear_if_stale(&mut self, now_ms: i64) {
        if i128::from(now_ms) - i128::from(self.updated_ms) > EXACT_MS {
            self.exact.clear();
            self.updated_ms = now_ms;
        }
    }
    fn clear_prefix_if_stale(&mut self, now_ms: i64) {
        if i128::from(now_ms) - i128::from(self.prefix_updated_ms) > PREFIX_MS {
            self.prefixes.clear();
            self.prefix_updated_ms = now_ms;
        }
    }
    /// Source quirk: report the old membership, THEN clear if stale.
    pub fn contains(&mut self, query: &str, now_ms: i64) -> bool {
        let found = self.exact.contains(query);
        self.clear_if_stale(now_ms);
        found
    }
    pub fn contains_prefix_of(&mut self, query: &str, now_ms: i64) -> bool {
        self.clear_prefix_if_stale(now_ms);
        // hCtU walks at least one character before testing its terminal marker.
        self.prefixes
            .iter()
            .any(|prefix| !prefix.is_empty() && query.starts_with(prefix))
    }
    pub fn add(&mut self, query: &str, now_ms: i64) {
        self.clear_if_stale(now_ms);
        self.exact.insert(query.into());
    }
    pub fn add_as_prefix(&mut self, query: &str, now_ms: i64) {
        self.clear_prefix_if_stale(now_ms);
        self.prefixes.insert(query.into());
    }
    pub fn set_request_in_flight(&mut self, now_ms: i64) -> RequestToken {
        let token = RequestToken(Arc::new(()));
        self.in_flight = Some((token.clone(), now_ms));
        token
    }
    pub fn clear_request_in_flight(&mut self, token: &RequestToken) {
        if self
            .in_flight
            .as_ref()
            .is_some_and(|(current, _)| Arc::ptr_eq(&current.0, &token.0))
        {
            self.in_flight = None;
        }
    }
    pub fn is_request_in_flight(&self, now_ms: i64) -> bool {
        self.in_flight.as_ref().is_some_and(|(_, start)| {
            *start != 0 && i128::from(now_ms) - i128::from(*start) < IN_FLIGHT_MS
        })
    }
    /// 20Mt success, BEFORE upsert. Errors/cancellation do not call this as an
    /// empty result. A prefix-cache short circuit is not an HTTP response.
    pub fn record_response(
        &mut self,
        query: &str,
        count: usize,
        token: Option<&RequestToken>,
        aborted: bool,
        now_ms: i64,
    ) {
        if !aborted && count < 10 {
            self.add_as_prefix(query, now_ms);
            if let Some(token) = token {
                self.clear_request_in_flight(token);
            }
        }
    }
    /// qKv8 success AFTER upsert, using the original (not rewritten) query.
    pub fn record_upsert(&mut self, query: &str, count: usize, aborted: bool, now_ms: i64) {
        if !aborted && count > 0 {
            self.add(query, now_ms);
        }
    }
    pub fn reset(&mut self, now_ms: i64) {
        *self = Self::new(now_ms);
    }
}
