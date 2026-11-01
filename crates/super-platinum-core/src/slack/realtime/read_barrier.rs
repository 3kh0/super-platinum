//! Native read invalidation at consumed socket ingress, before lossy parsing.
//! These receipts grant no account, page, history or action authority.
use super::*;
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, Ordering},
};

const MAX_WATCHES: usize = 16;
#[derive(Default)]
struct State {
    revision: u64,
    closed: bool,
    watches: Vec<Weak<Flag>>,
}
struct Flag {
    channel: String,
    retired: AtomicBool,
}
#[derive(Default)]
pub(super) struct Ingress {
    state: Mutex<State>,
}
impl std::fmt::Debug for Ingress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReadIngress(..)")
    }
}
#[derive(Clone)]
pub struct ReadIdentity(Arc<Ingress>);
impl std::fmt::Debug for ReadIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReadIdentity(..)")
    }
}
impl ReadIdentity {
    pub fn is_for(&self, connection: &Connection) -> bool {
        Arc::ptr_eq(&self.0, &connection.read_ingress)
    }
    pub fn open(&self) -> bool {
        self.0.state.lock().is_ok_and(|s| !s.closed)
    }
}
#[derive(Clone)]
pub struct ReadBarrier {
    ingress: Arc<Ingress>,
    revision: u64,
}
impl std::fmt::Debug for ReadBarrier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReadBarrier(..)")
    }
}
/// Non-clone original occurrence. A consumer must also own the actual native
/// account/socket/page and check its current barrier before starting IO.
pub struct ReadWatch {
    ingress: Arc<Ingress>,
    revision: u64,
    flag: Arc<Flag>,
}
impl ReadBarrier {
    pub fn belongs_to(&self, connection: &Connection) -> bool {
        Arc::ptr_eq(&self.ingress, &connection.read_ingress)
    }
    pub fn current_for(&self, connection: &Connection) -> bool {
        self.belongs_to(connection)
            && self
                .ingress
                .state
                .lock()
                .is_ok_and(|s| !s.closed && s.revision == self.revision)
    }
}
impl ReadWatch {
    pub fn armed_after(&self, applied: &ReadBarrier) -> bool {
        Arc::ptr_eq(&self.ingress, &applied.ingress)
            && self.revision == applied.revision
            && self.current()
    }
    pub fn current(&self) -> bool {
        self.ingress
            .state
            .lock()
            .is_ok_and(|s| !s.closed && !self.flag.retired.load(Ordering::Acquire))
    }
    /// Hold this bounded, synchronous publication point against socket ingress.
    /// The closure must not await or reenter this connection's reader methods.
    pub fn with_current<T>(&self, publish: impl FnOnce() -> T) -> Option<T> {
        let state = self.ingress.state.lock().ok()?;
        if state.closed || self.flag.retired.load(Ordering::Acquire) {
            return None;
        }
        Some(publish())
    }
}
impl Ingress {
    pub(super) fn identity(self: &Arc<Self>) -> ReadIdentity {
        ReadIdentity(self.clone())
    }
    pub(super) fn initial(self: &Arc<Self>) -> ReadBarrier {
        ReadBarrier {
            ingress: self.clone(),
            revision: 0,
        }
    }
    pub(super) fn watch(self: &Arc<Self>, channel: &str) -> Option<ReadWatch> {
        if channel.is_empty() || channel.len() > 128 {
            return None;
        }
        let mut state = self.state.lock().ok()?;
        if state.closed {
            return None;
        }
        state.watches.retain(|w| w.strong_count() > 0);
        if state.watches.len() >= MAX_WATCHES {
            return None;
        }
        let flag = Arc::new(Flag {
            channel: channel.into(),
            retired: AtomicBool::new(false),
        });
        state.watches.push(Arc::downgrade(&flag));
        Some(ReadWatch {
            ingress: self.clone(),
            revision: state.revision,
            flag,
        })
    }
    /// None = irrelevant event; Some(None) = unclassifiable, retire every watch.
    fn channel(value: Option<&Value>) -> Option<Option<&str>> {
        let Some(v) = value else { return Some(None) };
        let Some(kind) = v.get("type").and_then(Value::as_str) else {
            return Some(None);
        };
        if matches!(kind, "user_typing" | "presence_change" | "pong" | "hello") {
            return None;
        }
        // Unknown event schemas may be batched: an incidental channel field
        // is not proof that all mutations belong to that one channel.
        if !matches!(
            kind,
            "message"
                | "message_changed"
                | "message_deleted"
                | "message_replied"
                | "reaction_added"
                | "reaction_removed"
                | "thread_subscribed"
                | "thread_unsubscribed"
                | "thread_marked"
                | "channel_left"
                | "channel_deleted"
                | "channel_archive"
                | "channel_unarchive"
        ) {
            return Some(None);
        }
        let mut channel = None;
        for candidate in [
            v.get("channel"),
            v.get("channel_id"),
            v.get("item").and_then(|i| i.get("channel")),
            v.get("message").and_then(|i| i.get("channel")),
            v.get("previous_message").and_then(|i| i.get("channel")),
            v.get("subscription").and_then(|i| i.get("channel")),
            v.get("subscription").and_then(|i| i.get("channel_id")),
        ]
        .into_iter()
        .flatten()
        {
            let Some(c) = candidate.as_str().filter(|c| !c.is_empty()) else {
                return Some(None);
            };
            if channel.is_some_and(|previous| previous != c) {
                return Some(None);
            }
            channel = Some(c);
        }
        Some(channel)
    }
    pub(super) fn observe(self: &Arc<Self>, value: Option<&Value>) -> Option<ReadBarrier> {
        let channel = Self::channel(value)?;
        let mut state = self.state.lock().ok()?;
        let next = state.revision.checked_add(1);
        if next.is_none() {
            state.closed = true;
        }
        let closed = state.closed;
        state.watches.retain(|weak| {
            let Some(watch) = weak.upgrade() else {
                return false;
            };
            if closed || channel.is_none_or(|c| c == watch.channel) {
                watch.retired.store(true, Ordering::Release);
            }
            true
        });
        state.revision = next?;
        Some(ReadBarrier {
            ingress: self.clone(),
            revision: state.revision,
        })
    }
    pub(super) fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.closed = true;
            for flag in state.watches.iter().filter_map(Weak::upgrade) {
                flag.retired.store(true, Ordering::Release);
            }
        }
    }
}
pub(super) struct Close(pub(super) Arc<Ingress>);
impl Drop for Close {
    fn drop(&mut self) {
        self.0.close();
    }
}

pub(super) fn connected(generation: u64, connection: Connection) -> Vec<RtUpdate> {
    let barrier = connection.read_ingress.initial();
    vec![
        RtUpdate::Connected {
            generation,
            connection,
        },
        RtUpdate::ReadBarrier {
            generation,
            barrier,
        },
    ]
}
/// The same synchronous producer is used by the socket loop and offline tests.
pub(super) fn updates(
    ingress: &Arc<Ingress>,
    subscriptions: &member_subscription::PendingReplies,
    generation: u64,
    original: &str,
) -> Vec<RtUpdate> {
    let value = serde_json::from_str::<Value>(original).ok();
    let barrier = ingress.observe(value.as_ref());
    let event = value.and_then(|v| {
        if subscriptions.receive(&v) {
            None
        } else {
            parse_event_value(v)
        }
    });
    let mut out = Vec::with_capacity(2);
    if let Some(event) = event {
        out.push(preference::update(generation, event, original));
    }
    if let Some(barrier) = barrier {
        out.push(RtUpdate::ReadBarrier {
            generation,
            barrier,
        });
    }
    out
}

// Existing explicit protected fixture feature only; no normal production
// constructor exposes an ingress barrier. It runs the actual producer above.
#[cfg(any(test, feature = "protected-test-support"))]
pub fn protected_connection(generation: u64) -> (Connection, Vec<RtUpdate>) {
    let (tx, _rx) = mpsc::channel(1);
    let connection = Connection::from_sender(tx);
    let updates = connected(generation, connection.clone());
    (connection, updates)
}
#[cfg(any(test, feature = "protected-test-support"))]
pub fn protected_updates(
    connection: &Connection,
    generation: u64,
    original: &str,
) -> Vec<RtUpdate> {
    updates(
        &connection.read_ingress,
        &connection.subscriptions,
        generation,
        original,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn barrier(updates: &[RtUpdate]) -> ReadBarrier {
        let RtUpdate::ReadBarrier { barrier, .. } = updates.last().unwrap() else {
            panic!("actual producer barrier")
        };
        barrier.clone()
    }
    #[test]
    fn thread_earlier_network_ingress_blocks_lossy_parse_and_queue_races_with_channel_and_owner_isolation()
     {
        let (connection, start) = protected_connection(4);
        let applied = barrier(&start);
        let own = connection.watch_message_read("C1").unwrap();
        let peer = connection.watch_message_read("C2").unwrap();
        assert!(own.armed_after(&applied) && peer.armed_after(&applied));
        let queued = protected_updates(
            &connection,
            4,
            r#"{"type":"message","subtype":"message_changed","channel":"C1"}"#,
        );
        assert_eq!(
            queued.len(),
            1,
            "original parser drops the malformed nested message, ingress still retires the read"
        );
        assert!(!own.current() && peer.current());
        let mut published = false;
        assert!(own.with_current(|| published = true).is_none());
        assert!(!published);
        peer.with_current(|| published = true).unwrap();
        assert!(published);
        let after_ingress = connection.watch_message_read("C1").unwrap();
        assert!(
            !after_ingress.armed_after(&applied),
            "an unapplied queued frame prevents starting the read"
        );
        let applied = barrier(&queued);
        assert!(after_ingress.armed_after(&applied));
        let live = protected_updates(
            &connection,
            4,
            r#"{"type":"reaction_added","user":"U1","reaction":"eyes","item":{"type":"message","channel":"C1","ts":"1719800001.000001"}}"#,
        );
        assert!(matches!(live.first(), Some(RtUpdate::Event { .. })));
        assert!(matches!(live.last(), Some(RtUpdate::ReadBarrier { .. })));
        assert!(
            !after_ingress.current() && peer.current(),
            "retirement precedes consumer dequeue"
        );
        let (other, _) = protected_connection(4);
        assert!(!barrier(&live).belongs_to(&other));
        protected_updates(&connection, 4, "malformed JSON");
        assert!(
            !peer.current(),
            "unknown-channel consumed ingress retires all outstanding reads"
        );
        let first = connection.watch_message_read("C1").unwrap();
        let second = connection.watch_message_read("C2").unwrap();
        protected_updates(
            &connection,
            4,
            r#"{"type":"message","channel":"C1","message":{"channel":"C2"}}"#,
        );
        assert!(
            !first.current() && !second.current(),
            "conflicting known channels retire all watches"
        );
        let unrelated = connection.watch_message_read("C3").unwrap();
        protected_updates(&connection, 4, r#"{"type":"future_batch","channel":"C9"}"#);
        assert!(
            !unrelated.current(),
            "unknown schema cannot use an incidental channel as a complete scope"
        );
        let held = connection.watch_message_read("C3").unwrap();
        connection.read_ingress.close();
        assert!(!held.current() && connection.watch_message_read("C3").is_none());
    }
}
