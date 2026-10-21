//! Query-owned users/search intake into the existing ordered member store.
//! No network, selector eligibility, or replacement source history lives here.
use super::{Context, context};
use crate::{
    app_home::channel_fetches::{self, MemberFlags},
    state::ShellState,
};
use dioxus::prelude::*;
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use super_platinum_core::{
    block_actions::workflow_management::member_hydration::{Current, RendererReply},
    slack::{Error, api::custom::texty::members::Pages, models::User},
    state::directory_members::{Observations, Origin, Ready, Store},
};
#[cfg(test)]
mod tests;

fn unavailable() -> Error {
    Error::Api("directory_member_search_unavailable".into())
}

struct Witness {
    context: Arc<Context>,
    generation: String,
    observed: Observations,
    native: BTreeMap<String, Value>,
}
impl Witness {
    fn capture(shell: &ShellState, team: &str, generation: &str) -> Result<Self, Error> {
        let context = Arc::new(Context::capture(shell, team)?);
        let w = shell.core.workspaces.get(team).ok_or_else(unavailable)?;
        Ok(Self {
            observed: w.directory_members.observations(
                &context.account,
                &context.session,
                context.epoch,
                generation,
            )?,
            native: context::native_snapshot(&w.users)?,
            context,
            generation: generation.into(),
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        let ctx = &self.context;
        ctx.current(shell)
            && shell
                .core
                .workspaces
                .get(&ctx.session.team_id)
                .is_some_and(|w| {
                    self.observed.current(
                        &w.directory_members,
                        &ctx.account,
                        &ctx.session,
                        ctx.epoch,
                        &self.generation,
                    ) && context::native_snapshot(&w.users)
                        .is_ok_and(|native| native == self.native)
                })
    }
}

/// Capture once before the first page request, retaining it through all pages.
/// Missing/pending source history refuses; capture never initializes/reset it.
pub(crate) struct Read {
    witness: Witness,
    flags: Arc<MemberFlags>,
}
impl Read {
    pub(crate) fn capture(shell: &ShellState, team: &str, generation: &str) -> Result<Self, Error> {
        let witness = Witness::capture(shell, team, generation)?;
        let flags = Arc::new(channel_fetches::member_flags(shell, team)?);
        Ok(Self { witness, flags })
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.witness.current(shell) && self.flags.current(shell)
    }
    pub(crate) async fn publish(
        &self,
        state: Signal<ShellState>,
        pages: &Pages,
        current: impl Fn(&ShellState) -> bool,
    ) -> Result<Snapshot, Error> {
        let rows = pages.rows()?;
        let interactions = search_interactions(rows)?;
        self.publish_rows(state, rows, &interactions, current).await
    }
    /// Original caller-supplied rows and interaction map, captured under this
    /// same read before acquisition. No availability defaults or source reset.
    pub(crate) async fn publish_rows(
        &self,
        mut state: Signal<ShellState>,
        rows: &[Value],
        interactions: &Value,
        current: impl Fn(&ShellState) -> bool,
    ) -> Result<Snapshot, Error> {
        let admitted = |shell: &ShellState| self.current(shell) && current(shell);
        let mut stage = {
            let shell = state.peek();
            if !admitted(&shell) {
                return Err(unavailable());
            }
            Stage::capture_rows(&shell, &self.witness, rows, interactions)?
        };
        let ctx = &self.witness.context;
        let generation = &self.witness.generation;
        let flags = self.flags.flags();
        // Observations admits no previous pending batches, so this is exactly
        // one Directory intake, including a successful empty result.
        let pending = stage.store.prepare(
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            generation,
            flags.clone(),
            &stage.native,
        )?;
        let lowered = channel_fetches::member_fields(
            state,
            &admitted,
            pending.generation(),
            pending.fields(),
        )
        .await?;
        if !admitted(&state.peek()) {
            return Err(unavailable());
        }
        let reply = RendererReply {
            generation: pending.generation().into(),
            operation: "ordinary-toLowerCase".into(),
            fields: pending.fields().to_vec(),
            lowered,
        };
        stage.native = stage
            .store
            .adopt(
                pending,
                reply,
                flags,
                &ctx.account,
                &ctx.session,
                ctx.epoch,
                generation,
                &stage.native,
            )?
            .into_parts()
            .1;
        // Check before a Signal write as well as inside the atomic sink.
        if !admitted(&state.peek()) {
            return Err(unavailable());
        }
        let ready = stage.install(&mut state.write(), &self.witness, &admitted)?;
        Ok(Snapshot {
            context: ctx.clone(),
            generation: generation.clone(),
            ready,
            flags: self.flags.clone(),
        })
    }
}

pub(crate) struct Snapshot {
    context: Arc<Context>,
    generation: String,
    ready: Ready,
    flags: Arc<MemberFlags>,
}
impl Snapshot {
    pub(crate) fn rows(&self) -> &[Current] {
        self.ready.rows()
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        let ctx = &self.context;
        ctx.current(shell)
            && self.flags.current(shell)
            && shell
                .core
                .workspaces
                .get(&ctx.session.team_id)
                .is_some_and(|w| {
                    self.ready.current(
                        &w.directory_members,
                        &ctx.account,
                        &ctx.session,
                        ctx.epoch,
                        &self.generation,
                        &w.users,
                    )
                })
    }
}
/// Full local accepted collection, not a claim of server-wide completeness.
/// Caller separately checks its original query/lease when using this snapshot.
pub(crate) fn snapshot(
    shell: &ShellState,
    team: &str,
    generation: &str,
) -> Result<Snapshot, Error> {
    let read = Read::capture(shell, team, generation)?;
    let ctx = &read.witness.context;
    let w = shell.core.workspaces.get(team).ok_or_else(unavailable)?;
    let ready =
        w.directory_members
            .ready(&ctx.account, &ctx.session, ctx.epoch, generation, &w.users)?;
    Ok(Snapshot {
        context: ctx.clone(),
        generation: generation.into(),
        ready,
        flags: read.flags,
    })
}

struct Stage {
    store: Store,
    native: HashMap<String, User>,
}
impl Stage {
    #[cfg(test)]
    fn capture(shell: &ShellState, witness: &Witness, rows: &[Value]) -> Result<Self, Error> {
        Self::capture_rows(shell, witness, rows, &search_interactions(rows)?)
    }
    fn capture_rows(
        shell: &ShellState,
        witness: &Witness,
        rows: &[Value],
        interactions: &Value,
    ) -> Result<Self, Error> {
        if !witness.current(shell) {
            return Err(unavailable());
        }
        let ctx = &witness.context;
        let w = shell
            .core
            .workspaces
            .get(&ctx.session.team_id)
            .ok_or_else(unavailable)?;
        let mut store = w.directory_members.clone();
        store.enqueue(
            &ctx.account,
            &ctx.session,
            Origin::Directory,
            rows.to_vec(),
            interactions.clone(),
        )?;
        Ok(Self {
            store,
            native: w.users.clone(),
        })
    }
    fn install(
        self,
        shell: &mut ShellState,
        witness: &Witness,
        current: &impl Fn(&ShellState) -> bool,
    ) -> Result<Ready, Error> {
        if !witness.current(shell) || !current(shell) {
            return Err(unavailable());
        }
        let ctx = &witness.context;
        // Missing old observations, invalidated rows or unchanged-version
        // native drift cannot be relabelled a full inventory by a search page.
        let ready = self.store.ready(
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            &witness.generation,
            &self.native,
        )?;
        let w = shell
            .core
            .workspaces
            .get_mut(&ctx.session.team_id)
            .ok_or_else(unavailable)?;
        w.directory_members = self.store;
        w.users = self.native;
        Ok(ready)
    }
}

/// Actual users/search availability map supplied to ScqY. Explicit raw
/// is_stranger still overrides this in the unchanged hydration plan.
fn search_interactions(rows: &[Value]) -> Result<Value, Error> {
    let mut interactions = Map::new();
    for row in rows {
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(unavailable)?;
        interactions.insert(id.into(), Value::Bool(true));
    }
    Ok(Value::Object(interactions))
}
