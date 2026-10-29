//! One Harness-scoped pending lane. Receiver destruction precedes lane handoff.
use super::*;
use directory::read_scope::ReadScope;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use super_platinum_core::slack::PreparedRequest;
use tokio::sync::{Mutex, OwnedMutexGuard, oneshot};

type Receiver = oneshot::Receiver<Result<Value, Error>>;
pub(in crate::app_home::inputs::channels) enum Read {
    Live,
    Refused,
    Unavailable(Error),
    Registered(Exchange),
}
pub(in crate::app_home::inputs::channels) struct Exchange {
    state: Signal<ShellState>,
    lane: Arc<Mutex<()>>,
    nonce: u64,
    receiver: Option<Receiver>,
    guard: Option<OwnedMutexGuard<()>>,
}
impl Exchange {
    pub(in crate::app_home::inputs::channels) async fn receive(&mut self) -> Result<Value, Error> {
        self.receiver
            .as_mut()
            .expect("one exchange receiver")
            .await
            .unwrap_or_else(|_| Err(Error::Offline("Home fixture read unavailable".into())))
    }
}
impl Drop for Exchange {
    fn drop(&mut self) {
        // Do not let a replacement acquire the lane before this sender is closed.
        drop(self.receiver.take());
        if let Ok(mut s) = self.state.try_write() {
            if let Some(f) = s.app_home.channels.fixture.as_mut() {
                if Arc::ptr_eq(&f.lane, &self.lane) && f.pending_owner == Some(self.nonce) {
                    // Keep the closed sender as the old pending_closed evidence.
                    // A new registrar retires it; this nonce never clears a new slot.
                    f.pending_owner = None;
                }
            }
        }
        drop(self.guard.take());
    }
}

async fn begin<F>(state: Signal<ShellState>, t: &Ticket, scope: &ReadScope, register: F) -> Read
where
    F: FnOnce(&mut ShellState, &Ticket) -> Option<Receiver>,
{
    if !crate::fixture::is_fixture() {
        return Read::Live;
    }
    begin_closed(state, t, scope, register).await
}
async fn begin_closed<F>(
    mut state: Signal<ShellState>,
    t: &Ticket,
    scope: &ReadScope,
    register: F,
) -> Read
where
    F: FnOnce(&mut ShellState, &Ticket) -> Option<Receiver>,
{
    if !scope.current() {
        return Read::Refused;
    }
    let lane = {
        let Ok(s) = state.try_read() else {
            return Read::Refused;
        };
        let Some(f) = s.app_home.channels.fixture.as_ref() else {
            return Read::Unavailable(Error::Offline("Home fixture read unavailable".into()));
        };
        f.lane.clone()
    };
    let guard = lane.clone().lock_owned().await;
    if !scope.admitted() {
        return Read::Refused;
    }
    let Ok(mut s) = state.try_write() else {
        return Read::Refused;
    };
    if !t.current(&s)
        || s.app_home
            .channels
            .fixture
            .as_ref()
            .is_none_or(|f| !Arc::ptr_eq(&f.lane, &lane))
    {
        return Read::Refused;
    }
    if pending_open(&s) {
        return Read::Unavailable(Error::Offline("Home fixture read already pending".into()));
    }
    // Only a closed, no-longer-owned sender can be retired before registration.
    let f = s.app_home.channels.fixture.as_mut().unwrap();
    f.pending.take();
    f.pending_owner = None;
    let Some(receiver) = register(&mut s, t) else {
        return Read::Unavailable(Error::Offline("Home fixture read unavailable".into()));
    };
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let nonce = NEXT.fetch_add(1, Ordering::Relaxed);
    if let Some(f) = s.app_home.channels.fixture.as_mut() {
        if f.pending.is_some() {
            f.pending_owner = Some(nonce);
        }
    }
    drop(s);
    Read::Registered(Exchange {
        state,
        lane,
        nonce,
        receiver: Some(receiver),
        guard: Some(guard),
    })
}

#[cfg(test)]
pub(in crate::app_home::inputs::channels::fixture) async fn injected<F>(
    state: Signal<ShellState>,
    t: &Ticket,
    scope: &ReadScope,
    register: F,
) -> Read
where
    F: FnOnce(&mut ShellState, &Ticket) -> Option<Receiver>,
{
    begin_closed(state, t, scope, register).await
}

pub(in crate::app_home::inputs::channels) async fn channel(
    state: Signal<ShellState>,
    t: &Ticket,
    scope: &ReadScope,
    request: &PreparedRequest,
) -> Read {
    begin(state, t, scope, |s, t| super::request(s, t, request)).await
}
pub(in crate::app_home::inputs::channels) async fn conversations(
    state: Signal<ShellState>,
    t: &Ticket,
    scope: &ReadScope,
    requests: &directory::aggregate::Requests,
) -> Read {
    begin(state, t, scope, |s, t| {
        super::request_conversations(s, t, requests)
    })
    .await
}

#[cfg(test)]
mod tests;
