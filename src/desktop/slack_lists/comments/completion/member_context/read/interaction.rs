//! The file-channel first-N branch acquires canInteract through conditional
//! users/info, then upserts the original users/list rows in encounter order.
use super::*;
use std::collections::BTreeSet;

pub(super) async fn acquire(
    state: Signal<ShellState>,
    owner: &Fence,
    endpoint: &DirectoryEndpoint,
    source: &intake::Snapshot,
    rows: &[Value],
    file: bool,
    current: &impl Fn(&ShellState) -> bool,
) -> Result<(Vec<Value>, Value), Error> {
    let mut seen = BTreeSet::new();
    let ids = rows
        .iter()
        .map(|r| r["id"].as_str().map(str::to_owned).ok_or_else(unavailable))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect::<Vec<_>>();
    if !file {
        let interactions = ids
            .iter()
            .map(|id| (id.clone(), true))
            .collect::<BTreeMap<_, _>>();
        let projected = member_info::project_interaction(rows, &interactions, &BTreeMap::new())?;
        return Ok((
            projected,
            serde_json::to_value(interactions).map_err(|_| unavailable())?,
        ));
    }
    if ids.is_empty() {
        return Ok((Vec::new(), json!({})));
    }
    let ctx = &owner.context;
    let (mut reads, prior) = {
        let shell = state.peek();
        if !current(&shell) || !source.current(&shell) {
            return Err(unavailable());
        }
        let w = shell
            .core
            .workspaces
            .get(&ctx.session.team_id)
            .ok_or_else(unavailable)?;
        let observed = w.directory_members.observations(
            &owner.account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
        )?;
        let prior = observed.interactions().clone();
        let known = ids
            .iter()
            .filter(|id| prior.contains_key(*id))
            .cloned()
            .collect::<Vec<_>>();
        let mut reads = Vec::new();
        for chunk in known.chunks(100) {
            reads.push(member_info::Read::conditional(
                chunk.to_vec(),
                source.rows(),
            )?);
        }
        let missing = member_info::unknown::Capture::network_only(observed, &ids)?;
        reads.extend(missing.reads()?);
        (reads, prior)
    };
    let mut interactions = BTreeMap::new();
    for original in reads.drain(..) {
        let mut read = original;
        let mut markers = BTreeSet::new();
        let mut complete = false;
        for _ in 0..50 {
            let request = endpoint.member_info(&ctx.client, &ctx.session, Instant::now(), &read)?;
            let admitted = |s: &ShellState| {
                current(s) && source.current(s) && endpoint.current(&ctx.session, Instant::now())
            };
            let (raw, keys) = execute(state, owner, request, Kind::Member, &admitted).await?;
            let page = member_info::Page::decode(&read, raw, keys)?;
            // An unresolved or rejected interaction observation never becomes
            // a positive permission or an invented known-negative member.
            if !page.pending_ids().is_empty()
                || !page.failed_ids().is_empty()
                || !page.invalid_ids().is_empty()
            {
                return Err(unavailable());
            }
            interactions.extend(page.interactions().clone());
            match page.next_marker() {
                None => {
                    complete = true;
                    break;
                }
                Some(next) if markers.insert(next.to_owned()) => read = read.next_page(next)?,
                _ => return Err(unavailable()),
            }
        }
        if !complete {
            return Err(unavailable());
        }
    }
    let projected = member_info::project_interaction(rows, &interactions, &prior)?;
    // Retain actual canInteract facts separately from the projected raw rows.
    // Missing values stay missing; enqueue's is_stranger projection is explicit.
    Ok((
        projected,
        serde_json::to_value(interactions).map_err(|_| unavailable())?,
    ))
}
