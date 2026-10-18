//! A fresh, separately owned native reconciliation after ordinary ScqY intake.
//! This never retries a mutation or changes the retained source projection.
use super::*;
use std::collections::BTreeSet;
use super_platinum_core::state::directory_members::NativeRepair;

pub(in crate::app_home::about::management::activity::members) async fn repair_known(
    mut state: Signal<ShellState>,
    q: &Query,
) -> Result<(), Error> {
    let recovery = {
        let s = state.peek();
        let Some(mut recovery) = Recovery::capture(&s, q)? else {
            return Ok(());
        };
        let known: BTreeSet<_> = recovery.prior.source_ids().map(str::to_owned).collect();
        recovery.requested.retain(|id| known.contains(id));
        if recovery.requested.is_empty() {
            return Ok(());
        }
        recovery
    };
    let received = Arc::new(recovery::acquire(state, q, recovery.requested.clone()).await?);
    q.recovery_receipts.lock().unwrap().push(received.clone());
    // A new experiment read cannot authenticate a changed native/source input:
    // the original request witness must still match after both awaits.
    let raw = read::request(
        state,
        q,
        super_platinum_core::slack::api::list_experiments(
            &q.owner.ctx.client,
            &q.owner.ctx.session,
        ),
    )
    .await?;
    let flags = Flags::decode(&raw)?;
    let plan = prepare(&state.peek(), q, &recovery, &received, flags.clone())?;
    let reply = render::hydrate_fields(state, q, plan.generation(), plan.fields()).await?;
    install_repair(&mut state.write(), q, &recovery, &flags, plan, reply)?;
    crate::bootstrap::persist_workspace_metadata(&state, &q.owner.ctx.session.team_id);
    Ok(())
}

fn prepare(
    s: &ShellState,
    q: &Query,
    recovery: &Recovery,
    received: &recovery::Acquisition,
    flags: Flags,
) -> Result<NativeRepair, Error> {
    if !recovery.current(s, q) || recovery.requested != received.requested {
        return Err(unavailable());
    }
    let rows =
        super_platinum_core::block_actions::directory_endpoint::member_info::project_interaction(
            &received.rows,
            &received.interactions,
            recovery.prior.interactions(),
        )?;
    // Only complete fresh coverage can repair this request. An invalid/pending
    // or failed chunk is retained by Acquisition, never converted to a witness.
    let actual: BTreeSet<_> = rows.iter().filter_map(|r| r.get("id")?.as_str()).collect();
    let expected: BTreeSet<_> = recovery.requested.iter().map(String::as_str).collect();
    if !received.failed.is_empty() || actual != expected || rows.len() != expected.len() {
        return Err(unavailable());
    }
    let w = s
        .core
        .workspaces
        .get(&q.owner.ctx.session.team_id)
        .ok_or_else(unavailable)?;
    w.directory_members.prepare_native_repair(
        &recovery.account,
        &q.owner.ctx.session,
        q.owner.ctx.epoch,
        &q.generation(),
        flags,
        rows,
        &w.users,
    )
}

pub(super) fn install_repair(
    s: &mut ShellState,
    q: &Query,
    recovery: &Recovery,
    flags: &Flags,
    plan: NativeRepair,
    reply: RendererReply,
) -> Result<(), Error> {
    if !recovery.current(s, q) {
        return Err(unavailable());
    }
    let w = s
        .core
        .workspaces
        .get_mut(&q.owner.ctx.session.team_id)
        .ok_or_else(unavailable)?;
    let mut staged = w.directory_members.clone();
    let commit = staged.adopt_native_repair(
        plan,
        reply,
        flags,
        &recovery.account,
        &q.owner.ctx.session,
        q.owner.ctx.epoch,
        &q.generation(),
        &w.users,
    )?;
    w.users = commit.into_native();
    w.directory_members = staged;
    Ok(())
}
