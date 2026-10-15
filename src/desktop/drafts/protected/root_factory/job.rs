//! Cancellation-retained one-shot outcome. No automatic retries or task abort.
use super::*;
pub(super) struct Job<T> {
    result: Mutex<Option<Result<Arc<T>, Error>>>,
    changed: tokio::sync::Notify,
}
pub(super) struct Wait<T>(Arc<Job<T>>);
impl<T> Job<T> {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            result: Mutex::new(None),
            changed: Default::default(),
        })
    }
    pub fn result(&self) -> Result<Arc<T>, Error> {
        self.result
            .lock()
            .map_err(|_| Error::Storage(durable::Error::Storage))?
            .as_ref()
            .cloned()
            .unwrap_or(Err(Error::Pending))
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
}
impl<T> Wait<T> {
    pub async fn ready(&self) {
        loop {
            let notified = self.0.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            // Poison is terminal failure, never an infinite wait.
            if self.0.result.lock().map_or(true, |result| result.is_some()) {
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
    pub fn finish(mut self, value: Result<T, Error>) {
        if let Ok(mut result) = self.job.result.lock() {
            *result = Some(value.map(Arc::new));
        }
        self.finished = true;
        self.job.changed.notify_waiters();
    }
}
impl<T> Drop for Completion<T> {
    fn drop(&mut self) {
        if !self.finished {
            if let Ok(mut result) = self.job.result.lock() {
                *result = Some(Err(Error::Storage(durable::Error::Storage)));
            }
            self.job.changed.notify_waiters();
        }
    }
}
