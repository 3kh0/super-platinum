//! 2gUC fetchObjectsWithQuery: concatenate, then truncate; never deduplicate.
use super::*;
use serde_json::Map;
use std::collections::BTreeSet;

#[derive(Clone, Default, PartialEq)]
pub struct Outcomes {
    pub failed_ids: Vec<String>,
    pub pending_ids: Vec<String>,
    pub invalid_ids: Vec<String>,
    pub membership_ids: Vec<String>,
    pub disconnected_ids: Vec<String>,
    pub can_interact: Map<String, Value>,
}
impl Outcomes {
    fn decode(raw: &Value) -> Result<Self, Error> {
        fn ids(raw: &Value, key: &str) -> Result<Vec<String>, Error> {
            match raw.get(key) {
                None | Some(Value::Null) => Ok(Vec::new()),
                Some(Value::Array(a)) => a
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .filter(|v| !v.is_empty())
                            .map(str::to_owned)
                            .ok_or_else(unavailable)
                    })
                    .collect(),
                _ => Err(unavailable()),
            }
        }
        let can_interact = match raw.get("can_interact") {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(v)) => v.clone(),
            _ => return Err(unavailable()),
        };
        Ok(Self {
            failed_ids: ids(raw, "failed_ids")?,
            pending_ids: ids(raw, "pending_ids")?,
            invalid_ids: ids(raw, "invalid_ids")?,
            membership_ids: ids(raw, "member_channels")?,
            disconnected_ids: ids(raw, "disconnected_ids")?,
            can_interact,
        })
    }
    fn extend(&mut self, next: &Self) {
        self.failed_ids.extend(next.failed_ids.iter().cloned());
        self.pending_ids.extend(next.pending_ids.iter().cloned());
        self.invalid_ids.extend(next.invalid_ids.iter().cloned());
        self.membership_ids
            .extend(next.membership_ids.iter().cloned());
        self.disconnected_ids
            .extend(next.disconnected_ids.iter().cloned());
        self.can_interact.extend(next.can_interact.clone());
    }
}
#[derive(Clone)]
pub struct Page {
    raw: Value,
    rows: Vec<Value>,
    next: Option<String>,
    outcomes: Outcomes,
}
impl Page {
    /// Object results require actual keys from the same response bytes. No
    /// sorted-map iteration may replace JavaScript Object.values order.
    pub fn decode_ordered(raw: Value, keys: Option<&[String]>) -> Result<Self, Error> {
        if !raw.is_object()
            || raw.get("ok").is_some_and(|v| v != true)
            || raw.get("error").is_some_and(|v| !v.is_null())
        {
            return Err(unavailable());
        }
        let mut rows = Vec::new();
        match raw.get("results") {
            None if keys.is_none() => {}
            Some(Value::Array(a)) if keys.is_none() => rows.extend(a.iter().cloned()),
            Some(Value::Object(o)) => {
                let keys = keys.ok_or_else(unavailable)?;
                let unique: BTreeSet<_> = keys.iter().collect();
                if unique.len() != keys.len() || unique != o.keys().collect() {
                    return Err(unavailable());
                }
                for key in keys {
                    let value = &o[key];
                    if let Some(a) = value.as_array() {
                        rows.extend(a.iter().cloned());
                    } else {
                        rows.push(value.clone());
                    }
                }
            }
            _ => return Err(unavailable()),
        }
        for key in ["mpims", "huddles"] {
            match raw.get(key) {
                None | Some(Value::Null) => {}
                Some(Value::Array(a)) => rows.extend(a.iter().cloned()),
                _ => return Err(unavailable()),
            }
        }
        // Defensive admission bounds/shapes, not source filtering by channel kind.
        if rows.len() > 10_000
            || rows.iter().any(|r| {
                !r.is_object()
                    || r["id"]
                        .as_str()
                        .is_none_or(|id| id.is_empty() || id.len() > 4096)
            })
        {
            return Err(unavailable());
        }
        let next = match raw.get("next_marker") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) if s.is_empty() => None,
            Some(Value::String(s)) if marker(s) => Some(s.clone()),
            _ => return Err(unavailable()),
        };
        let outcomes = Outcomes::decode(&raw)?;
        Ok(Self {
            raw,
            rows,
            next,
            outcomes,
        })
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn rows(&self) -> &[Value] {
        &self.rows
    }
    pub fn marker(&self) -> Option<&str> {
        self.next.as_deref()
    }
    pub fn outcomes(&self) -> &Outcomes {
        &self.outcomes
    }
}
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
    /// A refused append poisons this accumulation: partial rows cannot later be
    /// relabelled a complete response. Fresh query/retry needs a fresh Pages.
    pub fn append(&mut self, page: Page) -> Result<(), Error> {
        if self.failed || self.complete || self.pages.len() >= 128 {
            self.failed = true;
            return Err(unavailable());
        }
        let count = self.rows.len() + page.rows.len();
        let complete = page.next.is_none() || count >= LIMIT;
        let next = if count > LIMIT {
            page.rows
                .get(LIMIT - self.rows.len())
                .and_then(|v| v["id"].as_str())
                .map(str::to_owned)
        } else {
            page.next.clone()
        };
        if !complete
            && (self.pages.len() == 127 || next.as_ref().is_some_and(|m| self.seen.contains(m)))
        {
            self.failed = true;
            return Err(unavailable());
        }
        if let Some(next) = &next {
            self.seen.insert(next.clone());
        }
        self.rows
            .extend(page.rows.iter().take(LIMIT - self.rows.len()).cloned());
        self.outcomes.extend(&page.outcomes);
        self.pages.push(page);
        self.next = next;
        self.complete = complete;
        Ok(())
    }
    pub fn complete(&self) -> bool {
        self.complete && !self.failed
    }
    /// Before completion this is the next request marker; after completion it
    /// is the source's overflow-row ID or terminal response next_marker.
    pub fn marker(&self) -> Option<&str> {
        self.next.as_deref()
    }
    pub fn pages(&self) -> &[Page] {
        &self.pages
    }
    pub fn rows(&self) -> Result<&[Value], Error> {
        self.complete()
            .then_some(self.rows.as_slice())
            .ok_or_else(unavailable)
    }
    pub fn outcomes(&self) -> Result<&Outcomes, Error> {
        self.complete()
            .then_some(&self.outcomes)
            .ok_or_else(unavailable)
    }
}
