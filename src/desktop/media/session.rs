//! Immutable shell incarnation, authenticated source ownership and queued jobs.
use super::{
    MediaRegistry, MediaSource,
    loading::{Fetch, Hooks, Mode},
};
use crate::state::ShellState;
use dioxus::prelude::{ReadableExt, Signal};
use std::{
    future::Future,
    hash::{Hash, Hasher},
    sync::Arc,
};
use super_platinum_core::slack::{Error, Transport};
#[derive(Clone)]
pub(super) struct Scope(Arc<Binding>);
struct Binding {
    account: Option<String>,
    retired: tokio::sync::watch::Sender<bool>,
}
impl Scope {
    fn fresh(account: Option<String>) -> Self {
        let (retired, _) = tokio::sync::watch::channel(false);
        Self(Arc::new(Binding { account, retired }))
    }
    pub(super) fn active(&self) -> bool {
        !*self.0.retired.borrow()
    }
    pub(super) fn retire(&self) {
        self.0.retired.send_replace(true);
    }
    pub(super) async fn retired(&self) {
        let mut receiver = self.0.retired.subscribe();
        let _ = receiver.wait_for(|retired| *retired).await;
    }
}
impl Default for Scope {
    fn default() -> Self {
        Self::fresh(None)
    }
}
impl PartialEq for Scope {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for Scope {}
impl Hash for Scope {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::ptr::hash(Arc::as_ptr(&self.0), state);
    }
}
impl MediaRegistry {
    pub(crate) fn for_session(&self, account: Option<String>) -> Self {
        Self {
            scope: Scope::fresh(account),
            ..self.clone()
        }
    }
    pub(crate) fn retire(&self) {
        self.scope.retire();
    }
    pub(super) fn eligible(&self, source: &MediaSource) -> bool {
        self.scope.active() && (!source.authenticated || source.scope.as_ref() == Some(&self.scope))
    }
}
struct Job {
    state: Signal<ShellState>,
    media: MediaRegistry,
    transport: Arc<Transport>,
    account: Option<String>,
    epoch: u64,
    cookie: String,
    mode: Mode,
}
impl Job {
    fn capture(state: Signal<ShellState>, transport: Arc<Transport>) -> Option<Self> {
        let shell = state.try_peek().ok()?;
        let cookie = shell.core.session.as_ref()?.d_cookie.clone();
        let mode = if transport.health() == super::Health::Offline {
            Mode::CacheOnly
        } else {
            Mode::Network
        };
        let job = Self {
            state,
            media: shell.media.clone(),
            transport,
            account: shell.core.active_account.clone(),
            epoch: shell.core.account_epoch,
            cookie,
            mode,
        };
        job.current().then_some(job)
    }
    fn current(&self) -> bool {
        self.state.try_peek().is_ok_and(|shell| {
            self.media.scope.active()
                && self.media.scope == shell.media.scope
                && self.media.scope.0.account == shell.core.active_account
                && self.account == shell.core.active_account
                && self.epoch == shell.core.account_epoch
                && shell
                    .core
                    .transport
                    .as_ref()
                    .is_some_and(|transport| Arc::ptr_eq(transport, &self.transport))
                && self.transport.d_cookie() == self.cookie
                && shell
                    .core
                    .session
                    .as_ref()
                    .is_some_and(|session| session.d_cookie == self.cookie)
        })
    }
    async fn run_with<F, Fut>(self, fetch: F, hooks: Hooks)
    where
        F: Fn(Fetch) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<Vec<u8>, Error>> + Send + 'static,
    {
        if !self.current() {
            return;
        }
        self.media
            .load_pending_with(
                self.transport.clone(),
                || self.current(),
                self.mode,
                fetch,
                hooks,
            )
            .await;
    }
}
/// Capture before spawning or yielding: retained credentials never select a
/// replacement shell's registry on the first poll.
pub(crate) fn load(
    state: Signal<ShellState>,
    transport: Arc<Transport>,
) -> impl Future<Output = ()> {
    let job = Job::capture(state, transport);
    async move {
        if let Some(job) = job {
            job.run_with(super::loading::fetch, Hooks::default()).await;
        }
    }
}
#[cfg(test)]
pub(crate) fn load_with<F, Fut>(
    state: Signal<ShellState>,
    transport: Arc<Transport>,
    fetch: F,
    hooks: Hooks,
) -> impl Future<Output = ()>
where
    F: Fn(Fetch) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<Vec<u8>, Error>> + Send + 'static,
{
    let job = Job::capture(state, transport);
    async move {
        if let Some(job) = job {
            job.run_with(fetch, hooks).await;
        }
    }
}
