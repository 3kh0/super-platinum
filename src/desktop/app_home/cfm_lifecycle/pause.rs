//! Process-local UI-thread timer ownership, shared by every CFM and account.
//! The pure source model never invents timer IDs; this registry allocates actual
//! native timers and suppresses callbacks only when their own handle is cancelled.
use std::{cell::RefCell, collections::BTreeMap, sync::Arc, time::Duration};
use super_platinum_core::state::directory_channels::freshness::{Pause, TimerId};

thread_local! {
    static OWNER: RefCell<Timers> = RefCell::new(Timers::new());
}
struct Timer {
    identity: Arc<()>,
    cancel: tokio::sync::oneshot::Sender<()>,
}
struct Timers {
    pause: Pause,
    next: u64,
    live: BTreeMap<u64, Timer>,
}
impl Timers {
    fn new() -> Self {
        let mut pause = Pause::default();
        // Ordinary client-v2 source starts paused until actual bootstrap's
        // togglePause transition. Renderer installation itself cannot unpause it.
        pause.indefinitely();
        Self {
            pause,
            next: 1,
            live: BTreeMap::new(),
        }
    }
    fn cancel(&mut self, id: Option<TimerId>) {
        if let Some(timer) = id.and_then(|id| self.live.remove(&id.value())) {
            let _ = timer.cancel.send(());
        }
    }
    fn keypress(&mut self) -> Option<(TimerId, Arc<()>, tokio::sync::oneshot::Receiver<()>)> {
        let next = self.next.checked_add(1)?;
        let prior = self.pause.cancellation_handle();
        self.cancel(prior); // Cancel BEFORE allocating/installing replacement.
        let id = TimerId::new(self.next).ok()?;
        self.next = next;
        let identity = Arc::new(());
        let (cancel, receiver) = tokio::sync::oneshot::channel();
        self.live.insert(
            id.value(),
            Timer {
                identity: identity.clone(),
                cancel,
            },
        );
        assert_eq!(self.pause.keypress(id), prior);
        Some((id, identity, receiver))
    }
    fn indefinitely(&mut self) {
        let old = self.pause.indefinitely();
        self.cancel(old);
    }
    fn fired(&mut self, id: TimerId, identity: &Arc<()>) {
        if self
            .live
            .get(&id.value())
            .is_none_or(|timer| !Arc::ptr_eq(&timer.identity, identity))
        {
            return;
        }
        self.live.remove(&id.value());
        // Source callback deliberately clears a newer pause as well. Unpause
        // does not cancel an older timer, so that orphan remains a real callback.
        self.pause.timer_fired();
    }
}

pub(crate) fn paused() -> bool {
    !super::listener::ready() || OWNER.with(|owner| owner.borrow().pause.paused())
}
/// Borrow the one actual source Pause synchronously; no reconstructed timer
/// state or second Pause is created for Apps. Missing listener is unavailable.
pub(crate) fn with_pause<R>(
    read: impl FnOnce(&Pause) -> R,
) -> Result<R, super_platinum_core::slack::Error> {
    if !super::listener::ready() {
        return Err(super_platinum_core::slack::Error::Api(
            "channel_lifecycle_unavailable".into(),
        ));
    }
    Ok(OWNER.with(|owner| read(&owner.borrow().pause)))
}
pub(crate) fn keypress() {
    let Some((id, identity, mut cancelled)) = OWNER.with(|owner| owner.borrow_mut().keypress())
    else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        tokio::select! {
            biased;
            _=&mut cancelled=>{},
            _=tokio::time::sleep(Duration::from_millis(Pause::DELAY_MS))=>{
                OWNER.with(|owner|owner.borrow_mut().fired(id,&identity));
            }
        }
    });
}
pub(crate) fn indefinitely() {
    OWNER.with(|owner| owner.borrow_mut().indefinitely());
}
pub(crate) fn unpause() {
    OWNER.with(|owner| owner.borrow_mut().pause.unpause());
}

/// Source synchronous dispatch scope; never hold this across an await. Pause is
/// intentionally not a nesting counter: source finally unpauses immediately.
pub(crate) struct Dispatch;
pub(crate) fn dispatch() -> Dispatch {
    indefinitely();
    Dispatch
}
impl Drop for Dispatch {
    fn drop(&mut self) {
        unpause();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn orphan_callback_clears_newer_pause_but_cancelled_callback_cannot() {
        let mut timers = Timers::new();
        assert!(timers.pause.paused());
        let (first, one, _) = timers.keypress().unwrap();
        timers.pause.unpause();
        let (second, two, _) = timers.keypress().unwrap();
        assert!(timers.pause.paused());
        timers.fired(first, &one);
        assert!(!timers.pause.paused());
        timers.indefinitely();
        assert!(timers.pause.paused());
        timers.fired(second, &two);
        assert!(!timers.pause.paused());
        let (third, three, _) = timers.keypress().unwrap();
        let (_fourth, _four, _) = timers.keypress().unwrap();
        timers.fired(third, &three);
        assert!(timers.pause.paused());
    }
    #[test]
    fn callback_identity_and_initial_sentinel_share_only_actual_owned_handles() {
        let mut timers = Timers::new();
        let (first, one, _) = timers.keypress().unwrap();
        assert_eq!(first.value(), 1);
        timers.fired(first, &Arc::new(()));
        assert!(timers.pause.paused());
        timers.indefinitely(); // Cancels the real first timer, whose handle is1.
        timers.fired(first, &one);
        assert!(timers.pause.paused());
        timers.pause.unpause();
        assert!(!timers.pause.paused());
        assert!(timers.live.is_empty());
    }
}
