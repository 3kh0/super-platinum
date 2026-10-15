//! One armed diagnostic fault at a time. No catalog/Loaded/selection owner.
use super::{ledger, native};
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use super_platinum_core::slack::{Error, models::Emoji};

enum Release {
    Local(mpsc::Receiver<()>),
    Native(native::Event),
}
struct Hold {
    account: u64,
    revision: u64,
    remote: bool,
    release: Release,
}
static HOLD: Mutex<Option<Hold>> = Mutex::new(None);
static RELEASE: Mutex<Option<mpsc::SyncSender<()>>> = Mutex::new(None);
static REACHED: AtomicU64 = AtomicU64::new(0);
static DONE: AtomicU64 = AtomicU64::new(0);
static CHANGED: tokio::sync::Notify = tokio::sync::Notify::const_new();
static REJECTED: AtomicU64 = AtomicU64::new(0);
static WAITING: AtomicU64 = AtomicU64::new(0);
static WAITING_MOUNT: AtomicU64 = AtomicU64::new(0);
struct Remote {
    account: u64,
    revision: u64,
    rows: Vec<Emoji>,
}
static REMOTE: Mutex<Option<Remote>> = Mutex::new(None);

pub(super) fn arm(
    account: u64,
    revision: u64,
    remote: bool,
    event: Option<native::Event>,
) -> Result<(), String> {
    if account > 3 || revision == 0 {
        return Err("Invalid hold selector".into());
    }
    let mut hold = HOLD.lock().map_err(|_| "Hold lock poisoned")?;
    let mut release = RELEASE.lock().map_err(|_| "Release lock poisoned")?;
    if hold.is_some()
        || release.is_some()
        || (REACHED.load(Ordering::Acquire) != 0 && DONE.load(Ordering::Acquire) == 0)
    {
        return Err("Existing hold not settled".into());
    }
    REACHED.store(0, Ordering::Release);
    DONE.store(0, Ordering::Release);
    let source = if let Some(event) = event {
        Release::Native(event)
    } else {
        let (tx, rx) = mpsc::sync_channel(1);
        *release = Some(tx);
        Release::Local(rx)
    };
    *hold = Some(Hold {
        account,
        revision,
        remote,
        release: source,
    });
    Ok(())
}
pub(super) fn hold(generation: u64, revision: u64) {
    let hold = {
        let mut hold = HOLD.lock().expect("diagnostic hold lock");
        if hold.as_ref().is_none_or(|h| {
            h.account != generation >> 48
                || h.revision != revision
                || (generation & 3) != (if h.remote { 2 } else { 1 })
        }) {
            return;
        }
        hold.take().unwrap()
    };
    ledger::emit(ledger::event::HOLD_ENTER, generation, 0, 0);
    REACHED.store(generation, Ordering::Release);
    CHANGED.notify_one();
    let result = match hold.release {
        Release::Local(rx) => rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string()),
        Release::Native(event) => event.wait(generation),
    };
    if result.is_err() {
        ledger::emit(ledger::event::FAILURE, generation, 0, 30);
    }
    ledger::emit(
        ledger::event::HOLD_RELEASE,
        generation,
        0,
        u64::from(result.is_err()),
    );
    DONE.store(generation, Ordering::Release);
    CHANGED.notify_one();
}
pub(super) async fn reached() -> Result<u64, String> {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let generation = REACHED.load(Ordering::Acquire);
            if generation != 0 {
                return generation;
            }
            CHANGED.notified().await;
        }
    })
    .await
    .map_err(|_| "Build hold was not reached".into())
}
pub(super) fn outcome(generation: u64, published: bool) {
    ledger::emit(
        if published {
            ledger::event::PUBLISHED
        } else {
            ledger::event::REJECTED
        },
        generation,
        0,
        0,
    );
    if !published {
        REJECTED.store(generation, Ordering::Release);
    }
    CHANGED.notify_one();
}
pub(super) async fn changed() {
    CHANGED.notified().await;
}
pub(super) fn rejected(generation: u64) -> bool {
    REJECTED.load(Ordering::Acquire) == generation
}
pub(super) fn worker_wait(generation: u64, mount: u64) {
    ledger::emit(ledger::event::WORKER_WAIT, generation, 0, mount);
    WAITING_MOUNT.store(mount, Ordering::Release);
    WAITING.store(generation, Ordering::Release);
    CHANGED.notify_one();
}
pub(super) fn waiting(generation: u64, mount: u64) -> bool {
    WAITING.load(Ordering::Acquire) == generation && WAITING_MOUNT.load(Ordering::Acquire) == mount
}
pub(super) fn worker_active() {
    let previous = WAITING.swap(0, Ordering::AcqRel);
    WAITING_MOUNT.store(0, Ordering::Release);
    ledger::emit(ledger::event::WORKER_ACTIVE, previous, 0, 0);
}
pub(super) fn release() -> Result<(), String> {
    let tx = RELEASE
        .lock()
        .map_err(|_| "Release lock poisoned")?
        .take()
        .ok_or("No local release")?;
    tx.try_send(()).map_err(|_| "Build release failed".into())
}
pub(super) fn arm_remote(account: u64, rows: Vec<Emoji>) -> Result<(), String> {
    let mut slot = REMOTE.lock().map_err(|_| "Remote slot poisoned")?;
    if account > 2 || slot.is_some() {
        return Err("Remote arm refused".into());
    }
    *slot = Some(Remote {
        account,
        revision: 5,
        rows,
    });
    Ok(())
}
pub(super) fn remote_allowed(account: u64, revision: u64) -> bool {
    REMOTE.lock().is_ok_and(|slot| {
        slot.as_ref()
            .is_some_and(|r| r.account == account && r.revision == revision)
    })
}
pub(super) fn remote_response(
    account: u64,
    revision: u64,
    ticket: u64,
) -> Option<Result<Vec<Emoji>, Error>> {
    let mut slot = REMOTE.lock().ok()?;
    if slot
        .as_ref()
        .is_none_or(|r| r.account != account || r.revision != revision)
    {
        return None;
    }
    let response = slot.take().unwrap();
    ledger::emit(
        ledger::event::REMOTE_FETCH,
        ledger::generation(account, ticket, true),
        0,
        0,
    );
    Some(Ok(response.rows))
}
