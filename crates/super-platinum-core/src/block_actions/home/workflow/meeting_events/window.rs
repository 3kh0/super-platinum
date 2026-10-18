use super::{Event, Fault, positive, request::valid_day};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

// Local bounded storage policy, not a server history/window limit. Capacity
// refusal preserves the previous snapshot and caller's refresh/error state.
const MAX_EVENTS: usize = 4096;
const MAX_DAYS: usize = 366;

#[derive(Clone, Debug, PartialEq)]
pub struct WindowPage {
    days: Vec<(String, Vec<Event>)>,
}
pub fn decode_window(raw: &Value, requested: &[String]) -> Result<WindowPage, Fault> {
    positive(raw)?;
    let rows = raw["days"].as_array().ok_or(Fault::InvalidEnvelope)?;
    if requested.is_empty()
        || requested.len() > MAX_DAYS
        || !requested.iter().all(|d| valid_day(d))
        || rows.len() != requested.len()
    {
        return Err(Fault::InvalidEnvelope);
    }
    let expected: BTreeSet<_> = requested.iter().map(String::as_str).collect();
    if expected.len() != requested.len() {
        return Err(Fault::InvalidEnvelope);
    }
    let mut seen = BTreeSet::new();
    let mut days = Vec::new();
    let mut count = 0;
    for row in rows {
        if row["sync_status"] == "syncing" {
            return Err(Fault::Syncing);
        }
        let day = row["day"].as_str().ok_or(Fault::InvalidEnvelope)?;
        if !expected.contains(day) || !seen.insert(day) {
            return Err(Fault::InvalidEnvelope);
        }
        let events = row["events"].as_array().ok_or(Fault::InvalidEnvelope)?;
        count += events.len();
        if count > MAX_EVENTS {
            return Err(Fault::Capacity);
        }
        let mut parsed = Vec::new();
        let mut ids = BTreeSet::new();
        for raw in events {
            let event = Event::from_raw(raw.clone())?;
            if event.deleted() || event.raw()["calendar_event_type"] == "working_location" {
                continue;
            }
            if !ids.insert(event.id().to_owned()) {
                return Err(Fault::InvalidEnvelope);
            }
            parsed.push(event);
        }
        days.push((day.into(), parsed));
    }
    Ok(WindowPage { days })
}

/// The host owns this snapshot and its account/capture generation. Exact raw
/// event data is retained; no disk/network work, timers, or query state exists.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EventWindow {
    by_id: BTreeMap<String, Event>,
    // JavaScript Object.keys: integer-index keys sort numerically; other keys
    // retain insertion order. Existing-key replacement does not move its slot.
    insertion_order: Vec<String>,
    by_link: BTreeMap<String, Vec<String>>,
    by_day: BTreeMap<String, Vec<String>>,
}
impl EventWindow {
    /// 2vjo.Fw: first indexed ID, not earliest start/latest update.
    pub fn by_link(&self, link: &str) -> Option<&Event> {
        self.by_link
            .get(link)?
            .first()
            .and_then(|id| self.by_id.get(id))
    }
    pub fn by_id(&self, id: &str) -> Option<&Event> {
        self.by_id.get(id)
    }
    pub fn day_loaded(&self, day: &str) -> bool {
        self.by_day.contains_key(day)
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn upsert(&mut self, event: Event) -> Result<(), Fault> {
        let mut next = self.clone();
        next.upsert_inner(event)?;
        next.bounded()?;
        *self = next;
        Ok(())
    }
    /// Positive realtime event object; caller must already validate account,
    /// realtime generation and the source event type/subtype before calling.
    /// Deletion returns the removed full event as evidence, never an API error.
    pub fn realtime(&mut self, subtype: &str, raw: Value) -> Result<Option<Event>, Fault> {
        let event = Event::from_raw(raw)?;
        // t0MN clears/refetches when legacy/numeric/encoded ID generations
        // differ. Do not mix generations or silently keep a stale index.
        if self
            .first_event_id()
            .is_some_and(|id| id_format(id) != id_format(event.id()))
        {
            return Err(Fault::RefreshRequired);
        }
        match subtype {
            "created" | "updated" => {
                self.upsert(event)?;
                Ok(None)
            }
            "deleted" => Ok(self.remove(event.id())),
            _ => Err(Fault::Unresolved),
        }
    }
    pub fn replace_days(&mut self, page: WindowPage) -> Result<(), Fault> {
        let mut next = self.clone();
        for (day, events) in page.days {
            // Preserve an old event if another loaded day still references it.
            let old = next.by_day.get(&day).cloned().unwrap_or_default();
            for id in old {
                if !next
                    .by_day
                    .iter()
                    .any(|(key, ids)| key != &day && ids.contains(&id))
                {
                    next.remove_indexed(&id);
                }
            }
            let mut ids = Vec::new();
            for event in events {
                let id = event.id().to_owned();
                next.index_event(event);
                ids.push(id);
            }
            next.by_day.insert(day, ids);
        }
        next.bounded()?;
        *self = next;
        Ok(())
    }
    fn upsert_inner(&mut self, event: Event) -> Result<(), Fault> {
        let id = event.id().to_owned();
        if event.deleted() {
            self.remove(&id);
            return Ok(());
        }
        for ids in self.by_day.values_mut() {
            ids.retain(|prior| prior != &id);
        }
        let days = event_days(&event)?;
        self.index_event(event);
        for day in days {
            if let Some(ids) = self.by_day.get_mut(&day) {
                let start = start(&self.by_id[&id]);
                let slot = ids
                    .iter()
                    .position(|prior| start < self.by_id.get(prior).map(start_of).unwrap_or(0.0))
                    .unwrap_or(ids.len());
                ids.insert(slot, id.clone());
            }
        }
        Ok(())
    }
    fn index_event(&mut self, event: Event) {
        let id = event.id().to_owned();
        if !self.by_id.contains_key(&id) {
            self.insertion_order.push(id.clone());
        }
        if self.by_id.get(&id).and_then(Event::link) != event.link() {
            self.remove_link(&id);
        }
        if let Some(link) = event.link() {
            let ids = self.by_link.entry(link.into()).or_default();
            if !ids.contains(&id) {
                ids.push(id.clone());
            }
        }
        self.by_id.insert(id, event);
    }
    fn remove_link(&mut self, id: &str) {
        if let Some(link) = self.by_id.get(id).and_then(Event::link).map(str::to_owned) {
            if let Some(ids) = self.by_link.get_mut(&link) {
                ids.retain(|prior| prior != id);
                if ids.is_empty() {
                    self.by_link.remove(&link);
                }
            }
        }
    }
    fn remove_indexed(&mut self, id: &str) -> Option<Event> {
        self.remove_link(id);
        self.insertion_order.retain(|prior| prior != id);
        self.by_id.remove(id)
    }
    fn remove(&mut self, id: &str) -> Option<Event> {
        let prior = self.remove_indexed(id);
        for ids in self.by_day.values_mut() {
            ids.retain(|v| v != id);
        }
        prior
    }
    fn first_event_id(&self) -> Option<&str> {
        self.insertion_order
            .iter()
            .filter_map(|id| array_index(id).map(|index| (index, id.as_str())))
            .min_by_key(|(index, _)| *index)
            .map(|(_, id)| id)
            .or_else(|| self.insertion_order.first().map(String::as_str))
    }
    fn bounded(&self) -> Result<(), Fault> {
        if self.by_id.len() > MAX_EVENTS || self.by_day.len() > MAX_DAYS {
            Err(Fault::Capacity)
        } else {
            Ok(())
        }
    }
}
// OrdinaryOwnPropertyKeys array-index boundary: 2^32 - 1 is not an index.
// Canonical spelling excludes leading zeros, signs, whitespace and exponents.
fn array_index(id: &str) -> Option<u32> {
    let value = id.parse::<u32>().ok()?;
    (value != u32::MAX && value.to_string() == id).then_some(value)
}
fn id_format(id: &str) -> u8 {
    if id.contains(':') {
        0
    } else if id.bytes().all(|b| b.is_ascii_digit()) {
        1
    } else {
        2
    }
}
fn start_of(event: &Event) -> f64 {
    start(event)
}
fn start(event: &Event) -> f64 {
    event.raw()["start"].as_f64().unwrap_or(0.0)
}
fn event_days(event: &Event) -> Result<Vec<String>, Fault> {
    let Some(start) = event.raw().get("start") else {
        return Ok(vec![]);
    };
    let start = start
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or(Fault::InvalidEvent)?;
    if start == 0.0 {
        return Ok(vec![]);
    }
    let date = |ts: f64| {
        let millis = ts * 1000.0;
        if !millis.is_finite() || millis.abs() > 8_640_000_000_000_000.0 {
            return Err(Fault::InvalidEvent);
        }
        chrono::DateTime::from_timestamp_millis(millis.trunc() as i64)
            .map(|v| v.date_naive())
            .ok_or(Fault::InvalidEvent)
    };
    let first = date(start)?;
    let end = match event.raw().get("end") {
        None => start,
        Some(v) => v
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or(Fault::InvalidEvent)?,
    };
    let last = if end <= start { first } else { date(end)? };
    let count = last.signed_duration_since(first).num_days();
    if !(0..MAX_DAYS as i64).contains(&count) {
        return Err(Fault::Capacity);
    }
    (0..=count)
        .map(|n| {
            first
                .checked_add_signed(chrono::Duration::days(n))
                .map(|d| d.format("%Y-%m-%d").to_string())
                .ok_or(Fault::InvalidEvent)
        })
        .collect()
}

#[cfg(test)]
mod order_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn javascript_array_indices_precede_strings_and_use_numeric_order() {
        let mut window = EventWindow::default();
        for id in ["zEncoded", "4294967295", "01", "10", "2", "0", "a:legacy"] {
            window
                .upsert(Event::from_raw(json!({"id":id})).unwrap())
                .unwrap();
        }
        for expected in ["0", "2", "10", "zEncoded", "4294967295", "01", "a:legacy"] {
            assert_eq!(window.first_event_id(), Some(expected));
            window.remove(expected).unwrap();
        }
        assert_eq!(window.first_event_id(), None);
        for value in [
            "00",
            "01",
            "-0",
            "+1",
            "1.0",
            "1e0",
            "4294967295",
            "4294967296",
        ] {
            assert_eq!(array_index(value), None);
        }
        assert_eq!(array_index("4294967294"), Some(4294967294));
    }
}
