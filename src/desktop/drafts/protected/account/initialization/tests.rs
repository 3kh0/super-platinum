use super::super::super::{DraftKey, LocalDraft, Scope};
use super::super::wait;
use super::*;
use std::time::Duration;
#[path = "fixture.rs"]
mod fixture;
use fixture::*;
#[path = "owned_tests.rs"]
mod owned;

pub(super) struct WaitGate {
    entered: tokio::sync::oneshot::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
}
impl Holder {
    pub(in super::super) fn hold_wait(
        &self,
    ) -> (
        tokio::sync::oneshot::Receiver<()>,
        std::sync::mpsc::Sender<()>,
    ) {
        let (entered, received) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        let mut gate = self.gate.lock().unwrap();
        assert!(gate.is_none());
        *gate = Some(WaitGate {
            entered,
            release: released,
        });
        (received, release)
    }
    pub(super) fn wait_gate(&self) {
        let gate = self.gate.lock().unwrap().take();
        if let Some(gate) = gate {
            let _ = gate.entered.send(());
            gate.release.recv_timeout(Duration::from_secs(10)).unwrap();
        }
    }
}

#[tokio::test]
async fn concurrent_waiters_recover_exact_load_after_first_waiter_is_canceled() {
    let f = Fixture::new();
    let (entered, release) = f.account.initialization.hold_wait();
    let before = f.account.ownership_inventory().unwrap();
    let first_account = f.account.clone();
    let first = tokio::spawn(async move { first_account.initialize().await });
    entered.await.unwrap();
    let task_id = f.task_id();
    assert_eq!(before.validate(&f.account), Err(ownership::Error::Changed));
    let second_account = f.account.clone();
    let second = tokio::spawn(async move { second_account.initialize().await });
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    assert!(matches!(f.account.client(), Err(durable::Error::Pending)));
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), second)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let result = f.loaded();
    for _ in 0..3 {
        f.account.initialize().await.unwrap();
    }
    assert_eq!(f.task_id(), task_id);
    assert!(Arc::ptr_eq(&result, &f.loaded()));
    assert!(f.account.client().is_ok());
}

#[tokio::test]
async fn no_ui_waiter_is_needed_to_publish_actual_success_before_notification() {
    let f = Fixture::new();
    let (entered, release) = f.account.initialization.hold_wait();
    let account = f.account.clone();
    let waiter = tokio::spawn(async move { account.initialize().await });
    entered.await.unwrap();
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    let pending = f.account.ownership_inventory().unwrap();
    let mut changed = f.account.initialization.changed.subscribe();
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if f.status() == Status::Received {
                break;
            }
            changed.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert!(f.account.ready.load(Ordering::Acquire));
    assert!(f.account.client().is_ok());
    assert_eq!(pending.validate(&f.account), Err(ownership::Error::Changed));
    let actual = f.loaded();
    f.account.initialize().await.unwrap();
    assert!(Arc::ptr_eq(&actual, &f.loaded()));
}

#[tokio::test]
async fn repeated_initialize_preserves_actual_live_stream_and_historical_inventory() {
    let f = Fixture::new();
    f.account.initialize().await.unwrap();
    let initial = f.loaded();
    assert!(initial.locals().is_empty());
    let scope = Box::pin(enroll(&f.account)).await;
    let client = f.account.client().unwrap();
    let state = wait(client.stream_state("T1".into())).await.unwrap();
    let stream = wait(client.open_stream("T1".into(), state)).await.unwrap();
    let task_id = f.task_id();
    for _ in 0..3 {
        f.account.initialize().await.unwrap();
    }
    assert!(
        stream.is_live(),
        "a second AfterRestart revokes actual Stream"
    );
    assert_eq!(f.task_id(), task_id);
    assert!(Arc::ptr_eq(&initial, &f.loaded()));
    assert!(
        initial.locals().is_empty(),
        "retained initial Loaded is historical"
    );
    let current = f.account.inventory().await.unwrap();
    assert_eq!(
        current
            .managed()
            .get(&scope.key)
            .unwrap()
            .local()
            .composer
            .text,
        "Newer local"
    );
    assert!(stream.is_live(), "separate inventory uses Current");
}

#[tokio::test]
async fn actual_worker_error_stays_terminal_after_private_marker_repair() {
    let f = Fixture::new();
    let marker = f.path.join("fixture.json");
    let original = std::fs::read(&marker).unwrap();
    std::fs::write(&marker, b"wrong isolated marker").unwrap();
    assert_eq!(f.account.initialize().await, Err(durable::Error::Storage));
    assert_eq!(f.status(), Status::Received);
    assert!(matches!(
        f.account.initialization.state.lock().unwrap().outcome,
        Some(Err(durable::Error::Storage))
    ));
    let task_id = f.task_id();
    let failed = f.account.ownership_inventory().unwrap();
    std::fs::write(marker, original).unwrap();
    for _ in 0..3 {
        assert_eq!(f.account.initialize().await, Err(durable::Error::Storage));
    }
    assert_eq!(f.task_id(), task_id);
    failed.validate(&f.account).unwrap();
    assert!(!f.account.ready.load(Ordering::Acquire));
    assert!(matches!(f.account.client(), Err(durable::Error::Pending)));
}

#[test]
fn actual_queued_blocking_task_abort_retains_reply_and_never_replays() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    let (entered_tx, entered) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let blocker = runtime.spawn_blocking(move || {
        entered_tx.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(10)).unwrap();
    });
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let f = Fixture::new();
    {
        let _entered = runtime.enter();
        f.account.initialization.begin(&f.account).unwrap();
    }
    let pending = f.account.ownership_inventory().unwrap();
    let task_id = f.task_id();
    let abort = f
        .account
        .initialization
        .state
        .lock()
        .unwrap()
        .task
        .as_ref()
        .unwrap()
        .abort_handle();
    abort.abort();
    release.send(()).unwrap();
    runtime.block_on(async {
        blocker.await.unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), f.account.initialize())
                .await
                .unwrap(),
            Err(durable::Error::Storage)
        );
    });
    assert_eq!(f.status(), Status::ExecutorLost);
    assert_eq!(pending.validate(&f.account), Err(ownership::Error::Changed));
    {
        let state = f.account.initialization.state.lock().unwrap();
        assert!(
            state.reply.is_some(),
            "actual receiver was never moved to a polled task"
        );
        assert!(
            state.outcome.is_none(),
            "executor loss is not a fabricated worker Storage"
        );
    }
    assert_eq!(
        runtime.block_on(f.account.initialize()),
        Err(durable::Error::Storage)
    );
    assert_eq!(f.task_id(), task_id);
    assert!(!f.account.ready.load(Ordering::Acquire));
}

#[test]
fn missing_runtime_is_pre_admission_refusal_then_actual_first_load_can_start() {
    let f = Fixture::new();
    let before = f.account.ownership_inventory().unwrap();
    let mut future = Box::pin(f.account.initialize());
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    assert_eq!(
        std::future::Future::poll(future.as_mut(), &mut cx),
        std::task::Poll::Ready(Err(durable::Error::Storage))
    );
    drop(future);
    assert_eq!(f.status(), Status::MissingRuntime);
    assert_eq!(before.validate(&f.account), Err(ownership::Error::Changed));
    {
        let state = f.account.initialization.state.lock().unwrap();
        assert!(state.reply.is_none() && state.task.is_none() && state.outcome.is_none());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(f.account.initialize()).unwrap();
    assert_eq!(f.status(), Status::Received);
    assert!(f.account.client().is_ok());
}

#[tokio::test]
async fn actual_success_survives_completion_guard_destruction_and_read_only_revisits() {
    let f = Fixture::new();
    f.account.initialize().await.unwrap();
    let result = f.loaded();
    let observed = f.account.ownership_inventory().unwrap();
    assert_eq!(observed.initialization_status(), Status::Received);
    for _ in 0..3 {
        f.account.initialize().await.unwrap();
    }
    observed.validate(&f.account).unwrap();
    assert!(Arc::ptr_eq(&result, &f.loaded()));
    let replacement = Account::isolated(f.account.id().into(), f._store.client());
    assert_eq!(
        observed.validate(&replacement),
        Err(ownership::Error::Replaced)
    );
}

#[test]
fn poisoned_initialization_is_unknown_not_empty_or_ready() {
    let f = Fixture::new();
    let before = f.account.ownership_inventory().unwrap();
    let holder = f.account.initialization.clone();
    assert!(
        std::thread::spawn(move || {
            let _guard = holder.state.lock().unwrap();
            panic!("poison actual custody mutex");
        })
        .join()
        .is_err()
    );
    assert_eq!(before.validate(&f.account), Err(ownership::Error::Unknown));
    assert!(matches!(
        f.account.ownership_inventory(),
        Err(ownership::Error::Unknown)
    ));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert_eq!(
        runtime.block_on(f.account.initialize()),
        Err(durable::Error::Storage)
    );
    assert!(!f.account.ready.load(Ordering::Acquire));
}
