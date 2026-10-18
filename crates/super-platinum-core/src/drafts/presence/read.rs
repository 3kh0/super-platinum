use super::*;
use std::collections::BTreeSet;

#[derive(Debug, PartialEq)]
pub enum Step {
    More(String),
    Done,
}
/// Supersession + revision fence for one caller-owned acquisition. Query,
/// credential, transport and realtime guards remain the desktop caller's duty.
pub struct Read {
    generation: u64,
    revision: u64,
    initial: Vec<Record>,
    total: usize,
    active_cursors: BTreeSet<String>,
    ids_cursors: BTreeSet<String>,
    ids: BTreeSet<String>,
    pages: usize,
}
impl Read {
    pub fn begin(store: &mut Presence) -> Self {
        store.read_generation = store.read_generation.wrapping_add(1);
        Self {
            generation: store.read_generation,
            revision: store.revision,
            initial: store.records.clone(),
            total: 0,
            active_cursors: Default::default(),
            ids_cursors: Default::default(),
            ids: Default::default(),
            pages: 0,
        }
    }
    pub fn current(&self, store: &Presence) -> bool {
        self.generation == store.read_generation && self.revision == store.revision
    }
    pub fn needs_purge(&self) -> bool {
        !self.initial.is_empty()
    }
    pub fn active(&mut self, store: &mut Presence, raw: &Value) -> Result<Step, Error> {
        if !self.current(store) {
            return Err(invalid());
        }
        envelope(raw)?;
        let empty = Vec::new();
        let rows = match raw.get("drafts") {
            None | Some(Value::Null) => &empty,
            Some(v) => v
                .as_array()
                .filter(|r| r.len() <= 1000)
                .ok_or_else(invalid)?,
        };
        let more = flag(raw, "has_more")?;
        // Stage the whole page; malformed rows cannot partially change metadata.
        let mut next = store.clone();
        let mut seen = BTreeSet::new();
        for row in rows {
            if !seen.insert(text(row, "id")?) {
                return Err(invalid());
            }
            next.reconcile(row, false)?;
        }
        let total = self.total.checked_add(rows.len()).ok_or_else(invalid)?;
        let step = if rows.is_empty() || !more || total >= 200 {
            Step::Done
        } else {
            let value = &rows.last().unwrap()["last_updated_ts"];
            let cursor = match value {
                Value::String(v) => v.clone(),
                Value::Number(n) => n.to_string(),
                _ => return Err(invalid()),
            };
            if cursor.is_empty() || !self.active_cursors.insert(cursor.clone()) {
                return Err(invalid());
            }
            Step::More(cursor)
        };
        let before = store.records.clone();
        store.records = next.records;
        store.changed(before);
        self.revision = store.revision;
        self.total = total;
        Ok(step)
    }
    pub fn active_ids(&mut self, store: &mut Presence, raw: &Value) -> Result<Step, Error> {
        if !self.current(store) {
            return Err(invalid());
        }
        envelope(raw)?;
        let empty = Vec::new();
        let rows = match raw.get("active_draft_ids") {
            None | Some(Value::Null) => &empty,
            Some(v) => v
                .as_array()
                .filter(|r| r.len() <= 10_000)
                .ok_or_else(invalid)?,
        };
        let more = flag(raw, "has_more")?;
        let mut ids = self.ids.clone();
        for id in rows {
            let id = id
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 255 && !s.chars().any(char::is_control))
                .ok_or_else(invalid)?;
            ids.insert(id.into());
        }
        // h/H5 returns the empty terminal page itself, rather than the prior
        // accumulator. This affects only the observed server metadata mirror.
        let step = if rows.is_empty() {
            ids.clear();
            Step::Done
        } else if !more {
            Step::Done
        } else {
            let cursor = text(raw, "next_ts")?;
            if self.pages >= 100 || !self.ids_cursors.insert(cursor.clone()) {
                return Err(invalid());
            }
            Step::More(cursor)
        };
        self.pages += 1;
        self.ids = ids;
        if step == Step::Done {
            let before = store.records.clone();
            let removed: BTreeSet<_> = self
                .initial
                .iter()
                .filter(|old| !self.ids.contains(&old.id))
                .map(|old| old.key.as_str())
                .collect();
            store.records.retain(|r| !removed.contains(r.key.as_str()));
            store.changed(before);
            self.revision = store.revision;
        }
        Ok(step)
    }
}
