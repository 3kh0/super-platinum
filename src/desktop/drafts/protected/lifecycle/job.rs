//! A waiter is disposable; the retained slot owns its result. No retry executor.
use super::*;
use std::sync::Mutex;
pub(super) struct Job<T> {
    result: Mutex<Option<Result<Arc<T>, durable::Error>>>,
    changed: tokio::sync::Notify,
}
pub(super) struct Wait<T>(Arc<Job<T>>);
impl<T> Clone for Wait<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T> Job<T> {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            result: Mutex::default(),
            changed: Default::default(),
        })
    }
    pub fn wait(self: &Arc<Self>) -> Wait<T> {
        Wait(self.clone())
    }
    pub fn completion(self: &Arc<Self>) -> Completion<T> {
        Completion {
            job: self.clone(),
            finished: false,
        }
    }
    pub fn result(&self) -> Result<Arc<T>, durable::Error> {
        self.result
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .as_ref()
            .cloned()
            .unwrap_or(Err(durable::Error::Pending))
    }
}
impl<T> Wait<T> {
    pub async fn ready(&self) {
        loop {
            let notified = self.0.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.0.result.lock().unwrap().is_some() {
                return;
            }
            notified.await;
        }
    }
}
pub(super) struct Completion<T> {
    job: Arc<Job<T>>,
    finished: bool,
}
impl<T> Completion<T> {
    pub fn finish(mut self, result: Result<Arc<T>, durable::Error>) {
        *self.job.result.lock().unwrap() = Some(result);
        self.finished = true;
        self.job.changed.notify_waiters();
    }
}
impl<T> Drop for Completion<T> {
    fn drop(&mut self) {
        if !self.finished {
            if let Ok(mut result) = self.job.result.lock() {
                *result = Some(Err(durable::Error::Storage));
            }
            self.job.changed.notify_waiters();
        }
    }
}
