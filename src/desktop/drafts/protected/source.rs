//! Captured socket provenance plus a revocable durable continuity capability.
//! This module has no realtime receiver or production wiring.
use super::*;
use crate::{drafts::observation, state::ShellState};
use std::sync::atomic::{AtomicBool, Ordering};
use super_platinum_core::slack::events::RawEvent;

pub(super) mod inventory;
pub(super) mod jobs;

pub(crate) struct Source {
    pub(super) inventory: Arc<inventory::Record>,
    account: Arc<Account>,
    socket: Arc<observation::Source>,
    frame_generation: u64,
    stream: durable::Stream,
    revoked: Arc<AtomicBool>,
    runtime: tokio::runtime::Handle,
    cleanup: Option<Arc<jobs::Job>>,
}
impl Source {
    /// Explicit continuity setup; never called by a load, render or tick. The
    /// socket was captured by its original worker and is checked after every
    /// storage await. Canceling the UI waiter cannot abandon an opened stream.
    pub async fn open(
        state: dioxus::prelude::Signal<ShellState>,
        account: Arc<Account>,
        socket: Arc<observation::Source>,
        frame_generation: u64,
        team: String,
    ) -> Result<Arc<Self>, durable::Error> {
        jobs::Request {
            state,
            account,
            socket,
            frame_generation,
            team,
        }
        .begin()
        .map_err(|refused| refused.error)?
        .receive()
        .await
    }

    /// A local basis is an explicit full-inventory decision. Opening a socket
    /// alone neither verifies a row nor enrolls local content.
    pub async fn verify_local(
        self: &Arc<Self>,
        state: dioxus::prelude::Signal<ShellState>,
        owner: Owner,
        expected: durable::Review,
    ) -> Result<durable::Review, durable::Error> {
        use dioxus::prelude::ReadableExt;
        owner.check(&expected)?;
        let source = self.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        dioxus::core::spawn_forever(async move {
            let result = async {
                let current = owner.review().await?;
                if !current.same(&expected) {
                    return Err(durable::Error::Stale);
                }
                let stream = {
                    let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
                    source.stream(&shell)?
                };
                owner.check_stream(&stream)?;
                account::wait(owner.client()?.verify_local(stream, expected)).await
            }
            .await;
            let _ = send.send(result);
        });
        receive.await.map_err(|_| durable::Error::Storage)?
    }
    pub(super) fn captured(
        account: Arc<Account>,
        socket: Arc<observation::Source>,
        frame_generation: u64,
        stream: durable::Stream,
        shell: &ShellState,
    ) -> Result<Arc<Self>, durable::Error> {
        Self::captured_with_job(account, socket, frame_generation, stream, shell, None)
    }
    fn captured_with_job(
        account: Arc<Account>,
        socket: Arc<observation::Source>,
        frame_generation: u64,
        stream: durable::Stream,
        shell: &ShellState,
        cleanup: Option<Arc<jobs::Job>>,
    ) -> Result<Arc<Self>, durable::Error> {
        if stream.account() != account.id() {
            stream.revoke();
            return Err(durable::Error::Stale);
        }
        let runtime = tokio::runtime::Handle::try_current().ok();
        if !socket.matches_owner(account.id(), stream.team())
            || shell.core.active_account.as_deref() != Some(account.id())
            || !socket.current(shell, frame_generation)
            || account.team_blocked(stream.team())
            || runtime.is_none()
        {
            if let Some(child) = &cleanup {
                child.retire(stream, durable::GapReason::SourceReplaced);
            } else {
                account.retire_stream(stream, durable::GapReason::SourceReplaced, runtime);
            }
            return Err(durable::Error::Gap);
        }
        let revoked = Arc::new(AtomicBool::new(false));
        Ok(Arc::new(Self {
            inventory: inventory::Record::new(
                &account,
                socket.clone(),
                frame_generation,
                stream.clone(),
                revoked.clone(),
            ),
            account,
            socket,
            frame_generation,
            stream,
            revoked,
            runtime: runtime.expect("checked runtime"),
            cleanup,
        }))
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        self.live() && self.socket.current(shell, self.frame_generation)
    }
    pub(super) fn live(&self) -> bool {
        !self.revoked.load(Ordering::Acquire)
            && self.stream.is_live()
            && !self.account.team_blocked(self.stream.team())
    }
    pub(crate) fn stream(&self, shell: &ShellState) -> Result<durable::Stream, durable::Error> {
        self.current(shell)
            .then(|| self.stream.clone())
            .ok_or(durable::Error::Gap)
    }
    pub(super) fn prepare(
        &self,
        shell: &ShellState,
        key: &DraftKey,
        raw: &RawEvent,
    ) -> Result<observation::Prepared, durable::Error> {
        if !self.current(shell) || key.team != self.stream.team() {
            return Err(durable::Error::Gap);
        }
        self.socket
            .prepare(shell, self.frame_generation, key, raw)
            .map_err(|_| durable::Error::Unsupported)
    }
    /// Retirement is synchronous even when disk I/O subsequently fails. The
    /// process barrier also prevents opening a replacement stream meanwhile.
    pub fn gap(self: &Arc<Self>, reason: durable::GapReason) {
        if !self.revoked.swap(true, Ordering::AcqRel) {
            self.inventory.phase(inventory::Phase::GapRequested(reason));
            self.retire(reason);
        }
    }
    fn retire(&self, reason: durable::GapReason) {
        match &self.cleanup {
            Some(child) => child.retire(self.stream.clone(), reason),
            None => {
                self.account
                    .retire_stream(self.stream.clone(), reason, Some(self.runtime.clone()))
            }
        }
    }
    /// Explicit disk-only repair. It never opens a stream or invokes transport.
    pub fn retry_gap(self: &Arc<Self>) -> Result<(), durable::Error> {
        self.inventory.clock.change();
        match &self.cleanup {
            Some(child) => child.retry_gap(),
            None => self.account.retry_gap(self.stream.team()),
        }
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        self.inventory.phase(inventory::Phase::Dropped);
        if !self.revoked.swap(true, Ordering::AcqRel) {
            self.retire(durable::GapReason::InterruptedAdmission);
        }
    }
}
