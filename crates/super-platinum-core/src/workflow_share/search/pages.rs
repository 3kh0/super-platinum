//! Share's count24 variant of 2gUC; reuse the neutral same-byte Page decoder.
use super::*;
use std::collections::BTreeSet;
#[derive(Clone, Default)]
pub struct Pages {
    pages: Vec<Page>,
    rows: Vec<Value>,
    next: Option<String>,
    seen: BTreeSet<String>,
    outcomes: Outcomes,
    complete: bool,
    failed: bool,
}
impl Pages {
    pub fn accept(&mut self, raw: Value, keys: Option<&[String]>) -> Result<(), Error> {
        match Page::decode_ordered(raw, keys) {
            Ok(page) => self.append(page),
            Err(e) => {
                self.fail();
                Err(e)
            }
        }
    }
    /// A transport failure also poisons the aggregate; issued raw receipts stay
    /// caller-owned. No partial canonical publication can use rows/outcomes.
    pub fn fail(&mut self) {
        self.failed = true;
    }
    pub fn append(&mut self, page: Page) -> Result<(), Error> {
        if self.failed || self.complete || self.pages.len() >= 128 {
            self.fail();
            return Err(invalid());
        }
        let count = self.rows.len() + page.rows().len();
        let complete = page.marker().is_none() || count >= LIMIT;
        let next = if count > LIMIT {
            page.rows()
                .get(LIMIT - self.rows.len())
                .and_then(|r| r["id"].as_str())
                .map(str::to_owned)
        } else {
            page.marker().map(str::to_owned)
        };
        if !complete
            && (self.pages.len() == 127 || next.as_ref().is_some_and(|m| self.seen.contains(m)))
        {
            self.fail();
            return Err(invalid());
        }
        if let Some(m) = &next {
            self.seen.insert(m.clone());
        }
        self.rows
            .extend(page.rows().iter().take(LIMIT - self.rows.len()).cloned());
        let o = page.outcomes();
        self.outcomes
            .failed_ids
            .extend(o.failed_ids.iter().cloned());
        self.outcomes
            .pending_ids
            .extend(o.pending_ids.iter().cloned());
        self.outcomes
            .invalid_ids
            .extend(o.invalid_ids.iter().cloned());
        self.outcomes
            .membership_ids
            .extend(o.membership_ids.iter().cloned());
        self.outcomes
            .disconnected_ids
            .extend(o.disconnected_ids.iter().cloned());
        self.outcomes.can_interact.extend(o.can_interact.clone());
        self.pages.push(page);
        self.next = next;
        self.complete = complete;
        Ok(())
    }
    pub fn complete(&self) -> bool {
        self.complete && !self.failed
    }
    pub fn marker(&self) -> Option<&str> {
        if self.failed {
            None
        } else {
            self.next.as_deref()
        }
    }
    pub fn pages(&self) -> &[Page] {
        &self.pages
    }
    pub fn rows(&self) -> Result<&[Value], Error> {
        self.complete()
            .then_some(self.rows.as_slice())
            .ok_or_else(invalid)
    }
    pub fn outcomes(&self) -> Result<&Outcomes, Error> {
        self.complete()
            .then_some(&self.outcomes)
            .ok_or_else(invalid)
    }
}
