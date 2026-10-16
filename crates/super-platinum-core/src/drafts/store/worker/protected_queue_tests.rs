use super::*;
use std::time::{Duration, Instant};

fn fresh() -> (Arc<Queue>, PathBuf) {
    let account = uuid::Uuid::new_v4().to_string();
    let path = std::env::temp_dir().join(format!("isolated-shutdown-{}", uuid::Uuid::new_v4()));
    let Route::Isolated(queue) = create(&account, path.clone()).unwrap() else {
        panic!("isolated route required");
    };
    (queue, path)
}

// Own Worker only, never Queue/Route: diagnostics cannot defer the Drop tested.
// Release both gates and rescue the wait even if an assertion unwinds.
struct Rescue {
    worker: Arc<Worker>,
    stopping: Arc<AtomicBool>,
    wait_release: Option<mpsc::Sender<()>>,
    stop_release: Option<mpsc::Sender<()>>,
}
impl Rescue {
    fn new(queue: &Queue) -> Self {
        Self {
            worker: queue.worker.clone(),
            stopping: queue.stopping.clone(),
            wait_release: None,
            stop_release: None,
        }
    }
    fn gate_wait(&mut self, queue: &Queue) -> mpsc::Receiver<()> {
        let (entered, receiver) = mpsc::channel();
        let (release, released) = mpsc::channel();
        {
            let _pending = self.worker.pending.lock().unwrap();
            *queue.shutdown_test.before_wait.lock().unwrap() = Some((entered, released));
        }
        self.wait_release = Some(release);
        self.worker.ready.notify_all();
        receiver
    }
    fn release_wait(&mut self) {
        if let Some(release) = self.wait_release.take() {
            let _ = release.send(());
        }
    }
    fn rescue(&mut self) {
        self.release_wait();
        if let Some(release) = self.stop_release.take() {
            let _ = release.send(());
        }
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            match self.worker.pending.try_lock() {
                Ok(_pending) => {
                    self.stopping.store(true, Ordering::Release);
                    self.worker.ready.notify_all();
                    return;
                }
                Err(std::sync::TryLockError::Poisoned(_)) => return,
                Err(std::sync::TryLockError::WouldBlock) => {
                    if Instant::now() >= deadline {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }
}
impl Drop for Rescue {
    fn drop(&mut self) {
        self.rescue();
    }
}

#[test]
fn old_isolated_stop_notification_precedes_wait_and_is_rescued() {
    let (queue, _) = fresh();
    let mut rescue = Rescue::new(&queue);
    let entered = rescue.gate_wait(&queue);
    entered.recv_timeout(Duration::from_secs(3)).unwrap();
    // Actual worker holds Pending after checking stop=false, before wait.
    // Reproduce only the old publication ordering, not a substitute Drop.
    queue.stopping.store(true, Ordering::Release);
    queue.worker.ready.notify_all();
    rescue.release_wait();
    {
        let _pending = queue.worker.pending.lock().unwrap();
        assert!(queue.stopping.load(Ordering::Acquire));
    }
    // Spurious wakes are allowed; liveness is not proof of notification retention.
    rescue.rescue();
    let (done, finished) = mpsc::channel();
    let dropper = std::thread::spawn(move || {
        drop(queue);
        let _ = done.send(());
    });
    finished.recv_timeout(Duration::from_secs(3)).unwrap();
    dropper.join().unwrap();
}

#[test]
fn actual_isolated_drop_publishes_stop_under_pending_before_releasing_lock() {
    let (queue, path) = fresh();
    let marker = std::fs::read(path.join("fixture.json")).unwrap();
    let mut rescue = Rescue::new(&queue);
    let entered = rescue.gate_wait(&queue);
    entered.recv_timeout(Duration::from_secs(3)).unwrap();
    rescue.release_wait();
    {
        // Ensure the real worker reached wait's atomic mutex release.
        let _pending = queue.worker.pending.lock().unwrap();
    }
    let (at_stop, boundary) = mpsc::channel();
    let (release, released) = mpsc::channel();
    *queue.shutdown_test.before_stop.lock().unwrap() = Some((at_stop, released));
    rescue.stop_release = Some(release);
    let (done, finished) = mpsc::channel();
    let dropper = std::thread::spawn(move || {
        drop(queue);
        let _ = done.send(());
    });
    // This hook is immediately before stop.store in the real Drop. Moving the
    // publication boundary outside Pending must fail even without a lost wake.
    assert!(boundary.recv_timeout(Duration::from_secs(3)).unwrap());
    assert!(!rescue.stopping.load(Ordering::Acquire));
    assert!(matches!(
        rescue.worker.pending.try_lock(),
        Err(std::sync::TryLockError::WouldBlock)
    ));
    #[cfg(windows)]
    assert!(File::open(path.join("fixture.lock")).is_err());
    rescue.stop_release.take().unwrap().send(()).unwrap();
    finished.recv_timeout(Duration::from_secs(3)).unwrap();
    dropper.join().unwrap();
    assert!(rescue.stopping.load(Ordering::Acquire));
    assert!(File::open(path.join("fixture.lock")).is_ok());
    assert_eq!(std::fs::read(path.join("fixture.json")).unwrap(), marker);
    // The constructor still rejects an existing directory; shutdown creates no
    // implicit reopen path and retains the fixture evidence on disk.
    assert!(create(&uuid::Uuid::new_v4().to_string(), path).is_err());
}
