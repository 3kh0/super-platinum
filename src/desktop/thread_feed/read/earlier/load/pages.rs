//! A continuation can only come from actual raw pages in the original ticket.
//! This is a bounded native full-body adapter, not Source history/store equality.
use super::*;
use std::collections::BTreeSet;
const MAX_PAGES: usize = 16;
struct Page {
    rows: BTreeMap<String, Message>,
    root: bool,
    more: bool,
    cursor: String,
}
fn page(raw: &Value, action: &Action) -> Result<Page, Error> {
    let seed = &action.scope.windows[&action.key].seed;
    let boundary = &seed.ids[0];
    if raw.get("ok") != Some(&Value::Bool(true)) {
        return Err(invalid());
    }
    let more = raw
        .get("has_more")
        .and_then(Value::as_bool)
        .ok_or_else(invalid)?;
    let cursor = raw
        .get("response_metadata")
        .and_then(|v| v.get("next_cursor"))
        .and_then(Value::as_str)
        .ok_or_else(invalid)?
        .to_owned();
    if cursor.len() > 4096
        || raw
            .get("unchanged_messages")
            .is_some_and(|v| v.as_array().is_none_or(|a| !a.is_empty()))
        || raw.get("deleted").is_some_and(|v| {
            !v.is_null() && v != &Value::Bool(false) && !v.as_array().is_some_and(|a| a.is_empty())
        })
        || ["limited", "is_limited"].iter().any(|k| {
            raw.get(*k)
                .is_some_and(|v| v != &Value::Bool(false) && v != &Value::Null)
        })
    {
        return Err(invalid());
    }
    let messages = raw
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    if messages.is_empty() || messages.len() > 100 {
        return Err(invalid());
    }
    let mut rows = BTreeMap::new();
    let mut root = false;
    let mut previous: Option<String> = None;
    for value in messages {
        let message: Message = serde_json::from_value(value.clone()).map_err(|_| invalid())?;
        let ts = message.ts.clone().ok_or_else(invalid)?;
        let is_root = ts == action.key.1;
        if !seed::ordinary(&message, &action.source.team, &action.key, is_root)
            || ts.as_str() > boundary.as_str()
        {
            return Err(invalid());
        }
        if previous.as_ref().is_some_and(|p| p >= &ts) {
            return Err(invalid());
        }
        previous = Some(ts.clone());
        if is_root {
            if root
                || message.reply_count != Some(seed.count)
                || message.latest_reply.as_ref() != Some(&seed.latest)
                || message
                    .extra
                    .get("subscribed")
                    .is_some_and(|v| v != &Value::Bool(true))
            {
                return Err(invalid());
            }
            root = true;
        } else if rows.insert(ts, message).is_some() {
            return Err(invalid());
        }
    }
    Ok(Page {
        rows,
        root,
        more,
        cursor,
    })
}
pub(super) struct Collector {
    rows: BTreeMap<String, Message>,
    last: String,
    cursor: String,
    seen: BTreeSet<String>,
    pages: usize,
}
pub(super) enum Step {
    Next,
    Complete(Vec<Message>),
    Fallback,
}
impl Collector {
    pub(super) fn start(raw: &Value, action: &Action) -> Result<Option<Self>, Error> {
        let seed = &action.scope.windows[&action.key].seed;
        if seed.count > 200
            || raw.get("has_more") != Some(&Value::Bool(true))
            || raw
                .get("response_metadata")
                .and_then(|v| v.get("next_cursor"))
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
        {
            return Ok(None);
        }
        // Do not reinterpret the original contradictory/incomplete fallback:
        // a page already containing the inclusive upper boundary cannot supply
        // a positively advancing continuation in this earliest-first adapter.
        if raw
            .get("messages")
            .and_then(Value::as_array)
            .is_some_and(|a| {
                a.iter()
                    .any(|m| m.get("ts").and_then(Value::as_str) == Some(seed.ids[0].as_str()))
            })
        {
            return Ok(None);
        }
        let p = page(raw, action)?;
        if !p.root || p.rows.is_empty() {
            return Ok(None);
        }
        let mut collector = Self {
            rows: BTreeMap::new(),
            last: action.key.1.clone(),
            cursor: p.cursor.clone(),
            seen: BTreeSet::from([p.cursor]),
            pages: 1,
        };
        collector.insert(p.rows, action)?;
        Ok(Some(collector))
    }
    fn insert(&mut self, rows: BTreeMap<String, Message>, action: &Action) -> Result<(), Error> {
        if rows.is_empty() {
            return Err(invalid());
        }
        for (ts, message) in rows {
            if ts <= self.last || self.rows.insert(ts.clone(), message).is_some() {
                return Err(invalid());
            }
            self.last = ts;
        }
        let seed = &action.scope.windows[&action.key].seed;
        let overlap = usize::from(self.rows.contains_key(&seed.ids[0]));
        let combined = self.rows.len() + seed.ids.len() - overlap;
        if combined > 200 || combined > seed.count as usize {
            return Err(invalid());
        }
        Ok(())
    }
    pub(super) fn cursor(&self) -> String {
        self.cursor.clone()
    }
    pub(super) fn accept(&mut self, raw: Value, action: &Action) -> Result<Step, Error> {
        let p = page(&raw, action)?;
        self.pages += 1;
        self.insert(p.rows, action)?;
        let boundary = &action.scope.windows[&action.key].seed.ids[0];
        if p.more {
            if p.cursor.is_empty()
                || self.rows.contains_key(boundary)
                || self.pages >= MAX_PAGES
                || !self.seen.insert(p.cursor.clone())
            {
                return Ok(Step::Fallback);
            }
            self.cursor = p.cursor;
            return Ok(Step::Next);
        }
        if !p.cursor.is_empty() {
            return Ok(Step::Fallback);
        }
        if self.rows.remove(boundary).is_none() || self.rows.is_empty() {
            return Err(invalid());
        }
        Ok(Step::Complete(
            std::mem::take(&mut self.rows).into_values().collect(),
        ))
    }
}
