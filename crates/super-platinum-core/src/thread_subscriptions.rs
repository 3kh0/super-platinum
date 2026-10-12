//! Reply notification preferences. Subscription state is independent of read cursors.
use crate::{CoreAppState, domain::ThreadKey, slack::events::RawEvent};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
struct Entry {
    active: Option<bool>,
    revision: u64,
    pending: Option<u64>,
}
#[derive(Debug, Clone)]
pub struct Change {
    pub sequence: u64,
    revision: u64,
    pub active: bool,
}
#[derive(Debug, Clone, Default)]
pub struct ThreadSubscriptions {
    entries: HashMap<(u64, ThreadKey), Entry>,
    sequence: u64,
}
impl ThreadSubscriptions {
    pub fn active(&self, epoch: u64, key: &ThreadKey) -> Option<bool> {
        self.entries
            .get(&(epoch, key.clone()))
            .and_then(|v| v.active)
    }
    pub fn revision(&self, epoch: u64, key: &ThreadKey) -> u64 {
        self.entries
            .get(&(epoch, key.clone()))
            .map_or(0, |v| v.revision)
    }
    pub fn pending(&self, epoch: u64, key: &ThreadKey) -> bool {
        self.entries
            .get(&(epoch, key.clone()))
            .is_some_and(|v| v.pending.is_some())
    }
    pub fn apply(&mut self, epoch: u64, key: &ThreadKey, active: bool) {
        let entry = self.entries.entry((epoch, key.clone())).or_default();
        entry.active = Some(active);
        entry.revision = entry.revision.wrapping_add(1);
    }
    pub fn snapshot(&mut self, epoch: u64, key: &ThreadKey, active: bool, revision: u64) -> bool {
        if self.revision(epoch, key) != revision || self.pending(epoch, key) {
            return false;
        }
        self.apply(epoch, key, active);
        true
    }
    pub fn begin(&mut self, epoch: u64, key: &ThreadKey, active: bool) -> Option<Change> {
        if self.pending(epoch, key) {
            return None;
        }
        self.sequence = self.sequence.wrapping_add(1);
        let entry = self.entries.entry((epoch, key.clone())).or_default();
        entry.pending = Some(self.sequence);
        Some(Change {
            sequence: self.sequence,
            revision: entry.revision,
            active,
        })
    }
    pub fn finish(&mut self, epoch: u64, key: &ThreadKey, change: &Change, success: bool) -> bool {
        let Some(entry) = self.entries.get_mut(&(epoch, key.clone())) else {
            return false;
        };
        if entry.pending != Some(change.sequence) {
            return false;
        }
        entry.pending = None;
        if success && entry.revision == change.revision {
            self.apply(epoch, key, change.active);
        }
        true
    }
}

pub fn from_event(event: &RawEvent) -> Option<(String, String, bool)> {
    let active = match event.kind.as_str() {
        "thread_subscribed" => true,
        "thread_unsubscribed" => false,
        _ => return None,
    };
    let sub = event.rest.get("subscription")?;
    Some((
        sub.get("channel")?.as_str()?.into(),
        sub.get("thread_ts")?.as_str()?.into(),
        active,
    ))
}
impl CoreAppState {
    pub fn reconcile_thread_subscription(&mut self, key: &ThreadKey) {
        let Some(active) = self.thread_subscriptions.active(self.account_epoch, key) else {
            return;
        };
        if let Some(message) = self
            .workspaces
            .get_mut(&key.0)
            .and_then(|w| w.messages.get_mut(&key.1))
            .and_then(|bag| {
                bag.messages
                    .iter_mut()
                    .find(|m| m.ts.as_ref() == Some(&key.2))
            })
        {
            message.extra.insert("subscribed".into(), active.into());
        }
        if let Some(message) = self.threads.get_mut(key).and_then(|bag| {
            bag.messages
                .iter_mut()
                .find(|m| m.ts.as_ref() == Some(&key.2))
        }) {
            message.extra.insert("subscribed".into(), active.into());
        }
        if self.threads_view.matches_scope(self.account_epoch, &key.0) {
            if active {
                if let Some(item) = self
                    .threads_view
                    .items
                    .iter_mut()
                    .find(|i| i.channel() == Some(&key.1) && i.root_ts() == Some(&key.2))
                {
                    item.root_msg.extra.insert("subscribed".into(), true.into());
                } else {
                    self.threads_view.dirty = true;
                }
            } else {
                let raw:RawEvent=serde_json::from_value(serde_json::json!({"type":"thread_unsubscribed","subscription":{"channel":key.1,"thread_ts":key.2}})).expect("valid subscription event");
                self.threads_view
                    .observe(&crate::slack::events::RtEvent::Unknown(raw));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key() -> ThreadKey {
        ("T".into(), "C".into(), "1.0".into())
    }
    #[test]
    fn late_http_ack_cannot_reverse_a_newer_cross_client_change_even_after_aba() {
        let mut s = ThreadSubscriptions::default();
        let k = key();
        s.apply(1, &k, true);
        let request = s.begin(1, &k, false).unwrap();
        s.apply(1, &k, false);
        s.apply(1, &k, true);
        assert!(s.finish(1, &k, &request, true));
        assert_eq!(s.active(1, &k), Some(true));
        assert!(!s.pending(1, &k));
    }
    #[test]
    fn read_snapshot_cannot_override_a_pending_change_or_live_event() {
        let mut s = ThreadSubscriptions::default();
        let k = key();
        let request = s.begin(1, &k, true).unwrap();
        assert!(!s.snapshot(1, &k, false, 0));
        s.finish(1, &k, &request, true);
        assert!(!s.snapshot(1, &k, false, 0));
        assert_eq!(s.active(1, &k), Some(true));
    }
    #[test]
    fn failed_changes_and_old_account_acks_keep_other_threads_untouched() {
        let mut s = ThreadSubscriptions::default();
        let k = key();
        s.apply(1, &k, true);
        let request = s.begin(1, &k, false).unwrap();
        assert!(s.begin(1, &k, false).is_none());
        assert!(!s.finish(2, &k, &request, true));
        s.finish(1, &k, &request, false);
        assert_eq!(s.active(1, &k), Some(true));
        assert_eq!(s.active(2, &k), None);
        assert!(!s.pending(1, &k));
    }
}
