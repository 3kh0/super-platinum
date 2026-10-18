//! A bounded wait for native unlock observation, never a response factory.
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tokio::sync::oneshot;
pub(super) struct Gate {
    armed: AtomicBool,
    release: Mutex<Option<oneshot::Sender<()>>>,
    waiting: Mutex<Option<oneshot::Receiver<()>>>,
}
impl Gate {
    pub(super) fn new() -> Self {
        let (send, receive) = oneshot::channel();
        Self {
            armed: AtomicBool::new(false),
            release: Mutex::new(Some(send)),
            waiting: Mutex::new(Some(receive)),
        }
    }
    pub(super) fn armed(&self) -> bool {
        self.armed.load(Ordering::Acquire)
    }
    pub(super) fn cancel(&self) {
        // Poison does not justify retaining a Sender and stranding the waiter.
        self.release
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
    }
    pub(super) fn release(&self) -> Result<(), ()> {
        if !self.armed() {
            return Err(());
        }
        self.release
            .lock()
            .map_err(|_| ())?
            .take()
            .ok_or(())?
            .send(())
            .map_err(|_| ())
    }
    pub(super) async fn wait(&self) -> Result<(), ()> {
        self.wait_for(Duration::from_secs(60)).await
    }
    async fn wait_for(&self, bound: Duration) -> Result<(), ()> {
        let waiting = self.waiting.lock().map_err(|_| ())?.take().ok_or(())?;
        if self.armed.swap(true, Ordering::AcqRel) {
            return Err(());
        }
        let result = tokio::time::timeout(bound, waiting)
            .await
            .map_err(|_| ())
            .and_then(|v| v.map_err(|_| ()));
        if result.is_err() {
            self.cancel();
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[tokio::test]
    async fn driver_exit_cancels_original_wait_without_release_or_rearm() {
        let gate = Arc::new(Gate::new());
        let waiting = gate.clone();
        let task = tokio::spawn(async move { waiting.wait().await });
        while !gate.armed() {
            tokio::task::yield_now().await;
        }
        gate.cancel();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap(),
            Err(())
        );
        assert_eq!(gate.release(), Err(()));
        assert_eq!(gate.wait().await, Err(()));
    }
    #[tokio::test]
    async fn independent_deadline_drops_sender_and_refuses_a_late_release() {
        let gate = Gate::new();
        assert_eq!(gate.wait_for(Duration::from_millis(1)).await, Err(()));
        assert!(gate.armed());
        assert_eq!(gate.release(), Err(()));
        assert_eq!(gate.wait().await, Err(()));
    }
    #[tokio::test]
    async fn only_one_actual_armed_wait_can_be_released() {
        let gate = Arc::new(Gate::new());
        assert_eq!(gate.release(), Err(()));
        let waiting = gate.clone();
        let task = tokio::spawn(async move { waiting.wait().await });
        while !gate.armed() {
            tokio::task::yield_now().await;
        }
        gate.release().unwrap();
        assert_eq!(task.await.unwrap(), Ok(()));
        assert_eq!(gate.release(), Err(()));
        assert_eq!(gate.wait().await, Err(()));
        gate.cancel();
    }
}
