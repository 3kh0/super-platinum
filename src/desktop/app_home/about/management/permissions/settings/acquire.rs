use super::*;
use protocol::settings_acquisition::{Read, SearchKind};
use std::time::Instant;
use super_platinum_core::block_actions::directory_endpoint::DirectoryEndpoint;

pub(super) fn current(s: &ShellState, o: &Original, generation: u64) -> bool {
    owned(s, o.visit, o.id)
        .and_then(|c| c.settings.as_ref())
        .is_some_and(|v| v.generation == generation && v.current(s, o))
}
pub(super) async fn retired(state: Signal<ShellState>, o: &Original, generation: u64) {
    loop {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if !current(&state.peek(), o, generation) {
            return;
        }
    }
}
pub(super) async fn authorize(
    state: Signal<ShellState>,
    o: &Original,
    generation: u64,
) -> Result<(), Error> {
    if !current(&state.peek(), o, generation) || cooling(&state.peek(), &o.owner.ctx) {
        return Err(unavailable());
    }
    let expected = {
        let s = state.peek();
        owned(&s, o.visit, o.id)
            .unwrap()
            .settings
            .as_ref()
            .unwrap()
            .expected()
    };
    let still_current = || {
        let s = state.peek();
        current(&s, o, generation)
            && !cooling(&s, &o.owner.ctx)
            && owned(&s, o.visit, o.id)
                .and_then(|c| c.settings.as_ref())
                .is_some_and(|v| Some(v.epoch) == expected["epoch"].as_u64())
    };
    // A mode change may need a prerequisite read before it can commit. Its
    // expected controls must first render under the same original owner; this
    // passive bind cannot replay the action that selected the mode.
    if !native::bind_settings(o, &expected).await || !still_current() {
        return Err(unavailable());
    }
    if !native::settings(o, &expected, None).await || !still_current() {
        return Err(unavailable());
    }
    Ok(())
}
pub(super) async fn discovery(
    state: Signal<ShellState>,
    o: &Original,
    generation: u64,
) -> Result<(DirectoryEndpoint, Value), Error> {
    authorize(state, o, generation).await?;
    let ctx = &o.owner.ctx;
    let request = DirectoryEndpoint::discovery(&ctx.client, &ctx.session)?;
    let observed = Instant::now();
    let raw = if fixture::enabled(&state.peek()) {
        tokio::select! {r=fixture::run_access::read(state,&request)=>r.ok_or_else(unavailable)??,_=retired(state,o,generation)=>return Err(unavailable())}
    } else if crate::fixture::is_fixture() {
        return Err(unavailable());
    } else {
        tokio::select! {r=ctx.transport.execute(request)=>r?,_=retired(state,o,generation)=>return Err(unavailable())}
    };
    if !current(&state.peek(), o, generation) {
        return Err(unavailable());
    }
    Ok((
        DirectoryEndpoint::decode(&ctx.session, &raw, observed)?,
        raw,
    ))
}
pub(super) async fn request(
    state: Signal<ShellState>,
    o: &Original,
    generation: u64,
    read: &Read,
    endpoint: Option<&DirectoryEndpoint>,
) -> Result<(Value, Option<Vec<String>>), Error> {
    authorize(state, o, generation).await?;
    let ctx = &o.owner.ctx;
    let request = read.request(&ctx.client, &ctx.session, endpoint, Instant::now())?;
    let result = if fixture::enabled(&state.peek()) {
        tokio::select! {result=fixture::run_access::picker(state,&request)=>result.unwrap_or_else(||Err(unavailable())),_=retired(state,o,generation)=>Err(unavailable())}
    } else if crate::fixture::is_fixture() {
        Err(unavailable())
    } else {
        tokio::select! {result=ctx.transport.execute_workflow_settings_read(read,&ctx.client,&ctx.session,endpoint,Instant::now(),||current(&state.peek(),o,generation))=>result.map(|v|v.into_parts()),_=retired(state,o,generation)=>Err(unavailable())}
    };
    if !current(&state.peek(), o, generation) {
        return Err(unavailable());
    }
    result
}
pub(super) async fn search(
    mut state: Signal<ShellState>,
    o: Original,
    generation: u64,
    external: bool,
    text: String,
) {
    if !current(&state.peek(), &o, generation) {
        return;
    }
    let result = acquire(state, &o, generation, external, &text).await;
    if authorize(state, &o, generation).await.is_err() {
        return;
    }
    let mut s = state.write();
    if let Err(Error::RateLimited { retry_after_secs }) = &result {
        navigation::retain_limit(&mut s, &o.owner.ctx, *retry_after_secs);
    }
    if !current(&s, &o, generation) {
        return;
    }
    let settings = s
        .app_home
        .about
        .management
        .permissions
        .confirmation
        .as_mut()
        .unwrap()
        .settings
        .as_mut()
        .unwrap();
    settings.pending = false;
    match result {
        Ok((rows, receipts, available)) => {
            let mut ids = Vec::new();
            for raw in rows {
                if let Some((entity, row)) = source::row(&raw, &settings.context) {
                    if !entity.eligible
                        || row.external != external
                        || external && settings.policy.allows_external(&row.id) != Ok(true)
                    {
                        continue;
                    }
                    if !ids.contains(&row.id) {
                        ids.push(row.id.clone());
                    }
                    settings.context.entities.retain(|e| e.id != entity.id);
                    settings.context.entities.push(entity);
                    settings.rows.insert(row.id.clone(), row);
                }
            }
            if let Some(available) = available {
                settings.context.available_external_ids = Some(available);
            }
            settings.receipts.extend(receipts);
            ids.truncate(24);
            if external {
                settings.external_results = ids;
            } else {
                settings.results = ids;
            }
            settings.notice = None;
            if let Some(mode) = settings.pending_mode.take() {
                if settings
                    .draft
                    .as_mut()
                    .is_none_or(|d| d.select_mode(mode, &settings.context).is_err())
                {
                    settings.notice=Some("Available external organizations could not be verified. Your previous selection is retained.".into());
                }
            }
        }
        Err(_) => {
            settings.pending_mode = None;
            settings.notice=Some("Reference details could not be loaded. Your selections are retained. Search again to retry.".into());
        }
    }
    settings.epoch = next();
}
async fn acquire(
    state: Signal<ShellState>,
    o: &Original,
    generation: u64,
    external: bool,
    text: &str,
) -> Result<(Vec<Value>, Vec<Value>, Option<Vec<String>>), Error> {
    let query = query::Query::new(text);
    let (mut rows, scope, available_needed) = {
        let s = state.peek();
        let v = owned(&s, o.visit, o.id)
            .and_then(|c| c.settings.as_ref())
            .ok_or_else(unavailable)?;
        (
            v.rows
                .values()
                .filter(|r| {
                    v.context.entity(&r.id).is_ok_and(|e| e.eligible)
                        && r.external == external
                        && query.accepts(r.kind, external)
                        && (query.local().is_empty()
                            || r.label
                                .to_lowercase()
                                .contains(&query.local().to_lowercase()))
                })
                .map(|r| r.raw.clone())
                .collect::<Vec<_>>(),
            v.context.scope_id.clone(),
            external && v.context.available_external_ids.is_none(),
        )
    };
    // The native source suppresses empty-query remote search. Local facts are
    // already current source observations, not a fabricated remote response.
    if query.remote().is_empty() && !available_needed {
        return Ok((rows, vec![], None));
    }
    let mut kinds = query.routes(external, scope.starts_with('E'));
    if query.remote().is_empty() {
        kinds.clear();
    }
    if available_needed {
        kinds.insert(0, SearchKind::AvailableExternal);
    }
    let ctx = &o.owner.ctx;
    let (endpoint, raw) = discovery(state, o, generation).await?;
    let mut receipts = vec![raw];
    let mut available = None;
    for kind in kinds {
        let mut marker: Option<String> = None;
        let mut seen = std::collections::BTreeSet::new();
        let mut count = 0;
        let mut route_rows = Vec::new();
        loop {
            count += 1;
            if count > 20 {
                return Err(unavailable());
            }
            let read = Read::new(
                &ctx.session,
                kind,
                if matches!(kind, SearchKind::AvailableExternal) {
                    ""
                } else {
                    query.remote()
                },
                marker.as_deref(),
            )?;
            let (raw, keys) = request(state, o, generation, &read, Some(&endpoint)).await?;
            let page = read.decode_ordered(raw, keys.as_deref())?;
            receipts.push(page.raw().clone());
            let mut page_rows = page.rows().to_vec();
            if matches!(kind, SearchKind::Channels) {
                for row in &mut page_rows {
                    row["is_member"] = json!(
                        page.membership_ids()
                            .iter()
                            .any(|id| row["id"].as_str() == Some(id.as_str()))
                    );
                }
            }
            if matches!(kind, SearchKind::AvailableExternal) {
                let ids = available.get_or_insert_with(Vec::new);
                for row in &page_rows {
                    if let Some(id) = row["id"].as_str() {
                        if !ids.iter().any(|v| v == id) {
                            ids.push(id.to_owned());
                        }
                    }
                }
            }
            route_rows.extend(page_rows);
            marker = page.next_marker().map(str::to_owned);
            if !query::more(kind, route_rows.len(), marker.as_deref()) {
                break;
            }
            let Some(next) = marker.as_ref() else { break };
            if !seen.insert(next.clone()) {
                return Err(unavailable());
            }
        }
        if matches!(kind, SearchKind::Members | SearchKind::Channels) {
            route_rows.truncate(24);
        }
        rows.extend(route_rows);
    }
    Ok((rows, receipts, available))
}
