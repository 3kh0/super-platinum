//! Indexed TTL lookup plus source insertion order; no canonical Channel state.
use std::collections::{BTreeMap, HashMap};
#[derive(Default)]
pub(super) struct Freshness {
    values: HashMap<String, (u64, Option<u64>)>,
    order: BTreeMap<u64, String>,
    next: u64,
}
impl Freshness {
    pub(super) fn clear(&mut self) {
        self.values.clear();
        self.order.clear();
        self.next = 0;
    }
    pub(super) fn get(&self, id: &str) -> Option<&Option<u64>> {
        self.values.get(id).map(|(_, ttl)| ttl)
    }
    pub(super) fn insert(&mut self, id: String, ttl: Option<u64>) {
        if let Some((_, old)) = self.values.get_mut(&id) {
            *old = ttl;
            return;
        }
        if self.next == u64::MAX {
            // Renumber only the order index on counter exhaustion. No row cap,
            // eviction, TTL change or change in insertion order.
            self.order = std::mem::take(&mut self.order)
                .into_values()
                .enumerate()
                .map(|(n, id)| {
                    self.values.get_mut(&id).unwrap().0 = n as u64;
                    (n as u64, id)
                })
                .collect();
            self.next = self.order.len() as u64;
        }
        let index = self.next;
        self.next += 1;
        self.order.insert(index, id.clone());
        self.values.insert(id, (index, ttl));
    }
    pub(super) fn remove(&mut self, id: &str) {
        if let Some((index, _)) = self.values.remove(id) {
            self.order.remove(&index);
        }
    }
    pub(super) fn fresh_ids(&self, seconds: u64) -> Vec<String> {
        self.order
            .values()
            .filter(|id| self.get(id).is_some_and(|t| t.is_none_or(|t| t > seconds)))
            .cloned()
            .collect()
    }
}
