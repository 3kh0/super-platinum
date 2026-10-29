//! Numbered page storage and entry sessions. Account, parameter, rendered-query
//! and permission observations belong to the caller; this model grants none.
use crate::slack::api::people_pages::Page;
use std::{collections::BTreeMap, sync::Arc};
use uuid::Uuid;

/// Native resident-cache bound, not a Slack page-count limit. Evicted pages can
/// be fetched again. The visible page remains available across request failure.
const MAX_CACHED_PAGES: usize = 8;

#[derive(Clone)]
pub struct Ticket {
    pub page: u32,
    nonce: Arc<()>,
}
pub enum Plan {
    Cached,
    Fetch(Ticket),
    Refused,
}
pub struct Activation {
    pub page: Arc<Page>,
    nonce: Arc<()>,
}
impl Clone for Activation {
    fn clone(&self) -> Self {
        Self {
            page: self.page.clone(),
            nonce: self.nonce.clone(),
        }
    }
}
#[derive(Default)]
pub struct Book {
    pages: BTreeMap<u32, Arc<Page>>,
    active: Option<Activation>,
    pending: Option<Ticket>,
    failed: Option<Ticket>,
    page_count: Option<u32>,
    total: u64,
    last: Option<serde_json::Value>,
}
impl Book {
    pub fn active(&self) -> Option<&Activation> {
        self.active.as_ref()
    }
    pub fn page_count(&self) -> u32 {
        self.page_count.unwrap_or(0)
    }
    pub fn total(&self) -> u64 {
        self.total
    }
    pub fn last(&self) -> Option<&serde_json::Value> {
        self.last.as_ref()
    }
    pub fn pending(&self) -> Option<&Ticket> {
        self.pending.as_ref()
    }
    pub fn failed_page(&self) -> Option<u32> {
        self.failed.as_ref().map(|r| r.page)
    }
    pub fn failure(&self) -> Option<&Ticket> {
        self.failed.as_ref()
    }
    pub fn failure_current(&self, ticket: &Ticket) -> bool {
        self.failed
            .as_ref()
            .is_some_and(|r| r.page == ticket.page && Arc::ptr_eq(&r.nonce, &ticket.nonce))
    }
    pub fn retry(&mut self, ticket: &Ticket) -> Plan {
        if !self.failure_current(ticket) {
            return Plan::Refused;
        }
        self.select(ticket.page)
    }
    pub fn select(&mut self, page: u32) -> Plan {
        if page == 0 || page > self.page_count.unwrap_or(1).max(1) {
            return Plan::Refused;
        }
        if let Some(value) = self.pages.get(&page) {
            self.active = Some(Activation {
                page: value.clone(),
                nonce: Arc::new(()),
            });
            self.pending = None;
            self.failed = None;
            return Plan::Cached;
        }
        if self.pending.as_ref().is_some_and(|r| r.page == page) {
            return Plan::Refused;
        }
        // A navigation intent retires old selections immediately. Keep the
        // loaded page visible, but a failed request must not revive old receipts.
        if let Some(active) = &mut self.active {
            active.nonce = Arc::new(());
        }
        let ticket = Ticket {
            page,
            nonce: Arc::new(()),
        };
        self.pending = Some(ticket.clone());
        self.failed = None;
        Plan::Fetch(ticket)
    }
    pub fn accepts(&self, ticket: &Ticket) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|r| r.page == ticket.page && Arc::ptr_eq(&r.nonce, &ticket.nonce))
    }
    pub fn activation_current(&self, activation: &Activation) -> bool {
        self.pending.is_none()
            && self.active.as_ref().is_some_and(|a| {
                Arc::ptr_eq(&a.nonce, &activation.nonce) && Arc::ptr_eq(&a.page, &activation.page)
            })
    }
    pub fn commit(&mut self, ticket: &Ticket, page: Page) -> bool {
        if !self.accepts(ticket) || page.number() != ticket.page {
            return false;
        }
        if self.page_count.is_none() {
            if page.number() != 1 {
                return false;
            }
            self.page_count = Some(page.page_count());
        }
        // Source append preserves the original page_count, updates only last
        // and total_count, and stores each untouched array in its own page slot.
        self.total = page.total();
        self.last = page
            .raw()
            .get("pagination")
            .and_then(|p| p.get("last"))
            .cloned();
        let page = Arc::new(page);
        self.pages.insert(page.number(), page.clone());
        self.active = Some(Activation {
            page,
            nonce: Arc::new(()),
        });
        self.pending = None;
        self.failed = None;
        while self.pages.len() > MAX_CACHED_PAGES {
            let number = self.active.as_ref().unwrap().page.number();
            let oldest = self.pages.keys().copied().find(|n| *n != number).unwrap();
            self.pages.remove(&oldest);
        }
        true
    }
    pub fn fail(&mut self, ticket: &Ticket) -> bool {
        if !self.accepts(ticket) {
            return false;
        }
        self.pending = None;
        self.failed = Some(ticket.clone());
        true
    }
}

#[derive(Default)]
pub struct Sessions {
    query: Option<Uuid>,
    search: Option<Uuid>,
    last_event_ms: Option<u64>,
    last_query_start_ms: Option<u64>,
}
pub struct Entry {
    pub query: Uuid,
    pub search: Uuid,
    pub latest_api: Uuid,
}
impl Sessions {
    /// Port of module entry order: query getter, search getter, fresh API id.
    /// Both getters use the SAME last-event cell. A cold search starts another
    /// query after the caller captured its earlier query id; preserve that order.
    pub fn enter(&mut self, now_ms: u64, new_id: impl FnMut() -> Uuid) -> Entry {
        self.enter_with(now_ms, new_id)
    }
    /// Actual explicit filter producer: ID, then the two separate clock reads.
    /// This is owned lifecycle data, never permission/currentness authority.
    pub fn start_query(
        &mut self,
        mut clock: impl FnMut() -> u64,
        mut new_id: impl FnMut() -> Uuid,
    ) -> Uuid {
        let query = new_id();
        self.query = Some(query);
        self.last_event_ms = Some(clock());
        self.last_query_start_ms = Some(clock());
        query
    }
    /// Visible-People native adaptation of Source's first-all entry ordering.
    /// It does not claim eager hidden-module requests or analytics/URL effects.
    pub fn enter_first_all(&mut self, now_ms: u64, mut new_id: impl FnMut() -> Uuid) -> Entry {
        let latest_api = new_id();
        let (query, search) = self.pair(now_ms, &mut new_id);
        Entry {
            query,
            search,
            latest_api,
        }
    }
    fn enter_with(&mut self, now_ms: u64, mut new_id: impl FnMut() -> Uuid) -> Entry {
        let (query, search) = self.pair(now_ms, &mut new_id);
        Entry {
            query,
            search,
            latest_api: new_id(),
        }
    }
    fn pair(&mut self, now_ms: u64, mut new_id: impl FnMut() -> Uuid) -> (Uuid, Uuid) {
        let missing_time = self.last_event_ms.is_none_or(|t| t == 0);
        if self.query.is_none()
            || missing_time
            || now_ms.saturating_sub(self.last_event_ms.unwrap_or(0)) > 120_000
        {
            self.query = Some(new_id());
            self.last_query_start_ms = Some(now_ms);
        }
        self.last_event_ms = Some(now_ms);
        let query = self.query.unwrap();
        if self.search.is_none()
            || self.last_event_ms.is_none_or(|t| t == 0)
            || now_ms.saturating_sub(self.last_event_ms.unwrap_or(0)) > 300_000
        {
            self.search = Some(new_id());
            self.query = Some(new_id());
            self.last_query_start_ms = Some(now_ms);
        }
        self.last_event_ms = Some(now_ms);
        (query, self.search.unwrap())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod filter_tests;
