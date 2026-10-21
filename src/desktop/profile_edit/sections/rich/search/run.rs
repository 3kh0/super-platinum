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
    state: Signal<ShellState>,
    lease: query::Lease,
    raw_query: String,
    on_results: impl Fn(query::Presentation) + 'static,
) -> Result<(), Error> {
    let publish_lease = lease.clone();
    run_channels(state, lease, raw_query, move |mut state, batch| {
        let presentation = query::publish_channels_checked(
            &mut state.write(),
            &publish_lease,
            batch.source,
            &batch.ids,
            Some(batch.witness),
        )
        .ok_or_else(unavailable)?;
        on_results(presentation);
        Ok(())
    })
    .await
}

/// Ranked private receipt; caller owns publication and renderer insertion.
#[derive(Clone)]
pub(crate) struct ChannelBatch {
    source: Arc<canonical::Canonical>,
    ids: Vec<String>,
    witness: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>,
}
impl ChannelBatch {
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        (self.witness)(shell)
    }
    pub(crate) fn choices(&self, shell: &ShellState) -> Option<Vec<(Value, String)>> {
        if !self.current(shell) {
            return None;
        }
        self.ids
            .iter()
            .map(|id| {
                let token = serde_json::json!({"type":"channel", "channel_id":id});
                let label = canonical::ordinary_label(&token, self.source.row(id)?).ok()?;
                Some((token, label))
            })
            .collect()
    }
}

pub(crate) async fn run_channels<L: Host>(
    mut state: Signal<ShellState>,
    lease: L,
    raw_query: String,
    on_results: impl Fn(Signal<ShellState>, ChannelBatch) -> Result<(), Error> + 'static,
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

async fn capture<L: Host>(
    state: Signal<ShellState>,
    search: &Search<L>,
    source: Arc<canonical::Canonical>,
    raw_query: &str,
    remote_ids: BTreeSet<String>,
) -> Result<Arc<Batch>, Error> {
    let rows = source
        .rows()
        .iter()
        .filter(|row| channel_host::ordinary::<L>(row))
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

fn publish<L: Host>(
    state: Signal<ShellState>,
    search: Arc<Search<L>>,
    batch: Arc<Batch>,
    on_results: &impl Fn(Signal<ShellState>, ChannelBatch) -> Result<(), Error>,
) -> Result<(), Error> {
    if !search.current(&state.peek())
        || !batch.renderer.current(&search, &state.peek())
        || !batch.input.current(&state.peek(), &search)
    {
        return Err(unavailable());
    }
    let name = |row: &Value| batch.renderer.channel_name(row);
    let previous = |value: &str| batch.renderer.normalize(value);
    let matched = if L::LIST {
        matching::list_channels(
            &batch.renderer.formatted_query,
            &batch.renderer.query,
            &batch.rows,
            &batch.remote_ids,
            batch.input.passthrough,
            &name,
            &previous,
        )
    } else {
        matching::channels(
            &batch.renderer.formatted_query,
            &batch.renderer.query,
            &batch.rows,
            &batch.remote_ids,
            batch.input.passthrough,
            &name,
            &previous,
        )
    }
    .map_err(|_| unavailable())?;
    let normalize = |value: &str| batch.renderer.normalize(value);
    let ranked = {
        let mut cache = search
            .ranking
            .similarity()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let context = rank::Context {
            state: &batch.input.selectors,
            history: &batch.input.history,
            model: search.ranking.model(),
            now_ms: batch.input.now_ms,
            normalize: &normalize,
        };
        let compare = |a: &str, b: &str| batch.renderer.compare(a, b);
        if L::LIST {
            rank::rank_list_channels(
                &batch.renderer.formatted_query,
                &matched,
                &context,
                &mut cache,
                compare,
                batch.renderer.demote_passthrough(),
            )
        } else {
            rank::rank_channels(
                &batch.renderer.formatted_query,
                &matched,
                &context,
                &mut cache,
                compare,
                batch.renderer.demote_passthrough(),
            )
        }
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
    on_results(
        state,
        ChannelBatch {
            source: batch.source.clone(),
            ids,
            witness,
        },
    )
}
