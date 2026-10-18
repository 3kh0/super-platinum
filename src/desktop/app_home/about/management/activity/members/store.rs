//! Activity bridge to the one shared raw-order store. No native User is used
//! to manufacture source rows. A missing legacy observation requires a read.
use super::*;
use super_platinum_core::{
    block_actions::workflow_management::member_hydration::{Flags, RendererReply},
    state::directory_members::{Observations, Pending, Ready, RecoveryState},
};
mod repair;
mod unknown;
pub(super) use repair::repair_known;
pub(super) use unknown::recover_absent;

pub(super) struct Snapshot {
    account: String,
    ready: Ready,
}
impl Snapshot {
    pub(super) fn rows(
        &self,
    ) -> &[super_platinum_core::block_actions::workflow_management::member_hydration::Current] {
        self.ready.rows()
    }
    pub(super) fn current(&self, s: &ShellState, q: &Query) -> bool {
        q.current(s)
            && s.core.active_account.as_deref() == Some(&self.account)
            && s.core
                .workspaces
                .get(&q.owner.ctx.session.team_id)
                .is_some_and(|w| {
                    self.ready.current(
                        &w.directory_members,
                        &self.account,
                        &q.owner.ctx.session,
                        q.owner.ctx.epoch,
                        &q.generation(),
                        &w.users,
                    )
                })
    }
}

pub(super) fn snapshot(s: &ShellState, q: &Query) -> Result<Snapshot, Error> {
    let account = account(s, q)?;
    let w = s
        .core
        .workspaces
        .get(&q.owner.ctx.session.team_id)
        .ok_or_else(unavailable)?;
    Ok(Snapshot {
        ready: w.directory_members.ready(
            &account,
            &q.owner.ctx.session,
            q.owner.ctx.epoch,
            &q.generation(),
            &w.users,
        )?,
        account,
    })
}

/// Explicit native acquisition-history initialization. A retained different
/// identity is an error, never permission to rebind another account's store.
pub(super) fn initialize(s: &mut ShellState, q: &Query) -> Result<bool, Error> {
    let account = account(s, q)?;
    let w = s
        .core
        .workspaces
        .get_mut(&q.owner.ctx.session.team_id)
        .ok_or_else(unavailable)?;
    match w
        .directory_members
        .recovery_state(&account, &q.owner.ctx.session)?
    {
        RecoveryState::CurrentIdentity => Ok(false),
        RecoveryState::Unknown | RecoveryState::Overflowed => {
            let ids = w.users.keys().cloned().collect();
            let mut staged = w.directory_members.clone();
            staged.begin(&account, &q.owner.ctx.session, &ids)?;
            w.directory_members = staged;
            Ok(true)
        }
    }
}

pub(super) struct Recovery {
    account: String,
    prior: Observations,
    // Comparison only. This never supplies source rows or their encounter order.
    native: Value,
    requested: Vec<String>,
}
impl Recovery {
    pub(super) fn capture(s: &ShellState, q: &Query) -> Result<Option<Self>, Error> {
        let account = account(s, q)?;
        let w = s
            .core
            .workspaces
            .get(&q.owner.ctx.session.team_id)
            .ok_or_else(unavailable)?;
        let prior = w.directory_members.observations(
            &account,
            &q.owner.ctx.session,
            q.owner.ctx.epoch,
            &q.generation(),
        )?;
        // This set chooses read IDs only. Actual wire row order establishes the
        // new explicitly labelled native acquisition history.
        let requested: Vec<_> = w
            .directory_members
            .required_observations(&w.users)
            .into_iter()
            .collect();
        if requested.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            account,
            prior,
            native: serde_json::to_value(&w.users).map_err(|_| unavailable())?,
            requested,
        }))
    }
    pub(super) fn requested(&self) -> &[String] {
        &self.requested
    }
    fn current(&self, s: &ShellState, q: &Query) -> bool {
        account(s, q).is_ok_and(|account| account == self.account)
            && s.core
                .workspaces
                .get(&q.owner.ctx.session.team_id)
                .is_some_and(|w| {
                    self.prior.current(
                        &w.directory_members,
                        &self.account,
                        &q.owner.ctx.session,
                        q.owner.ctx.epoch,
                        &q.generation(),
                    ) && serde_json::to_value(&w.users).is_ok_and(|v| v == self.native)
                })
    }
    pub(super) fn enqueue(
        &self,
        s: &mut ShellState,
        q: &Query,
        received: &recovery::Acquisition,
    ) -> Result<(), Error> {
        if !self.current(s, q) || self.requested != received.requested {
            return Err(unavailable());
        }
        let rows = super_platinum_core::block_actions::directory_endpoint::member_info::project_interaction(
            &received.rows, &received.interactions, self.prior.interactions())?;
        let interactions =
            serde_json::to_value(&received.interactions).map_err(|_| unavailable())?;
        let w = s
            .core
            .workspaces
            .get_mut(&q.owner.ctx.session.team_id)
            .ok_or_else(unavailable)?;
        // Requested resets and returned disconnected marks share the exact
        // hydration Commit. Missing/invalid IDs do not delete source members.
        w.directory_members.enqueue_member_info(
            &self.account,
            &q.owner.ctx.session,
            rows,
            interactions,
            received.requested.clone(),
            received.disconnected.clone(),
        )
    }
}

/// Actual search results are supplied before An8m/ScqY processing. The shared
/// Plan performs those transforms once; typed decoder output is never queued.
pub(super) fn enqueue_search(
    s: &mut ShellState,
    q: &Query,
    pages: &super_platinum_core::block_actions::workflow_management::activity_members::Pages,
) -> Result<std::collections::BTreeSet<String>, Error> {
    let account = account(s, q)?;
    let rows = pages.rows()?;
    let mut interactions = serde_json::Map::new();
    let mut ids = std::collections::BTreeSet::new();
    for raw in rows {
        let row = super_platinum_core::block_actions::workflow_management::activity_members::Member::decode(raw)?;
        ids.insert(row.user().id.clone());
        // Actual ScqY caller supplies this distinct remote availability map.
        // Explicit is_stranger is applied later by the owned hydration Plan.
        interactions.insert(row.user().id.clone(), Value::Bool(true));
    }
    let w = s
        .core
        .workspaces
        .get_mut(&q.owner.ctx.session.team_id)
        .ok_or_else(unavailable)?;
    w.directory_members.enqueue(
        &account,
        &q.owner.ctx.session,
        super_platinum_core::state::directory_members::Origin::Directory,
        rows.to_vec(),
        Value::Object(interactions),
    )?;
    Ok(ids)
}

/// Consume only already observed raw batches. This never resets a legacy
/// store, reconstructs order, or treats valid empty queue as a Ready receipt.
pub(super) async fn drain(mut state: Signal<ShellState>, q: &Query) -> Result<(), Error> {
    let account = account(&state.peek(), q)?;
    if !pending(&state.peek(), q, &account)? {
        return Ok(());
    }
    let ctx = &q.owner.ctx;
    let raw = read::request(
        state,
        q,
        super_platinum_core::slack::api::list_experiments(&ctx.client, &ctx.session),
    )
    .await?;
    let flags = Flags::decode(&raw)?;
    // At most the store's complete bounded queue per turn. Concurrent arrivals
    // can require another owned turn; no always-on retry or silent truncation.
    for _ in 0..64 {
        let plan = {
            let s = state.peek();
            if !pending(&s, q, &account)? {
                return Ok(());
            }
            let w = s
                .core
                .workspaces
                .get(&ctx.session.team_id)
                .ok_or_else(unavailable)?;
            w.directory_members.prepare(
                &account,
                &ctx.session,
                ctx.epoch,
                &q.generation(),
                flags.clone(),
                &w.users,
            )?
        };
        let reply = render::hydrate(state, q, &plan).await?;
        install(&mut state.write(), q, &account, &flags, plan, reply)?;
        // Existing persistence worker is asynchronous; no disk work is added
        // to the serial state mutation or the renderer callback.
        crate::bootstrap::persist_workspace_metadata(&state, &ctx.session.team_id);
    }
    if pending(&state.peek(), q, &account)? {
        Err(unavailable())
    } else {
        Ok(())
    }
}

fn account(s: &ShellState, q: &Query) -> Result<String, Error> {
    if !q.current(s) {
        return Err(unavailable());
    }
    s.core.active_account.clone().ok_or_else(unavailable)
}
fn pending(s: &ShellState, q: &Query, expected: &str) -> Result<bool, Error> {
    if account(s, q)? != expected {
        return Err(unavailable());
    }
    s.core
        .workspaces
        .get(&q.owner.ctx.session.team_id)
        .ok_or_else(unavailable)?
        .directory_members
        .has_pending(expected, &q.owner.ctx.session)
}

fn install(
    s: &mut ShellState,
    q: &Query,
    expected: &str,
    flags: &Flags,
    plan: Pending,
    reply: RendererReply,
) -> Result<(), Error> {
    if account(s, q)? != expected {
        return Err(unavailable());
    }
    let ctx = &q.owner.ctx;
    let w = s
        .core
        .workspaces
        .get_mut(&ctx.session.team_id)
        .ok_or_else(unavailable)?;
    let mut staged = w.directory_members.clone();
    let commit = staged.adopt(
        plan,
        reply,
        flags,
        expected,
        &ctx.session,
        ctx.epoch,
        &q.generation(),
        &w.users,
    )?;
    let (_, native) = commit.into_parts();
    // Both fields come from the same checked Commit. There is no await or
    // separately supplied native_after map between this pair of assignments.
    w.users = native;
    w.directory_members = staged;
    Ok(())
}

#[cfg(test)]
mod tests;
