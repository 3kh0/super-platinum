//! Candidate only: immutable socket provenance, with no frame ingress.
use super::{Invalid, Prepared, decode};
use crate::{
    drafts::{Scope, network},
    state::ShellState,
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use super_platinum_core::{
    drafts::DraftKey,
    slack::{events::RawEvent, realtime::ConnectParams},
};

/// Process-local identity, not a durable stream lease or continuity proof.
/// The private allocation makes equal-generation replacement workers distinct.
#[derive(Clone)]
pub(crate) struct WorkerId(Arc<AtomicBool>);
impl WorkerId {
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    fn live(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

struct Slot {
    worker: WorkerId,
    generation: Option<u64>,
}
#[derive(Default)]
pub(crate) struct Sources {
    teams: BTreeMap<String, Slot>,
}

/// Owns the credentials and exact params captured before connect. No Debug or
/// Clone: a reconnect accepts a generation of this worker, never fresh secrets.
pub(crate) struct WorkerSource {
    binding: network::Binding,
    account: String,
    team: String,
    user: String,
    worker: WorkerId,
    params: Option<ConnectParams>,
    last_generation: u64,
}

pub(crate) struct Source {
    binding: network::Binding,
    account: String,
    team: String,
    user: String,
    worker: WorkerId,
    generation: u64,
}

impl Drop for WorkerSource {
    fn drop(&mut self) {
        // Async cancellation cannot borrow the shell. Reject retained proofs
        // immediately; durable continuity still needs its independent gap gate.
        self.worker.0.store(false, Ordering::Release);
    }
}

impl WorkerSource {
    /// Call under the serial shell mutation boundary before starting connect.
    /// Invalid params cannot displace an already accepted worker.
    pub fn capture(shell: &mut ShellState, params: ConnectParams) -> Result<Self, Invalid> {
        if crate::fixture::is_fixture() {
            #[cfg(feature = "protected-native-fixture")]
            let allowed = crate::drafts::protected::owned_native::permits_socket(shell);
            #[cfg(not(feature = "protected-native-fixture"))]
            let allowed = false;
            if !allowed {
                return Err(Invalid::Source);
            }
        }
        let binding =
            network::Binding::capture(shell, &params.team).map_err(|_| Invalid::Source)?;
        if !binding.matches_socket(&params) {
            return Err(Invalid::Source);
        }
        let account = shell.core.active_account.clone().ok_or(Invalid::Source)?;
        let user = shell
            .core
            .workspaces
            .get(&params.team)
            .ok_or(Invalid::Source)?
            .self_user_id
            .clone();
        let worker = WorkerId(Arc::new(AtomicBool::new(true)));
        shell.draft_writes.socket_sources.teams.insert(
            params.team.clone(),
            Slot {
                worker: worker.clone(),
                generation: None,
            },
        );
        Ok(Self {
            binding,
            account,
            team: params.team.clone(),
            user,
            worker,
            params: Some(params),
            last_generation: 0,
        })
    }

    /// Feed this returned value unchanged to realtime::connect. One launch only.
    pub fn take_connect_params(&mut self, shell: &ShellState) -> Result<ConnectParams, Invalid> {
        if !self.current(shell) {
            return Err(Invalid::Source);
        }
        self.params.take().ok_or(Invalid::Source)
    }

    pub fn identity(&self) -> &WorkerId {
        &self.worker
    }

    /// Check BEFORE changing core rt_generation for any incoming update. The
    /// existing epoch/Transport worker guard alone cannot reject replacement
    /// workers that share credentials, account and a numeric generation.
    pub fn current(&self, shell: &ShellState) -> bool {
        self.worker.live()
            && self.binding.current(shell)
            && shell
                .draft_writes
                .socket_sources
                .teams
                .get(&self.team)
                .is_some_and(|slot| slot.worker.same(&self.worker))
    }

    /// REQUIRED before the Connected arm changes core.rt_generation. Keep this
    /// check, the assignment, and accept in one serial shell write with no await.
    /// current() alone does not reject this worker's stale/duplicate Connected.
    pub fn can_accept(&self, shell: &ShellState, team: &str, generation: u64) -> bool {
        self.params.is_none()
            && team == self.team
            && generation > 0
            && generation > self.last_generation
            && self.current(shell)
    }

    /// Only after can_accept succeeds, assign core generation and call this in
    /// the SAME shell write. Rechecks preflight plus assignment; it cannot undo
    /// a caller's premature core mutation. This does not open a durable lease.
    pub fn accept(
        &mut self,
        shell: &mut ShellState,
        team: &str,
        generation: u64,
    ) -> Result<Source, Invalid> {
        if !self.can_accept(shell, team, generation)
            || shell
                .core
                .workspaces
                .get(team)
                .is_none_or(|workspace| workspace.rt_generation != generation)
        {
            return Err(Invalid::Source);
        }
        self.last_generation = generation;
        shell
            .draft_writes
            .socket_sources
            .teams
            .get_mut(team)
            .ok_or(Invalid::Source)?
            .generation = Some(generation);
        Ok(Source {
            binding: self.binding.clone(),
            account: self.account.clone(),
            team: self.team.clone(),
            user: self.user.clone(),
            worker: self.worker.clone(),
            generation,
        })
    }

    /// Disconnect may name a generation that failed before Connected. Retire
    /// any accepted older generation, without letting a stale disconnect retire
    /// the new one. A replacement worker can never retire its successor.
    pub fn disconnected(&mut self, shell: &mut ShellState, team: &str, generation: u64) {
        if team != self.team || !self.current(shell) || generation < self.last_generation {
            return;
        }
        self.last_generation = generation;
        if let Some(slot) = shell.draft_writes.socket_sources.teams.get_mut(team) {
            slot.generation = None;
        }
    }

    /// Also call on receiver closure/cancellation/account replacement. The
    /// durable ingress owner separately persists its gap before further work.
    pub fn retire(&self, shell: &mut ShellState) {
        if shell
            .draft_writes
            .socket_sources
            .teams
            .get(&self.team)
            .is_some_and(|slot| slot.worker.same(&self.worker))
        {
            shell.draft_writes.socket_sources.teams.remove(&self.team);
        }
    }
}

impl Source {
    pub(crate) fn matches_owner(&self, account: &str, team: &str) -> bool {
        self.account == account && self.team == team
    }
    pub fn identity(&self) -> &WorkerId {
        &self.worker
    }
    pub fn current(&self, shell: &ShellState, frame_generation: u64) -> bool {
        self.worker.live()
            && frame_generation == self.generation
            && self.binding.current(shell)
            && shell
                .draft_writes
                .socket_sources
                .teams
                .get(&self.team)
                .is_some_and(|slot| {
                    slot.worker.same(&self.worker) && slot.generation == Some(self.generation)
                })
            && shell
                .core
                .workspaces
                .get(&self.team)
                .is_some_and(|workspace| {
                    workspace.rt_generation == self.generation
                        && workspace.self_user_id == self.user
                })
    }

    /// Pass the original RtUpdate::Event generation, never the current shell's
    /// number. Durable admission and owner/enrollment validation remain required.
    pub fn prepare(
        &self,
        shell: &ShellState,
        frame_generation: u64,
        key: &DraftKey,
        raw: &RawEvent,
    ) -> Result<Prepared, Invalid> {
        if key.team != self.team || !self.current(shell, frame_generation) {
            return Err(Invalid::Source);
        }
        decode(
            &Scope {
                account_id: self.account.clone(),
                key: key.clone(),
            },
            &self.user,
            raw,
        )
    }
}

#[cfg(test)]
mod tests;
