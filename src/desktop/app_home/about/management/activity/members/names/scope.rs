//! Acquisition identity shared by picker and explicit detail reads. This does
//! not register a search, alter its generations, or manufacture policy facts.
use super::*;
use std::{future::Future, task::Poll};
use super_platinum_core::slack::PreparedRequest;

#[derive(Clone, Copy)]
pub(super) enum Scope<'a> {
    Query(&'a Query),
    Detail(&'a super::super::super::reads::Read),
    Refresh(&'a super::super::super::refresh::Attempt),
}
impl Scope<'_> {
    pub(super) fn owner(&self) -> &Owner {
        match self {
            Self::Query(q) => &q.owner,
            Self::Detail(r) => r.owner(),
            Self::Refresh(r) => r.owner(),
        }
    }
    pub(super) fn visit(&self) -> u64 {
        match self {
            Self::Query(q) => q.visit,
            Self::Detail(r) => r.visit(),
            Self::Refresh(r) => r.visit(),
        }
    }
    pub(super) fn document(&self) -> u64 {
        match self {
            Self::Query(q) => q.document,
            Self::Detail(r) => r.document(),
            Self::Refresh(r) => r.document(),
        }
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        match self {
            Self::Query(q) => q.current(s),
            Self::Detail(r) => r.current(s),
            Self::Refresh(r) => r.current(s),
        }
    }
    pub(super) async fn mutation_guard(
        &self,
        state: Signal<ShellState>,
    ) -> Result<Option<super::super::super::refresh::native::Release>, Error> {
        match self {
            Self::Refresh(r) => r.mutation_guard(state).await.map(Some),
            Self::Query(_) | Self::Detail(_) => Ok(None),
        }
    }
    async fn request(
        &self,
        state: Signal<ShellState>,
        request: PreparedRequest,
    ) -> Result<Value, Error> {
        match self {
            Self::Query(q) => read::request(state, q, request).await,
            Self::Detail(r) => r.request_prepared(state, request).await,
            Self::Refresh(r) => r.request_prepared(state, request).await,
        }
    }
}

/// A canceled acquisition may no longer lock its old native surface. Retire
/// only its internal boot nonce without notifying render subscribers; this
/// never applies a boot, installs an Observation, or changes visible state.
pub(super) struct PendingBoot {
    state: Signal<ShellState>,
    read: Option<BootRead>,
}
impl PendingBoot {
    pub(super) fn new(state: Signal<ShellState>, read: BootRead) -> Self {
        Self {
            state,
            read: Some(read),
        }
    }
    pub(super) fn take(&mut self) -> BootRead {
        self.read.take().expect("pending boot is consumed once")
    }
    pub(super) fn cancel(&mut self) -> bool {
        let Some(read) = self.read.take() else {
            return false;
        };
        if !read.current(&self.state.peek().core) {
            return false;
        }
        // Pinned Dioxus supports silent internal bookkeeping. A normal
        // write here could erase a newer uncommitted native edit.
        #[allow(deprecated)]
        let mut state = self.state.write_silent();
        read.failed(&mut state.core)
    }
}
impl Drop for PendingBoot {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub(super) struct Fence {
    pub(super) revision: u64,
    cookie: String,
    installed: Option<Arc<()>>,
}
impl Fence {
    pub(super) fn capture(s: &ShellState, scope: Scope<'_>) -> Result<Self, Error> {
        if !scope.current(s) {
            return Err(unavailable());
        }
        Ok(Self {
            revision: s.app_home.workflow_names_revision,
            cookie: s
                .core
                .session
                .as_ref()
                .ok_or_else(unavailable)?
                .d_cookie
                .clone(),
            installed: document(s, scope.visit())
                .and_then(|d| d.activity.member_names.as_ref())
                .map(|v| v.identity.clone()),
        })
    }
    pub(super) fn current(&self, s: &ShellState, scope: Scope<'_>) -> bool {
        scope.current(s)
            && self.revision == s.app_home.workflow_names_revision
            && s.core
                .session
                .as_ref()
                .is_some_and(|v| v.d_cookie == self.cookie)
            && document(s, scope.visit()).is_some_and(|d| {
                d.id == scope.document()
                    && match (&self.installed, &d.activity.member_names) {
                        (None, None) => true,
                        (Some(before), Some(after)) => Arc::ptr_eq(before, &after.identity),
                        _ => false,
                    }
            })
    }
    pub(super) async fn request(
        &self,
        state: Signal<ShellState>,
        scope: Scope<'_>,
        authority: Option<&Authority>,
        request: PreparedRequest,
    ) -> Result<Value, Error> {
        let current = || {
            self.current(&state.peek(), scope)
                && authority.is_none_or(|a| a.current(&state.peek().core))
        };
        let mut work = std::pin::pin!(scope.request(state, request));
        let guarded = std::future::poll_fn(|cx| {
            if !current() {
                return Poll::Ready(Err(unavailable()));
            }
            work.as_mut().poll(cx)
        });
        let retired = async {
            loop {
                if !current() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        };
        let result = tokio::select! {
            biased;
            _ = retired => Err(unavailable()),
            result = guarded => result,
        };
        if !current() {
            return Err(unavailable());
        }
        result
    }
}
