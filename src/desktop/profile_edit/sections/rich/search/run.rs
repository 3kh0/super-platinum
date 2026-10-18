//! A mounted query delivers local results, remote refresh, then accepted draft
//! observation changes. Labels cross into the view; input witnesses stay here.
use super::*;
use std::collections::BTreeSet;
use super_platinum_core::slack::api::custom::texty::{matching, ranking as rank};

#[derive(Clone)]
struct Batch {
    source: Arc<canonical::Canonical>,
    rows: Vec<Value>,
    renderer: Arc<render::Observed>,
    input: ranking::Input,
    remote_ids: BTreeSet<String>,
}

pub(crate) async fn run(
    mut state: Signal<ShellState>,
    lease: query::Lease,
    raw_query: String,
    on_results: impl Fn(query::Presentation) + 'static,
) -> Result<(), Error> {
    let search = Arc::new(Search::capture(state, lease)?);
    let formatted = render::formatted_query(&raw_query).to_owned();
    let path = {
        let shell = state.peek();
        crate::connection::activity::Observation::capture(
            &shell,
            search.context.epoch,
            &search.context.transport,
            &search.context.session,
            &formatted,
        )
        .ok_or_else(unavailable)?
    };
    let prior = { search.preferences(&state.peek()) };
    let preferences = ranking::capture_preferences(state, &search, prior).await?;
    if !search.retain_preferences(&mut state.write(), preferences) {
        return Err(unavailable());
    }
    let (changes, mut changed) = tokio::sync::mpsc::unbounded_channel();
    ranking::schedule_drafts(state, search.clone(), move |_, result| {
        let _ = changes.send(result);
    })?;
    let source = canonical::capture(&mut state.write(), search.lease.team())?;
    let local = async {
        let batch = capture(state, &search, source, &raw_query, BTreeSet::new()).await?;
        publish(state, search.clone(), batch.clone(), &on_results)?;
        Ok::<_, Error>(batch)
    };
    let mut batch = if path.path() == crate::connection::activity::Path::Local {
        local.await?
    } else {
        // Poll both branches together; local publication does not await remote.
        let (local_result, remote_result) = tokio::join!(local, search.remote(state, &formatted));
        if !search.current(&state.peek()) || !path.current(&state.peek(), &formatted) {
            return Err(unavailable());
        }
        let acquired = remote_result?;
        let ids = acquired
            .pages
            .rows()?
            .iter()
            .map(|row| {
                row["id"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(unavailable)
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        // A complete refreshed pass can recover unavailable initial local facts.
        let _ = local_result;
        let batch = capture(state, &search, acquired.source, &raw_query, ids).await?;
        publish(state, search.clone(), batch.clone(), &on_results)?;
        batch
    };
    loop {
        let result = tokio::select! {
            biased;
            _ = search.lease.cancelled() => return Ok(()),
            result = changed.recv() => match result { Some(value) => value, None => return Ok(()) },
        };
        result?;
        if !path.current(&state.peek(), &formatted) {
            return Err(unavailable());
        }
        if batch.input.current(&state.peek(), &search) {
            continue;
        }
        // Only the original non-draft observations may survive this refresh.
        // A clone keeps older selection witnesses immutable until replacement.
        let mut next = (*batch).clone();
        next.input.refresh_drafts(&state.peek(), &search)?;
        batch = Arc::new(next);
        publish(state, search.clone(), batch.clone(), &on_results)?;
    }
}

async fn capture(
    state: Signal<ShellState>,
    search: &Search,
    source: Arc<canonical::Canonical>,
    raw_query: &str,
    remote_ids: BTreeSet<String>,
) -> Result<Arc<Batch>, Error> {
    let rows = source
        .rows()
        .iter()
        .filter(|row| matching::ordinary_local(row))
        .cloned()
        .collect::<Vec<_>>();
    let renderer =
        Arc::new(render::capture(state, search, raw_query, source.clone(), &rows).await?);
    let input = ranking::capture(state, search, source.clone(), &rows).await?;
    Ok(Arc::new(Batch {
        source,
        rows,
        renderer,
        input,
        remote_ids,
    }))
}

fn publish(
    mut state: Signal<ShellState>,
    search: Arc<Search>,
    batch: Arc<Batch>,
    on_results: &impl Fn(query::Presentation),
) -> Result<(), Error> {
    if !search.current(&state.peek())
        || !batch.renderer.current(&search, &state.peek())
        || !batch.input.current(&state.peek(), &search)
    {
        return Err(unavailable());
    }
    let matched = matching::channels(
        &batch.renderer.formatted_query,
        &batch.renderer.query,
        &batch.rows,
        &batch.remote_ids,
        batch.input.passthrough,
        |row| batch.renderer.channel_name(row),
        |value| batch.renderer.normalize(value),
    )
    .map_err(|_| unavailable())?;
    let normalize = |value: &str| batch.renderer.normalize(value);
    let ranked = {
        let mut cache = search
            .ranking
            .similarity()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        rank::rank_channels(
            &batch.renderer.formatted_query,
            &matched,
            &rank::Context {
                state: &batch.input.selectors,
                history: &batch.input.history,
                model: search.ranking.model(),
                now_ms: batch.input.now_ms,
                normalize: &normalize,
            },
            &mut cache,
            |a, b| batch.renderer.compare(a, b),
            batch.renderer.demote_passthrough(),
        )
        .map_err(|_| unavailable())?
    };
    let ids = ranked
        .rows
        .iter()
        .map(|r| {
            matched
                .get(r.original_index)
                .map(|m| m.id().to_owned())
                .ok_or_else(unavailable)
        })
        .collect::<Result<Vec<_>, _>>()?;
    for id in &ranked.latest_ids {
        canonical::hit(state, &batch.source, id)?;
    }
    let (owner, retained) = (search.clone(), batch.clone());
    let witness = Arc::new(move |shell: &ShellState| {
        owner.current(shell)
            && retained.renderer.current(&owner, shell)
            && retained.input.current(shell, &owner)
    });
    let presentation = query::publish_channels_checked(
        &mut state.write(),
        &search.lease,
        batch.source.clone(),
        &ids,
        Some(witness),
    )
    .ok_or_else(unavailable)?;
    on_results(presentation);
    Ok(())
}
