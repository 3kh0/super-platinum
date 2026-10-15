//! Wait for one actual submitted record; never certifies whole-route readiness.
use super::*;

impl Worker {
    pub(in crate::drafts::store::worker) fn wait_accounted(
        &self,
        key: &Key,
        id: u64,
    ) -> Result<(), Reason> {
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        loop {
            let admission = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
            admission.entry(key)?;
            let book = pending.accounting.as_ref().ok_or(Reason::Incomplete)?;
            if book.terminal(id).is_some() {
                return Ok(());
            }
            if admission.dead {
                return Err(Reason::Dead);
            }
            if !book.waiting(id) {
                return Err(Reason::Incomplete);
            }
            // Separate Condvar from the work queue: waiters must never consume
            // Worker.ready's notify_one intended to wake its executor.
            #[cfg(test)]
            self.accounted_waiters
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let awakened = self.accounted.wait(pending);
            #[cfg(test)]
            self.accounted_waiters
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            pending = awakened.map_err(|_| Reason::Incomplete)?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    // Synchronization primitive only: no storage success or drain is asserted.
    fn setup() -> (Arc<Worker>, Key, accounting::Batch) {
        let owner = crate::drafts::store::protected::upgrade::Owned::fresh().unwrap();
        let worker = Arc::new(Worker::default());
        let mut state = State::new(owner.clone(), "terminal-test");
        let key = state.register(1, None, "test-accounted-only", Arc::new(()));
        let mut book = accounting::Book::owned(owner.account(), "terminal-test");
        assert_eq!(
            book.request(
                0,
                accounting::Identity {
                    account: owner.account(),
                    family: "test-accounted-only",
                    key: None
                }
            ),
            Some(1)
        );
        let batch = book.detach(1, 0);
        let mut pending = worker.pending.lock().unwrap();
        pending.admission = Some(state);
        pending.accounting = Some(book);
        drop(pending);
        (worker, key, batch)
    }
    fn blocked(worker: &Worker) {
        let end = std::time::Instant::now() + Duration::from_secs(3);
        while worker
            .accounted_waiters
            .load(std::sync::atomic::Ordering::SeqCst)
            == 0
        {
            assert!(
                std::time::Instant::now() < end,
                "waiter did not enter actual terminal wait"
            );
            std::thread::yield_now();
        }
        // Taking Pending synchronizes after Condvar::wait atomically released
        // it, so the test cannot satisfy this barrier before the actual wait.
        drop(worker.pending.lock().unwrap());
    }
    #[test]
    fn reply_does_not_certify_book_and_actual_finish_notifies_terminal_waiter() {
        let (worker, key, batch) = setup();
        let (entered, entry) = mpsc::channel();
        let (done, result) = mpsc::channel();
        let waiting = worker.clone();
        let handle = std::thread::spawn(move || {
            entered.send(()).unwrap();
            done.send(waiting.wait_accounted(&key, 1)).unwrap();
        });
        entry.recv_timeout(Duration::from_secs(3)).unwrap();
        blocked(&worker);
        assert!(matches!(result.try_recv(), Err(mpsc::TryRecvError::Empty)));
        let mut executing = accounting::Executing::new(&worker, batch);
        executing.finish_request(0, terminal::accepted(&()));
        assert_eq!(result.recv_timeout(Duration::from_secs(3)).unwrap(), Ok(()));
        handle.join().unwrap();
    }
    #[test]
    fn abandoned_execution_wakes_waiter_without_manufacturing_success() {
        let (worker, key, batch) = setup();
        let waiting = worker.clone();
        let (send, receive) = mpsc::channel();
        let handle =
            std::thread::spawn(move || send.send(waiting.wait_accounted(&key, 1)).unwrap());
        blocked(&worker);
        drop(accounting::Executing::new(&worker, batch));
        assert_eq!(
            receive.recv_timeout(Duration::from_secs(3)).unwrap(),
            Err(Reason::Incomplete)
        );
        handle.join().unwrap();
    }
}
