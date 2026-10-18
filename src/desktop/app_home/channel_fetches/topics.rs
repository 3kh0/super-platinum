//! Socket topic invalidation from t0MN/sdHB/GAK9. One leading/trailing throttle
//! serves all teams; each team retains its own ordered pending topic list.
//! Keys must include publication custody. Callers reject retired keys before IO.
use std::{
    collections::{BTreeMap, BTreeSet},
    hash::BuildHasher,
    time::{Duration, Instant},
};
use super_platinum_core::slack::events::RawEvent;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Event {
    Desync(Vec<String>),
    Resync(Vec<String>),
}
pub(super) fn event(raw: &RawEvent) -> Option<Event> {
    if !matches!(raw.kind.as_str(), "desync" | "resync") {
        return None;
    }
    let ids = match raw.rest.get("channel_ids") {
        None => Vec::new(),
        Some(value) => value
            .as_array()?
            .iter()
            .map(|v| v.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()?,
    };
    Some(if raw.kind == "desync" {
        Event::Desync(ids)
    } else {
        Event::Resync(ids)
    })
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Invalidation {
    None,
    All,
    /// Team/enterprise topics invalidate cached objects and replay team events.
    Team,
    Channels(Vec<String>),
}
pub(super) fn classify(topics: &[String], self_id: &str) -> Invalidation {
    let mut channels = Vec::new();
    let mut team = false;
    for id in topics {
        if id == self_id {
            return Invalidation::All;
        }
        if looks_like(id, b"CGDZ") {
            if !channels.contains(id) {
                channels.push(id.clone());
            }
        } else if looks_like(id, b"TE") {
            team = true;
        } else {
            return Invalidation::All;
        }
    }
    if team {
        Invalidation::Team
    } else if channels.is_empty() {
        Invalidation::None
    } else {
        Invalidation::Channels(channels)
    }
}
fn looks_like(id: &str, prefixes: &[u8]) -> bool {
    let bytes = id.as_bytes();
    bytes.len() >= 9
        && prefixes.contains(&bytes[0])
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

pub(super) struct Batch<K> {
    pub(super) key: K,
    pub(super) topics: Vec<String>,
    pub(super) all_synced: bool,
}
pub(super) struct Queue<K> {
    pending: BTreeMap<K, Vec<String>>,
    desynced: BTreeMap<K, BTreeSet<String>>,
    last_key: Option<K>,
    last_call: Option<Instant>,
    last_invoke: Option<Instant>,
    deadline: Option<Instant>,
    wait: Duration,
}
impl<K: Ord + Clone> Default for Queue<K> {
    fn default() -> Self {
        // Slack samples this once per module: ceil(300000 * (1 + random[0,.2])).
        // RandomState supplies a fresh randomized seed without a new dependency.
        let sample = std::collections::hash_map::RandomState::new().hash_one(0u8);
        Self::new(Duration::from_millis(300_000 + sample % 60_001))
    }
}
impl<K: Ord + Clone> Queue<K> {
    fn new(wait: Duration) -> Self {
        Self {
            pending: BTreeMap::new(),
            desynced: BTreeMap::new(),
            last_key: None,
            last_call: None,
            last_invoke: None,
            deadline: None,
            wait,
        }
    }
    pub(super) fn desync(&mut self, key: K, topics: Vec<String>) {
        self.desynced.entry(key).or_default().extend(topics);
    }
    pub(super) fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
    /// Drop retired native account/socket custody without publishing its topics.
    /// This does not grant the remaining keys authority or reset the throttle.
    pub(super) fn retain_keys(&mut self, mut current: impl FnMut(&K) -> bool) {
        self.pending.retain(|key, _| current(key));
        self.desynced.retain(|key, _| current(key));
        if self.last_key.as_ref().is_some_and(|key| !current(key)) {
            self.last_key = None;
        }
    }
    /// Feed only accepted resync frames. A first invocation is immediate.
    pub(super) fn push(&mut self, key: K, topics: Vec<String>, now: Instant) -> Option<Batch<K>> {
        let invoke = self.should_invoke(now);
        self.pending.entry(key.clone()).or_default().extend(topics);
        self.last_key = Some(key);
        self.last_call = Some(now);
        if invoke {
            self.deadline = Some(now + self.wait);
            return self.invoke(now);
        }
        if self.deadline.is_none() {
            self.deadline = Some(now + self.wait);
        }
        None
    }
    /// Timer callback; deadlines are monotonic. A delayed callback cannot cause
    /// duplicate publication. An early callback simply retains its deadline.
    pub(super) fn due(&mut self, now: Instant) -> Option<Batch<K>> {
        if self.deadline.is_none_or(|at| now < at) {
            return None;
        }
        if self.should_invoke(now) {
            self.deadline = None;
            return self.invoke(now);
        }
        let since_call = now.saturating_duration_since(self.last_call.unwrap());
        let since_invoke = now.saturating_duration_since(self.last_invoke.unwrap());
        self.deadline = Some(
            now + self
                .wait
                .saturating_sub(since_call)
                .min(self.wait.saturating_sub(since_invoke)),
        );
        None
    }
    fn should_invoke(&self, now: Instant) -> bool {
        self.last_call
            .is_none_or(|at| now.saturating_duration_since(at) >= self.wait)
            || self
                .last_invoke
                .is_none_or(|at| now.saturating_duration_since(at) >= self.wait)
    }
    fn invoke(&mut self, now: Instant) -> Option<Batch<K>> {
        let key = self.last_key.take()?;
        self.last_invoke = Some(now);
        let topics = self.pending.remove(&key).unwrap_or_default();
        let desynced = self.desynced.entry(key.clone()).or_default();
        for id in &topics {
            desynced.remove(id);
        }
        Some(Batch {
            key,
            topics,
            all_synced: desynced.is_empty(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ids(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| (*s).into()).collect()
    }
    #[test]
    fn source_classification_priority_and_exact_id_shapes() {
        assert_eq!(classify(&[], "U00000001"), Invalidation::None);
        assert_eq!(
            classify(&ids(&["C00000001", "G00000002", "C00000001"]), "U00000001"),
            Invalidation::Channels(ids(&["C00000001", "G00000002"]))
        );
        assert_eq!(
            classify(&ids(&["C00000001", "T00000001"]), "U00000001"),
            Invalidation::Team
        );
        assert_eq!(
            classify(&ids(&["E00000001"]), "U00000001"),
            Invalidation::Team
        );
        for id in [
            "U00000001",
            "U00000002",
            "C_SHORT",
            "C0000000a",
            "C00000001\n",
            "",
        ] {
            assert_eq!(
                classify(&ids(&["T00000001", id]), "U00000001"),
                Invalidation::All
            );
        }
    }
    #[test]
    fn only_actual_resync_desync_frames_are_ingress() {
        let raw = |v| serde_json::from_value::<RawEvent>(v).unwrap();
        assert_eq!(
            event(&raw(serde_json::json!({"type":"resync"}))),
            Some(Event::Resync(vec![]))
        );
        assert_eq!(
            event(&raw(
                serde_json::json!({"type":"desync","channel_ids":["C00000001"]})
            )),
            Some(Event::Desync(ids(&["C00000001"])))
        );
        for value in [
            serde_json::json!({"type":"channel_rename","channel_ids":["C00000001"]}),
            serde_json::json!({"type":"resync","channel_ids":null}),
            serde_json::json!({"type":"resync","channel_ids":[3]}),
        ] {
            assert!(event(&raw(value)).is_none());
        }
    }
    #[test]
    fn leading_then_ordered_trailing_and_no_duplicate_callback() {
        let mut q = Queue::new(Duration::from_secs(300));
        let now = Instant::now();
        q.desync("a", ids(&["C00000001", "C00000002"]));
        let first = q.push("a", ids(&["C00000001"]), now).unwrap();
        assert!(!first.all_synced);
        assert!(
            q.push("a", ids(&["C00000002"]), now + Duration::from_secs(5))
                .is_none()
        );
        assert!(
            q.push(
                "a",
                ids(&["C00000002", "C00000003"]),
                now + Duration::from_secs(10)
            )
            .is_none()
        );
        assert!(q.due(now + Duration::from_secs(299)).is_none());
        let tail = q.due(now + Duration::from_secs(300)).unwrap();
        assert_eq!(tail.topics, ids(&["C00000002", "C00000002", "C00000003"]));
        assert!(tail.all_synced);
        assert!(q.due(now + Duration::from_secs(301)).is_none());
        assert!(q.deadline().is_none());
    }
    #[test]
    fn single_leading_call_has_no_trailing_work() {
        let mut q = Queue::new(Duration::from_secs(300));
        let now = Instant::now();
        assert!(q.push("a", vec![], now).is_some());
        assert!(q.due(now + Duration::from_secs(300)).is_none());
        assert!(q.deadline().is_none());
        assert!(
            q.push("a", vec![], now + Duration::from_secs(400))
                .is_some()
        );
    }
    #[test]
    fn shared_throttle_retains_each_teams_unconsumed_topics() {
        let mut q = Queue::new(Duration::from_secs(300));
        let now = Instant::now();
        q.push("a", vec![], now).unwrap();
        q.push("a", ids(&["C00000001"]), now + Duration::from_secs(10));
        q.push("b", ids(&["C00000002"]), now + Duration::from_secs(20));
        let tail = q.due(now + Duration::from_secs(300)).unwrap();
        assert_eq!(tail.key, "b");
        assert_eq!(tail.topics, ids(&["C00000002"]));
        let next = q
            .push("a", ids(&["C00000003"]), now + Duration::from_secs(650))
            .unwrap();
        assert_eq!(next.topics, ids(&["C00000001", "C00000003"]));
    }
    #[test]
    fn call_after_trailing_waits_for_max_wait_from_last_invoke() {
        let mut q = Queue::new(Duration::from_secs(300));
        let now = Instant::now();
        q.push("a", vec![], now);
        q.push("a", vec![], now + Duration::from_secs(290));
        q.due(now + Duration::from_secs(300)).unwrap();
        assert!(
            q.push("a", vec![], now + Duration::from_secs(310))
                .is_none()
        );
        assert!(q.due(now + Duration::from_secs(609)).is_none());
        assert!(q.due(now + Duration::from_secs(610)).is_some());
    }
    #[test]
    fn retired_owner_cannot_survive_in_trailing_callback() {
        let mut q = Queue::new(Duration::from_secs(300));
        let now = Instant::now();
        q.push(("account", 1), vec![], now);
        q.push(
            ("account", 1),
            ids(&["U00000001"]),
            now + Duration::from_secs(10),
        );
        q.retain_keys(|(_, epoch)| *epoch == 2);
        assert!(q.due(now + Duration::from_secs(300)).is_none());
        let fresh = q
            .push(
                ("account", 2),
                ids(&["C00000001"]),
                now + Duration::from_secs(400),
            )
            .unwrap();
        assert_eq!(fresh.topics, ids(&["C00000001"]));
    }
}
