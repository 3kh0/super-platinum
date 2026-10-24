//! Exact owned queue successors. No marker can enter through a raw response.
use super::*;
use std::collections::BTreeSet;
use super_platinum_core::state::directory_channels::projection::ClassificationKind;

pub(super) fn rows(
    mut state: Signal<ShellState>,
    a: &Attempt,
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
    rows_in(&mut state.write(), a, rows, membership)?;
    ingress.channels(&state, &typed);
    Ok(())
}
pub(super) fn rows_in(
    shell: &mut ShellState,
    a: &Attempt,
    rows: &[Value],
    membership: Option<&[String]>,
) -> Result<(), Error> {
    rows_with_freshness(shell, a, rows, membership, conditional::source_fresh)
}
// The callback is the existing secondary freshness bookkeeping, separate from
// source publication. Tests supply its failure without forging an ingress.
pub(super) fn rows_with_freshness(
    shell: &mut ShellState,
    a: &Attempt,
    rows: &[Value],
    membership: Option<&[String]>,
    freshness: impl FnOnce(
        &mut ShellState,
        &Owner,
        Option<conditional::continuation::Input>,
        &Facts,
        bool,
    ) -> Result<Option<conditional::continuation::Continuation>, Error>,
) -> Result<(), Error> {
    let observed = observed_attempt(shell, a)?;
    let ctx = &a.owner.context;
    if rows.iter().any(|r| {
        r["id"]
            .as_str()
            .is_none_or(|id| !a.flight.ids().iter().any(|v| v == id))
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
    let receipt = members::receipt(shell, &a.owner, rows)?;
    let fresh = conditional::source_values(shell, &a.owner, rows)?;
    let mut shadow = a.shadow.borrow().clone();
    shadow
        .membership_step(&[], &joined, true)
        .map_err(|_| unavailable())?;
    let r = &shell.app_home.channel_fetches.unknown.slots[&ctx.key()];
    let w = &shell.core.workspaces[&ctx.session.team_id];
    let (mut store, native) = r
        .manager
        .queue_joined_memberships(
            &r.scope,
            &a.flight,
            &w.directory_channels,
            &observed,
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
    let boot = accepted.boot_in_flight();
    shadow
        .step(rows, &receipt.facts, boot)
        .map_err(|_| unavailable())?;
    store
        .enqueue_ingress(accepted, &ctx.account, &ctx.session, ctx.epoch)
        .map_err(|_| unavailable())?;
    let w = shell.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
    w.directory_channels = store;
    w.channels = native;
    *a.shadow.borrow_mut() = shadow;
    // These objects were accepted by the exact queue above. Preserve them
    // across timeout retries even if secondary freshness bookkeeping fails.
    a.accepted_ids
        .borrow_mut()
        .extend(returned.into_iter().map(str::to_owned));
    let next = match freshness(shell, &a.owner, fresh, &receipt.facts, boot) {
        Ok(next) => next,
        Err(error) => {
            failed(shell, &a.owner, error);
            None
        }
    };
    conditional::continuation::advance(shell, &a.owner, next)?;
    *a.publication.borrow_mut() = shell.app_home.channel_fetches.slots[&ctx.key()]
        .publication
        .clone();
    Ok(())
}
pub(super) fn unchanged(
    mut state: Signal<ShellState>,
    a: &Attempt,
    resolved: &[String],
    returned: &[String],
) -> Result<(), Error> {
    let mut shell = state.write();
    let observed = observed_attempt(&shell, a)?;
    if resolved
        .iter()
        .chain(returned)
        .any(|id| !a.flight.ids().contains(id))
    {
        return Err(unavailable());
    }
    let mut present = Vec::new();
    for id in resolved.iter().filter(|id| !returned.contains(id)) {
        if a.shadow.borrow().row(id).map_err(|_| unavailable())?.kind != ClassificationKind::Absent
        {
            present.push(id.clone());
        }
    }
    let ctx = &a.owner.context;
    let r = &shell.app_home.channel_fetches.unknown.slots[&ctx.key()];
    let w = &shell.core.workspaces[&ctx.session.team_id];
    let (store, native) = r
        .manager
        .queue_unchanged_channels(
            &r.scope,
            &a.flight,
            &w.directory_channels,
            &observed,
            GENERATION,
            &w.channels,
            &present,
        )
        .map_err(|_| unavailable())?
        .into_parts();
    let w = shell.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
    w.directory_channels = store;
    w.channels = native;
    advance_owned(&mut shell, a)
}
fn advance_owned(shell: &mut ShellState, a: &Attempt) -> Result<(), Error> {
    conditional::continuation::clear(shell, &a.owner);
    super::super::advance(shell, &a.owner)?;
    *a.publication.borrow_mut() = shell.app_home.channel_fetches.slots[&a.owner.context.key()]
        .publication
        .clone();
    Ok(())
}
pub(super) fn finish(shell: &mut ShellState, a: &Attempt) -> Result<(), Error> {
    if a.refused_dispatch.get() {
        return Err(unavailable());
    }
    let observed = observed_attempt(shell, a)?;
    let ctx = &a.owner.context;
    // Only wrapper-retained rows and actual accepted returned objects count.
    // Joined-only placeholders remain unresolved even though the shadow has a row.
    let retained = a.accepted_ids.borrow().iter().cloned().collect::<Vec<_>>();
    let r = &shell.app_home.channel_fetches.unknown.slots[&ctx.key()];
    let settlement = r
        .manager
        .settlement(&r.scope, &a.flight, &retained)
        .map_err(|_| unavailable())?;
    let scope = r.scope.clone();
    let w = shell.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
    w.directory_channels
        .enqueue_runtime_missing(
            &observed,
            &scope,
            GENERATION,
            &w.channels,
            settlement.missing(),
        )
        .map_err(|_| unavailable())?;
    advance_owned(shell, a)?;
    let r = shell
        .app_home
        .channel_fetches
        .unknown
        .slots
        .get_mut(&ctx.key())
        .unwrap();
    r.manager
        .published(&r.scope, settlement)
        .map_err(|_| unavailable())?;
    r.active = false;
    shell.app_home.channel_fetches.slots[&ctx.key()].notify();
    Ok(())
}
