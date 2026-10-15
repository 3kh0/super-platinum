//! A canceled UI waiter cannot discard an admitted operation's completion.
use super::*;
use std::sync::Mutex;
pub(super) struct Job<T> {
    result: Mutex<Option<Result<T, durable::Error>>>,
    done: std::sync::atomic::AtomicBool,
    changed: tokio::sync::Notify,
}
pub(crate) struct Wait<T>(Arc<Job<T>>);
impl<T> Wait<T> {
    pub(super) fn new(job: Arc<Job<T>>) -> Self {
        Self(job)
    }
    pub(super) fn is(&self, job: &Arc<Job<T>>) -> bool {
        Arc::ptr_eq(&self.0, job)
    }
    /// Readiness only, not a storage/publication capability or a second result.
    pub async fn ready(&self) {
        loop {
            let notified = self.0.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.0.done.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }
}
impl<T> Job<T> {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            result: Mutex::default(),
            done: Default::default(),
            changed: tokio::sync::Notify::new(),
        })
    }
    pub fn completion(self: &Arc<Self>) -> Completion<T> {
        Completion {
            job: self.clone(),
            finished: false,
        }
    }
    pub fn take(&self) -> Result<Result<T, durable::Error>, durable::Error> {
        self.result
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .take()
            .ok_or(durable::Error::Pending)
    }
}
pub(super) struct Completion<T> {
    job: Arc<Job<T>>,
    finished: bool,
}
impl<T> Completion<T> {
    pub fn finish(mut self, result: Result<T, durable::Error>) {
        self.finished = true;
        *self.job.result.lock().unwrap() = Some(result);
        self.job.done.store(true, Ordering::Release);
        self.job.changed.notify_waiters();
    }
}
impl<T> Drop for Completion<T> {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        if let Ok(mut result) = self.job.result.lock() {
            if result.is_none() {
                *result = Some(Err(durable::Error::Storage));
            }
        }
        self.job.done.store(true, Ordering::Release);
        self.job.changed.notify_waiters();
    }
}
