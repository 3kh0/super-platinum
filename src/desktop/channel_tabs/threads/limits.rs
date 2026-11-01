//! Original-account getView throttle facts, independent of visible tab custody.
//! A budget can only deny dispatch. It never supplies channel/read authority.
use super::*;
use std::{collections::BTreeMap, time::Duration};
use super_platinum_core::slack::SlackClient;
type Key = (String, String, String);
const MAX_SCOPES: usize = 32;
#[derive(Clone)]
pub(super) struct Owner {
    key: Key,
    read: Arc<crate::bootstrap::ReadOwner>,
    client: SlackClient,
}
impl Owner {
    pub fn from_gate(g: &gate::Gate) -> Self {
        Self {
            key: (g.account.clone(), g.team.clone(), g.session.user_id.clone()),
            read: g.owner.clone(),
            client: g.client.clone(),
        }
    }
    fn current(&self, s: &ShellState) -> bool {
        self.read.current(&s.core) && s.core.client.same_configuration(&self.client)
    }
}
struct Entry {
    owner: Owner,
    pending: Vec<Arc<()>>,
    until: Option<Instant>,
    blocked: bool,
    wake: Arc<()>,
}
#[derive(Default)]
pub(crate) struct Limits {
    entries: BTreeMap<Key, Entry>,
}
pub(super) struct Ticket {
    owner: Owner,
    token: Arc<()>,
}
#[derive(Clone, Copy, Default)]
pub(crate) struct Status {
    pub until: Option<Instant>,
    pub blocked: bool,
}
impl Status {
    pub fn cooling(self) -> bool {
        self.blocked || self.until.is_some_and(|at| at > Instant::now())
    }
}
pub(super) fn status(s: &ShellState, owner: &Owner) -> Status {
    s.channel_tabs
        .thread_limits
        .entries
        .get(&owner.key)
        .filter(|e| owner.current(s) && e.owner.current(s))
        .map_or(Status::default(), |e| Status {
            until: e.until.filter(|at| *at > Instant::now()),
            blocked: e.blocked,
        })
}
pub(crate) fn visible(s: &ShellState) -> Status {
    selected(s)
        .map(|r| status(s, &Owner::from_gate(&r.gate)))
        .unwrap_or_default()
}
fn paint(s: &mut ShellState, status: Status) {
    let v = &mut s.channel_tabs.threads;
    v.retry_at = status.until;
    v.rate_blocked = status.blocked;
    v.notice = if status.cooling() {
        "Threads are temporarily limited. Try again shortly."
    } else {
        "Threads could not start. Try again later."
    }
    .into();
}
pub(super) fn denied(s: &mut ShellState, owner: &Owner) {
    let state = status(s, owner);
    paint(s, state);
}
pub(super) fn reserve(s: &mut ShellState, owner: Owner) -> Option<Ticket> {
    if !owner.current(s) || status(s, &owner).cooling() {
        return None;
    }
    let retired = s
        .channel_tabs
        .thread_limits
        .entries
        .iter()
        .filter(|(_, e)| {
            !e.owner.current(s)
                || e.pending.is_empty()
                    && !e.blocked
                    && e.until.is_none_or(|at| at <= Instant::now())
        })
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();
    for key in retired {
        s.channel_tabs.thread_limits.entries.remove(&key);
    }
    let root = &mut s.channel_tabs.thread_limits;
    if !root.entries.contains_key(&owner.key) && root.entries.len() >= MAX_SCOPES {
        return None;
    }
    let e = root
        .entries
        .entry(owner.key.clone())
        .or_insert_with(|| Entry {
            owner: owner.clone(),
            pending: Vec::new(),
            until: None,
            blocked: false,
            wake: Arc::new(()),
        });
    if e.pending.len() >= 32 {
        return None;
    }
    let token = Arc::new(());
    e.pending.push(token.clone());
    Some(Ticket { owner, token })
}
fn owns(e: &Entry, t: &Ticket) -> bool {
    e.pending.iter().any(|p| Arc::ptr_eq(p, &t.token))
}
pub(super) fn allows(s: &ShellState, t: &Ticket) -> bool {
    t.owner.current(s)
        && s.channel_tabs
            .thread_limits
            .entries
            .get(&t.owner.key)
            .is_some_and(|e| {
                e.owner.current(s)
                    && owns(e, t)
                    && !e.blocked
                    && e.until.is_none_or(|at| at <= Instant::now())
            })
}
pub(super) fn release(s: &mut ShellState, t: &Ticket) {
    if let Some(e) = s.channel_tabs.thread_limits.entries.get_mut(&t.owner.key) {
        e.pending.retain(|p| !Arc::ptr_eq(p, &t.token));
    }
}
/// Called for actual attempts BEFORE the view-retirement fence. A response for
/// a retired tab may throttle its original live account, never a replacement.
pub(super) fn observe(mut state: Signal<ShellState>, t: &Ticket, result: &Result<Value, Error>) {
    let Err(Error::RateLimited { retry_after_secs }) = result else {
        return;
    };
    let wake = {
        let mut s = state.write();
        if !t.owner.current(&s) {
            return;
        }
        let Some(e) = s.channel_tabs.thread_limits.entries.get_mut(&t.owner.key) else {
            return;
        };
        if !owns(e, t) || e.blocked {
            return;
        }
        let Some(at) =
            Instant::now().checked_add(Duration::from_secs(retry_after_secs.unwrap_or(60)))
        else {
            e.blocked = true;
            e.until = None;
            e.wake = Arc::new(());
            return;
        };
        if e.until.is_some_and(|old| old >= at) {
            return;
        }
        e.until = Some(at);
        e.wake = Arc::new(());
        (at, e.wake.clone())
    };
    let owner = t.owner.clone();
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(wake.0.saturating_duration_since(Instant::now())).await;
        let mut s = state.write();
        if !owner.current(&s) {
            return;
        }
        let Some(e) = s.channel_tabs.thread_limits.entries.get_mut(&owner.key) else {
            return;
        };
        if e.until != Some(wake.0) || !Arc::ptr_eq(&e.wake, &wake.1) || Instant::now() < wake.0 {
            return;
        }
        e.until = None;
        // Presentation mirror only; authoritative data remains in this store.
        let same = s
            .channel_tabs
            .threads
            .route
            .as_ref()
            .is_some_and(|r| Owner::from_gate(&r.gate).key == owner.key);
        if same && s.channel_tabs.threads.retry_at == Some(wake.0) {
            s.channel_tabs.threads.retry_at = None;
        }
        // Signal publication enables UI only. No source start/mount/HTTP call.
    });
}
