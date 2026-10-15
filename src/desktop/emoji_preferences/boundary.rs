//! Private execution seams. Production always uses Native; only cfg(test)
//! supplies an in-memory clock/executor. There is no environment bypass.
use super::*;
use std::{future::Future, pin::Pin};
use super_platinum_core::slack::{Error, PreparedRequest};
pub(super) type Task = Pin<Box<dyn Future<Output = ()> + 'static>>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Work {
    History,
    Request,
}
pub(super) trait Boundary: Clone + 'static {
    fn now(&self) -> Instant;
    fn now_ms(&self) -> f64;
    fn is_fixture(&self) -> bool;
    fn spawn(&self, task: Task);
    async fn sleep(&self, wait: Duration);
    async fn work<T: Send + 'static, F: FnOnce() -> T + Send + 'static>(
        &self,
        kind: Work,
        work: F,
    ) -> Result<T, ()>;
    async fn execute(&self, owner: Arc<Owner>, request: PreparedRequest) -> Result<Value, Error>;
    fn persist(&self, state: Signal<ShellState>, team: &str);
    fn failure(&self, state: Signal<ShellState>, error: Error, message: &'static str);
}
#[derive(Clone, Copy)]
pub(super) struct Native;
impl Boundary for Native {
    fn now(&self) -> Instant {
        Instant::now()
    }
    fn now_ms(&self) -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as f64
    }
    fn is_fixture(&self) -> bool {
        crate::fixture::is_fixture()
    }
    fn spawn(&self, task: Task) {
        dioxus::core::spawn_forever(task);
    }
    async fn sleep(&self, wait: Duration) {
        tokio::time::sleep(wait).await;
    }
    async fn work<T: Send + 'static, F: FnOnce() -> T + Send + 'static>(
        &self,
        _: Work,
        work: F,
    ) -> Result<T, ()> {
        tokio::task::spawn_blocking(work).await.map_err(|_| ())
    }
    async fn execute(&self, owner: Arc<Owner>, request: PreparedRequest) -> Result<Value, Error> {
        owner
            .context
            .transport
            .execute_emoji_preferences(request)
            .await
    }
    fn persist(&self, state: Signal<ShellState>, team: &str) {
        crate::bootstrap::persist_workspace_metadata(&state, team);
    }
    fn failure(&self, mut state: Signal<ShellState>, error: Error, message: &'static str) {
        state.write().report_failure(&error, "", || message.into());
    }
}
