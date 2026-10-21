use super::*;
use protocol::settings_acquisition::{Read, SearchKind};
use std::time::Instant;
use super_platinum_core::block_actions::directory_endpoint::{DirectoryEndpoint, member_info};
pub(super) async fn retired(state: Signal<ShellState>, o: &Original, g: u64) {
    loop {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if !ready(&state.peek(), o, g) {
            return;
        }
    }
}
pub(super) async fn authorize(
    state: Signal<ShellState>,
    o: &Original,
    g: u64,
) -> Result<(), Error> {
    let e = {
        let s = state.peek();
        let m = owned(&s, o)
            .filter(|m| m.generation == g && m.current(&s, o))
            .ok_or_else(unavailable)?;
        m.expected()
    };
    if cooling(&state.peek(), &o.owner.ctx)
        || !native::witness(o, &e, None, true).await
        || !actions::checked(state, o, &e, None).await
        || !ready(&state.peek(), o, g)
    {
        return Err(unavailable());
    }
    Ok(())
}
pub(super) fn start(
    mut state: Signal<ShellState>,
    visit: u64,
    id: u64,
    hydrate: bool,
    retry: bool,
) {
    let captured = {
        let s = state.peek();
        let Some(c) = settings::owned(&s, visit, id) else {
            return;
        };
        let Some(m) = c.managers.as_ref() else { return };
        if m.pending
            || m.speed_bump
            || hydrate && (m.hydration_started && !retry || m.missing().is_empty())
        {
            return;
        }
        (c.original.clone(), m.generation)
    };
    dioxus::core::spawn_forever(async move {
        let (o, g) = captured;
        load_and_adopt(state, o, g, hydrate, None).await;
    });
}
pub(super) async fn start_search(state: Signal<ShellState>, o: Original, g: u64) {
    load_and_adopt(state, o, g, false, None).await;
}
pub(super) fn retry_facts(state: Signal<ShellState>, visit: u64, id: u64) {
    let captured = {
        let s = state.peek();
        settings::owned(&s, visit, id).and_then(|c| {
            let m = c
                .managers
                .as_ref()
                .filter(|m| m.current(&s, &c.original) && m.facts_retry() && !m.speed_bump)?;
            let (rows, hydrate) = m.agent_facts.as_ref()?.retry_rows(m.generation)?;
            Some((c.original.clone(), m.generation, rows, hydrate))
        })
    };
    if let Some((o, g, rows, hydrate)) = captured {
        dioxus::core::spawn_forever(async move {
            load_and_adopt(state, o, g, hydrate, Some(rows)).await;
        });
    }
}
async fn load_and_adopt(
    mut state: Signal<ShellState>,
    o: Original,
    g: u64,
    hydrate: bool,
    retry_rows: Option<Vec<Value>>,
) {
    if authorize(state, &o, g).await.is_err() {
        return;
    }
    let (ids, query) = {
        let mut s = state.write();
        if !ready(&s, &o, g) {
            return;
        }
        let m = owned_mut(&mut s).unwrap();
        if m.pending {
            return;
        }
        m.pending = true;
        m.notice = None;
        if hydrate {
            m.hydration_started = true;
        }
        (m.missing(), m.query.clone())
    };
    let loaded = if let Some(rows) = retry_rows {
        Ok((rows, vec![]))
    } else {
        load(state, &o, g, hydrate, &ids, &query).await
    };
    let result = match loaded {
        Ok((rows, receipts)) => {
            let result = agents::acquire(state, &o, g, rows, hydrate).await;
            result
                .and_then(|a| a.project(g, &o.owner.ctx.session.team_id))
                .map(|rows| (rows, receipts))
        }
        Err(error) => Err(error),
    };
    if authorize(state, &o, g).await.is_err() {
        return;
    }
    let mut s = state.write();
    if let Err(Error::RateLimited { retry_after_secs }) = &result {
        navigation::retain_limit(&mut s, &o.owner.ctx, *retry_after_secs);
    }
    if !ready(&s, &o, g) {
        return;
    }
    let m = owned_mut(&mut s).unwrap();
    m.pending = false;
    if let Some(facts) = &mut m.agent_facts {
        facts.finish(g, result.is_ok());
    }
    match result {
        Ok((rows,receipts))=>{
            let mut results=vec![];
            for row in rows {
                if row.eligible&&!results.contains(&row.id) {results.push(row.id.clone());}
                m.rows.insert(row.id.clone(),row);
            }
            m.receipts.extend(receipts);
            if !hydrate {results.truncate(24);m.results=results;}
            if hydrate&&!m.missing().is_empty() {
                m.notice=Some("Some workflow manager details are unavailable. Retry to load their details.".into());
            }
        }
        Err(_) => m.notice=Some(if m.facts_retry() {
            "Workflow manager eligibility could not be loaded. Your selections are retained. Retry to load eligibility details."
        } else if hydrate {
            "Workflow manager details could not be loaded. Your selections are retained. Retry to load their details."
        } else {
            "Workflow managers could not be searched. Your selections are retained. Search again to retry."
        }.into()),
    }
    m.epoch = next();
    drop(s);
    actions::bind(state, o.visit, o.id);
}

async fn discovery(
    state: Signal<ShellState>,
    o: &Original,
    g: u64,
) -> Result<(DirectoryEndpoint, Value), Error> {
    authorize(state, o, g).await?;
    let ctx = &o.owner.ctx;
    let request = DirectoryEndpoint::discovery(&ctx.client, &ctx.session)?;
    let now = Instant::now();
    let raw = if fixture::enabled(&state.peek()) {
        tokio::select! {r=fixture::managers::read(state,&request)=>r.ok_or_else(unavailable)??,_=retired(state,o,g)=>return Err(unavailable())}
    } else if crate::fixture::is_fixture() {
        return Err(unavailable());
    } else {
        tokio::select! {r=ctx.transport.execute(request)=>r?,_=retired(state,o,g)=>return Err(unavailable())}
    };
    if !ready(&state.peek(), o, g) {
        return Err(unavailable());
    }
    Ok((DirectoryEndpoint::decode(&ctx.session, &raw, now)?, raw))
}
async fn load(
    state: Signal<ShellState>,
    o: &Original,
    g: u64,
    hydrate: bool,
    ids: &[String],
    query: &str,
) -> Result<(Vec<Value>, Vec<Value>), Error> {
    let mut rows = if hydrate {
        vec![]
    } else {
        let s = state.peek();
        owned(&s, o)
            .ok_or_else(unavailable)?
            .rows
            .values()
            .filter(|r| {
                r.eligible
                    && (query.is_empty() || r.label.to_lowercase().contains(&query.to_lowercase()))
            })
            .map(|r| r.raw.clone())
            .collect()
    };
    if !hydrate && query.is_empty() {
        return Ok((rows, vec![]));
    }
    let (endpoint, receipt) = discovery(state, o, g).await?;
    let mut receipts = vec![receipt];
    let ctx = &o.owner.ctx;
    if hydrate {
        for chunk in ids.chunks(100) {
            let mut read = member_info::Read::capture(chunk.to_vec())?;
            let mut seen = std::collections::BTreeSet::new();
            for _ in 0..20 {
                authorize(state, o, g).await?;
                let request =
                    endpoint.member_info(&ctx.client, &ctx.session, Instant::now(), &read)?;
                let (raw, keys) = if fixture::enabled(&state.peek()) {
                    tokio::select! {r=fixture::managers::picker(state,&request)=>r.ok_or_else(unavailable)??,_=retired(state,o,g)=>return Err(unavailable())}
                } else if crate::fixture::is_fixture() {
                    return Err(unavailable());
                } else {
                    tokio::select! {r=ctx.transport.execute_directory_member_info_current(request,||ready(&state.peek(),o,g))=>r?.into_parts(),_=retired(state,o,g)=>return Err(unavailable())}
                };
                if !ready(&state.peek(), o, g) {
                    return Err(unavailable());
                }
                let page = member_info::Page::decode(&read, raw, keys)?;
                rows.extend_from_slice(page.rows());
                receipts.push(page.raw().clone());
                let Some(next) = page.next_marker() else {
                    break;
                };
                if !seen.insert(next.to_owned()) {
                    return Err(unavailable());
                }
                read = read.next_page(next)?;
            }
        }
    } else {
        let mut marker = None;
        let mut seen = std::collections::BTreeSet::new();
        let mut count = 0;
        for _ in 0..20 {
            authorize(state, o, g).await?;
            let read = Read::new(
                &ctx.session,
                SearchKind::Members,
                query
                    .strip_prefix(['@', '\u{ff20}', '#', '\u{ff03}', '/', '\u{ff0f}'])
                    .unwrap_or(query),
                marker.as_deref(),
            )?;
            let request =
                read.request(&ctx.client, &ctx.session, Some(&endpoint), Instant::now())?;
            let (raw, keys) = if fixture::enabled(&state.peek()) {
                tokio::select! {r=fixture::managers::picker(state,&request)=>r.ok_or_else(unavailable)??,_=retired(state,o,g)=>return Err(unavailable())}
            } else if crate::fixture::is_fixture() {
                return Err(unavailable());
            } else {
                tokio::select! {r=ctx.transport.execute_workflow_settings_read(&read,&ctx.client,&ctx.session,Some(&endpoint),Instant::now(),||ready(&state.peek(),o,g))=>r?.into_parts(),_=retired(state,o,g)=>return Err(unavailable())}
            };
            if !ready(&state.peek(), o, g) {
                return Err(unavailable());
            }
            let page = read.decode_ordered(raw, keys.as_deref())?;
            count += page.rows().len();
            rows.extend_from_slice(page.rows());
            receipts.push(page.raw().clone());
            marker = page.next_marker().map(str::to_owned);
            if count >= 24 || marker.is_none() {
                break;
            }
            if !seen.insert(marker.clone()) {
                return Err(unavailable());
            }
        }
    }
    Ok((rows, receipts))
}
