//! Never enqueue a shared receipt in canonical state before renderer completion.
use super::*;
use super_platinum_core::{
    block_actions::{
        directory_endpoint::member_info::unknown::PlaceholderAsset,
        workflow_management::member_hydration::{Flags, RendererReply},
    },
    slack::models::User,
    state::directory_members::{Origin, Pending, Store},
};
pub(crate) struct Stage {
    store: Store,
    native: HashMap<String, User>,
    flags: Flags,
    generation: String,
    outcome: Arc<Outcome>,
    prepared: usize,
}
impl Election {
    pub(crate) fn stage(
        &self,
        shell: &ShellState,
        flags: Flags,
        generation: String,
        asset: &PlaceholderAsset,
    ) -> Result<Stage, Error> {
        let adoption = self.adoption.as_ref().ok_or_else(unavailable)?;
        if !self.current(shell) || generation.is_empty() {
            return Err(unavailable());
        }
        let rows = self.outcome.direct_rows(shell, adoption, asset)?;
        let ctx = &self.outcome.dispatch.context;
        let w = shell
            .core
            .workspaces
            .get(&ctx.session.team_id)
            .ok_or_else(unavailable)?;
        let mut store = w.directory_members.clone();
        store.enqueue(
            &ctx.account,
            &ctx.session,
            Origin::DirectUpsert,
            rows,
            serde_json::json!({}),
        )?;
        Ok(Stage {
            store,
            native: w.users.clone(),
            flags,
            generation,
            outcome: self.outcome.clone(),
            prepared: 0,
        })
    }
}
impl Stage {
    fn current(&self, shell: &ShellState, election: &Election) -> bool {
        Arc::ptr_eq(&self.outcome, &election.outcome) && election.current(shell)
    }
    pub(crate) fn prepare(
        &mut self,
        shell: &ShellState,
        election: &Election,
    ) -> Result<Option<Pending>, Error> {
        if !self.current(shell, election) {
            return Err(unavailable());
        }
        let ctx = &self.outcome.dispatch.context;
        if !self.store.has_pending(&ctx.account, &ctx.session)? {
            return Ok(None);
        }
        if self.prepared >= 64 {
            return Err(unavailable());
        }
        let pending = self.store.prepare(
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            &self.generation,
            self.flags.clone(),
            &self.native,
        )?;
        self.prepared += 1;
        Ok(Some(pending))
    }
    pub(crate) fn apply(
        &mut self,
        shell: &ShellState,
        election: &Election,
        pending: Pending,
        reply: RendererReply,
    ) -> Result<(), Error> {
        if !self.current(shell, election) {
            return Err(unavailable());
        }
        let ctx = &self.outcome.dispatch.context;
        let mut store = self.store.clone();
        let commit = store.adopt(
            pending,
            reply,
            &self.flags,
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            &self.generation,
            &self.native,
        )?;
        let (_, native) = commit.into_parts();
        self.store = store;
        self.native = native;
        Ok(())
    }
    /// Consumer checks Query/renderer ownership in this same serial write.
    /// Manager settlement is checked against the private completed pair before
    /// either canonical field changes; installation afterward is infallible.
    pub(crate) fn install(
        self,
        shell: &mut ShellState,
        election: &mut Election,
        consumer_current: impl FnOnce(&ShellState) -> bool,
    ) -> Result<(), Error> {
        if !consumer_current(shell) || !self.current(shell, election) {
            return Err(unavailable());
        }
        let ctx = &self.outcome.dispatch.context;
        if self.store.has_pending(&ctx.account, &ctx.session)? {
            return Err(unavailable());
        }
        if !shell.core.workspaces.contains_key(&ctx.session.team_id) {
            return Err(unavailable());
        }
        let adoption = election.adoption.as_ref().ok_or_else(unavailable)?;
        let slot = shell.directory_member_fetches.slot_mut(&adoption.context)?;
        slot.manager.settle(
            &adoption.token,
            &self.store,
            &ctx.account,
            &ctx.session,
            &self.native,
        )?;
        let w = shell
            .core
            .workspaces
            .get_mut(&ctx.session.team_id)
            .expect("checked workspace");
        w.directory_members = self.store;
        w.users = self.native;
        election.adoption = None;
        slot.notify();
        Ok(())
    }
}
