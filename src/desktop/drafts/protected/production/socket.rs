//! One actual accepted socket, retained before the first managed destination.
//! Opening a durable Stream never manufactures enrollment or a second worker.
use super::*;
use crate::drafts::observation::{self, WorkerSource};
use dioxus::prelude::{ReadableExt, Signal};
use std::sync::{
    Weak,
    atomic::{AtomicBool, Ordering},
};
use super_platinum_core::slack::realtime::ConnectParams;

type Key = (String, String);
fn connections() -> &'static Mutex<BTreeMap<Key, Weak<Connection>>> {
    static CONNECTIONS: OnceLock<Mutex<BTreeMap<Key, Weak<Connection>>>> = OnceLock::new();
    CONNECTIONS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(super) fn capture(shell: &ShellState, team: &str) -> Result<Arc<Connection>, durable::Error> {
    let key = (
        shell
            .core
            .active_account
            .clone()
            .ok_or(durable::Error::Stale)?,
        team.to_owned(),
    );
    let connection = connections()
        .lock()
        .map_err(|_| durable::Error::Storage)?
        .get(&key)
        .and_then(Weak::upgrade)
        .ok_or(durable::Error::Gap)?;
    connection
        .current(shell)
        .then_some(connection)
        .ok_or(durable::Error::Gap)
}
pub(crate) fn current(shell: &ShellState, team: &str) -> Result<Arc<Source>, durable::Error> {
    capture(shell, team)?.source(shell)
}

pub(super) struct Connection {
    account: Arc<Account>,
    team: String,
    socket: Arc<observation::Source>,
    generation: u64,
    live: AtomicBool,
    gate: tokio::sync::Mutex<()>,
    held: Mutex<Held>,
}
#[derive(Default)]
struct Held {
    source: Option<Arc<Source>>,
    ingress: Option<Arc<ObservationQueue>>,
    // Once an actual Source job has failed, no unrelated Send retries it under
    // replacement authority. Its Account opening and result remain retained.
    opening_error: Option<durable::Error>,
    // Empty-team deferral is distinct from a submitted failed opening. Retain
    // the original Stale observation even after enrollment permits opening.
    no_destination: Option<durable::Error>,
}
impl Connection {
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.live.load(Ordering::Acquire)
            && shell.core.active_account.as_deref() == Some(self.account.id())
            && self.socket.matches_owner(self.account.id(), &self.team)
            && self.socket.current(shell, self.generation)
    }
    fn source(&self, shell: &ShellState) -> Result<Arc<Source>, durable::Error> {
        if !self.current(shell) {
            return Err(durable::Error::Gap);
        }
        let held = self.held.lock().map_err(|_| durable::Error::Storage)?;
        if let Some(error) = held.opening_error {
            return Err(error);
        }
        let source = held.source.clone().ok_or(durable::Error::Stale)?;
        source
            .current(shell)
            .then_some(source)
            .ok_or(durable::Error::Gap)
    }
    pub(super) async fn ensure(
        &self,
        state: Signal<ShellState>,
    ) -> Result<Arc<Source>, durable::Error> {
        self.ensure_optional(state)
            .await?
            .ok_or(durable::Error::Stale)
    }
    async fn ensure_optional(
        &self,
        state: Signal<ShellState>,
    ) -> Result<Option<Arc<Source>>, durable::Error> {
        let _gate = self.gate.lock().await;
        if !self.current(&state.peek()) {
            return Err(durable::Error::Gap);
        }
        {
            let held = self.held.lock().map_err(|_| durable::Error::Storage)?;
            if let Some(error) = held.opening_error {
                return Err(error);
            }
            if let Some(source) = &held.source {
                return source
                    .current(&state.peek())
                    .then(|| Some(source.clone()))
                    .ok_or(durable::Error::Gap);
            }
        }
        let loaded = self.account.inventory().await?;
        if !self.current(&state.peek()) {
            return Err(durable::Error::Gap);
        }
        if !loaded.managed().keys().any(|key| key.team == self.team) {
            self.held
                .lock()
                .map_err(|_| durable::Error::Storage)?
                .no_destination = Some(durable::Error::Stale);
            return Ok(None);
        }
        // Serialized with other first-send/frame callers. The actual socket is
        // revalidated by Source before and after each storage await as well.
        let result = Source::open(
            state,
            self.account.clone(),
            self.socket.clone(),
            self.generation,
            self.team.clone(),
        )
        .await;
        let source = match result {
            Ok(source) => source,
            Err(error) => {
                self.held
                    .lock()
                    .map_err(|_| durable::Error::Storage)?
                    .opening_error = Some(error);
                return Err(error);
            }
        };
        if !self.current(&state.peek()) || !source.current(&state.peek()) {
            source.gap(durable::GapReason::SourceReplaced);
            return Err(durable::Error::Stale);
        }
        let ingress = match self.account.open_ingress(source.clone()) {
            Ok(ingress) => ingress,
            Err(error) => {
                source.gap(durable::GapReason::InterruptedAdmission);
                self.held
                    .lock()
                    .map_err(|_| durable::Error::Storage)?
                    .opening_error = Some(error);
                return Err(error);
            }
        };
        let mut held = self.held.lock().map_err(|_| durable::Error::Storage)?;
        // Recheck while publishing under the same held lock as revocation.
        // A revoke that won before publication cannot miss this new Source.
        if !self.current(&state.peek()) {
            source.gap(durable::GapReason::SourceReplaced);
            return Err(durable::Error::Gap);
        }
        held.source = Some(source.clone());
        held.ingress = Some(ingress);
        Ok(Some(source))
    }
    fn revoke(&self, reason: durable::GapReason) {
        self.live.store(false, Ordering::Release);
        if let Ok(held) = self.held.lock() {
            if let Some(source) = &held.source {
                source.gap(reason);
            }
        }
    }
    async fn close(&self, reason: durable::GapReason) -> Result<(), durable::Error> {
        self.revoke(reason);
        let _gate = self.gate.lock().await;
        let source = self
            .held
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .source
            .clone();
        if let Some(source) = source {
            source.close(reason).await?;
        }
        let mut held = self.held.lock().map_err(|_| durable::Error::Storage)?;
        held.ingress = None;
        held.source = None;
        Ok(())
    }
    async fn event(
        &self,
        state: Signal<ShellState>,
        raw: super_platinum_core::slack::events::RawEvent,
    ) -> Result<(), durable::Error> {
        if !self.current(&state.peek()) {
            return Err(durable::Error::Gap);
        }
        // Before the first enrolled destination there is no draft projection
        // authority. Ordinary realtime still handles the frame. First Send
        // explicitly ensures this connection after enrollment; no per-frame
        // inventory read or timer attempts to manufacture continuity.
        let ingress = {
            let held = self.held.lock().map_err(|_| durable::Error::Storage)?;
            if let Some(error) = held.opening_error {
                return Err(error);
            }
            match &held.ingress {
                Some(ingress) => ingress.clone(),
                None => return Ok(()),
            }
        };
        ingress.enqueue_frame(state, &self.team, raw).await
    }
}

pub(crate) struct Worker {
    socket: WorkerSource,
    account: Arc<Account>,
    team: String,
    connection: Option<Arc<Connection>>,
}
impl Worker {
    pub(crate) fn capture(
        shell: &mut ShellState,
        params: ConnectParams,
    ) -> Result<(Self, ConnectParams), durable::Error> {
        let account = super::account(
            shell
                .core
                .active_account
                .as_deref()
                .ok_or(durable::Error::Stale)?,
        )?;
        let team = params.team.clone();
        let mut socket = WorkerSource::capture(shell, params).map_err(|_| durable::Error::Stale)?;
        let params = socket
            .take_connect_params(shell)
            .map_err(|_| durable::Error::Stale)?;
        let prior = connections()
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .get(&(account.id().to_owned(), team.clone()))
            .and_then(Weak::upgrade);
        if let Some(prior) = &prior {
            prior.revoke(durable::GapReason::SourceReplaced);
        }
        Ok((
            Self {
                socket,
                account,
                team,
                connection: prior,
            },
            params,
        ))
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.socket.current(shell)
    }
    pub(crate) fn can_accept(&self, shell: &ShellState, team: &str, generation: u64) -> bool {
        self.socket.can_accept(shell, team, generation)
    }
    pub(crate) fn accept(
        &mut self,
        shell: &mut ShellState,
        team: &str,
        generation: u64,
    ) -> Result<Arc<observation::Source>, durable::Error> {
        self.socket
            .accept(shell, team, generation)
            .map(Arc::new)
            .map_err(|_| durable::Error::Stale)
    }
    pub(crate) async fn connected(
        &mut self,
        state: Signal<ShellState>,
        socket: Arc<observation::Source>,
        generation: u64,
    ) -> Result<(), durable::Error> {
        self.close(durable::GapReason::SourceReplaced).await?;
        let connection = Arc::new(Connection {
            account: self.account.clone(),
            team: self.team.clone(),
            socket,
            generation,
            live: AtomicBool::new(true),
            gate: tokio::sync::Mutex::new(()),
            held: Mutex::new(Held::default()),
        });
        if !self.current(&state.peek()) || !connection.current(&state.peek()) {
            return Err(durable::Error::Stale);
        }
        {
            let mut book = connections().lock().map_err(|_| durable::Error::Storage)?;
            book.retain(|_, value| value.strong_count() != 0);
            let key = (self.account.id().to_owned(), self.team.clone());
            if book
                .get(&key)
                .and_then(Weak::upgrade)
                .is_some_and(|old| old.live.load(Ordering::Acquire))
                || (book.len() >= 64 && !book.contains_key(&key))
            {
                return Err(durable::Error::Pending);
            }
            book.insert(key, Arc::downgrade(&connection));
        }
        self.connection = Some(connection.clone());
        connection.ensure_optional(state).await.map(|_| ())
    }
    pub(crate) fn disconnected(&mut self, shell: &mut ShellState, team: &str, generation: u64) {
        self.socket.disconnected(shell, team, generation);
        if let Some(c) = &self.connection {
            if !c.current(shell) {
                c.revoke(durable::GapReason::Disconnect);
            }
        }
    }
    pub(crate) async fn close(&mut self, reason: durable::GapReason) -> Result<(), durable::Error> {
        if let Some(c) = &self.connection {
            c.close(reason).await?;
        }
        self.connection = None;
        self.account.retire_closed_ingress();
        for handle in self.account.retained_source_openings()? {
            let _ = self.account.acknowledge_source_opening(&handle);
        }
        Ok(())
    }
    pub(crate) async fn event(
        &self,
        state: Signal<ShellState>,
        team: &str,
        generation: u64,
        raw: super_platinum_core::slack::events::RawEvent,
    ) -> Result<(), durable::Error> {
        if !self.current(&state.peek()) || team != self.team {
            return Err(durable::Error::Stale);
        }
        let c = self.connection.as_ref().ok_or(durable::Error::Gap)?;
        if generation != c.generation {
            return Err(durable::Error::Stale);
        }
        c.event(state, raw).await
    }
    pub(crate) fn retire(&self, shell: &mut ShellState) {
        self.socket.retire(shell);
        if let Some(c) = &self.connection {
            c.revoke(durable::GapReason::SourceReplaced);
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(c) = &self.connection {
            c.revoke(durable::GapReason::InterruptedAdmission);
        }
    }
}

#[cfg(test)]
mod tests;
