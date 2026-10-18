//! Original receipt publication into the shared ordered atomic queue.
use super::*;
use std::collections::BTreeSet;

pub(super) fn rows(
    mut state: Signal<ShellState>,
    attempt: &Attempt,
    ingress: &crate::bootstrap::posting_ingress::Ingress,
    rows: &[Value],
    membership: Option<&[String]>,
) -> Result<(), Error> {
    let typed = rows
        .iter()
        .cloned()
        .map(serde_json::from_value)
        .collect::<Result<Vec<super_platinum_core::slack::models::Channel>, _>>()
        .map_err(|_| unavailable())?;
    rows_in(&mut state.write(), attempt, rows, membership)?;
    ingress.channels(&state, &typed);
    Ok(())
}
pub(super) fn rows_in(
    shell: &mut ShellState,
    attempt: &Attempt,
    rows: &[Value],
    membership: Option<&[String]>,
) -> Result<(), Error> {
    let observed = observed(shell, attempt)?;
    let ctx = &attempt.owner.context;
    if rows.iter().any(|r| {
        r["id"]
            .as_str()
            .is_none_or(|id| !attempt.batch.ids().iter().any(|wanted| wanted == id))
    }) {
        return Err(unavailable());
    }
    let returned: BTreeSet<_> = rows.iter().filter_map(|r| r["id"].as_str()).collect();
    let joined: Vec<_> = membership
        .unwrap_or_default()
        .iter()
        .filter(|id| !returned.contains(id.as_str()))
        .cloned()
        .collect();
    let receipt = members::receipt(shell, &attempt.owner, rows)?;
    let mut shadow = attempt.shadow.borrow().clone();
    shadow
        .membership_step(&[], &joined, true)
        .map_err(|_| unavailable())?;
    let ids = rows
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    // 340b marks each raw processor input fresh against its pre-dispatch row.
    let incoming = fresh(&shadow, &ids)?;
    let runtime = &shell.app_home.channel_fetches.conditional.slots[&ctx.key()];
    let w = &shell.core.workspaces[&ctx.session.team_id];
    let (mut store, native) = runtime
        .manager
        .queue_joined_memberships(
            &runtime.scope,
            &attempt.batch,
            &w.directory_channels,
            &observed,
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
            &w.channels,
            &joined,
        )
        .map_err(|_| unavailable())?
        .into_parts();
    let accepted = w
        .directory_channels
        .capture_receipt_ingress(
            &observed,
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
            &w.channels,
            Origin::Direct,
            rows.to_vec(),
            receipt.facts.clone(),
        )
        .map_err(|_| unavailable())?;
    shadow
        .step(rows, &receipt.facts, accepted.boot_in_flight())
        .map_err(|_| unavailable())?;
    store
        .enqueue_ingress(accepted, &ctx.account, &ctx.session, ctx.epoch)
        .map_err(|_| unavailable())?;
    let observation = facts(shell, ctx)?;
    let now = now(shell)?;
    let runtime = shell
        .app_home
        .channel_fetches
        .conditional
        .slots
        .get_mut(&ctx.key())
        .unwrap();
    runtime
        .manager
        .mark_fresh(&runtime.scope, now, observation, &incoming)
        .map_err(|_| unavailable())?;
    let w = shell.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
    w.directory_channels = store;
    w.channels = native;
    *attempt.shadow.borrow_mut() = shadow;
    publication_advance(shell, attempt)
}
pub(super) fn unchanged(
    mut state: Signal<ShellState>,
    attempt: &Attempt,
    resolved: &[String],
    returned: &[String],
) -> Result<Vec<String>, Error> {
    let mut shell = state.write();
    let observed = observed(&shell, attempt)?;
    let ctx = &attempt.owner.context;
    if resolved
        .iter()
        .chain(returned)
        .any(|id| !attempt.batch.ids().contains(id))
    {
        return Err(unavailable());
    }
    let mut missing = Vec::new();
    let mut present = Vec::new();
    for id in resolved.iter().filter(|id| !returned.contains(id)) {
        match attempt
            .shadow
            .borrow()
            .row(id)
            .map_err(|_| unavailable())?
            .kind
        {
            ClassificationKind::Absent => missing.push(id.clone()),
            _ => present.push(id.clone()),
        }
    }
    let runtime = &shell.app_home.channel_fetches.conditional.slots[&ctx.key()];
    let w = &shell.core.workspaces[&ctx.session.team_id];
    let (store, native) = runtime
        .manager
        .queue_unchanged_channels(
            &runtime.scope,
            &attempt.batch,
            &w.directory_channels,
            &observed,
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
            &w.channels,
            &present,
        )
        .map_err(|_| unavailable())?
        .into_parts();
    let w = shell.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
    w.directory_channels = store;
    w.channels = native;
    publication_advance(&mut shell, attempt)?;
    // Missing IDs require the distinct unknown acquisition manager; never mint absence.
    Ok(missing)
}
