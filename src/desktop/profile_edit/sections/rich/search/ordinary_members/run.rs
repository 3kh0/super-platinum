use super::*;

pub(crate) async fn run_list_members<L: Host>(
    mut state: Signal<ShellState>,
    lease: L,
    raw_query: String,
    on_results: impl Fn(Signal<ShellState>, MemberBatch) -> Result<(), Error> + 'static,
) -> Result<(), Error> {
    if !L::LIST {
        return Err(unavailable());
    }
    let search = Arc::new(Search::capture(state, lease)?);
    let prior = search.preferences(&state.peek());
    let preferences = ranking::capture_preferences(state, &search, prior).await?;
    if !search.retain_preferences(&mut state.write(), preferences.clone()) {
        return Err(unavailable());
    }
    // aD6p removes one @ before Searcher only when the observed file gate is on.
    // Other categories are outside this ordinary-member publication, not disabled.
    let file_policy = Arc::new(file_policy::Observation::capture(
        &state.peek(),
        &search,
        preferences.clone(),
    )?);
    let file_enabled = file_policy.policy().enabled();
    let query = if file_enabled {
        raw_query
            .strip_prefix(['@', '\u{ff20}'])
            .unwrap_or(&raw_query)
    } else {
        &raw_query
    }
    .to_owned();
    let formatted = render::formatted_query(&query).to_owned();
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
    let names = Arc::new(member_names::capture(state, &search, preferences.clone()).await?);
    let group_policy = Arc::new(group_policy::Observation::capture(&state.peek(), &search)?);
    let (changes, mut changed) = tokio::sync::mpsc::unbounded_channel();
    ranking::schedule_drafts(state, search.clone(), move |_, result| {
        let _ = changes.send(result);
    })?;
    if file_enabled {
        file_read::initialize(&mut state.write(), &search, names.clone(), &file_policy)?;
    }
    let file_state = state;
    let file_remote = async {
        if path.path() != crate::connection::activity::Path::Local && file_enabled {
            file_read::remote(file_state, &search, names.clone(), &file_policy, &formatted).await
        } else {
            Ok(None)
        }
    };
    tokio::pin!(file_remote);
    let ordinary_state = state;
    let ordinary = async {
        let state = ordinary_state;
        let local = async {
            let source = Arc::new(intake::snapshot(
                &state.peek(),
                search.lease.team(),
                &search.lease.member_generation(),
            )?);
            // End the Signal read before the renderer/remote acquisition can await.
            let groups = local_groups(&state.peek(), &search, &group_policy);
            let batch = capture(
                state,
                &search,
                source,
                groups,
                group_policy.clone(),
                file_policy.clone(),
                names.clone(),
                preferences.clone(),
                &query,
                BTreeSet::new(),
            )
            .await?;
            publish(
                state,
                search.clone(),
                batch.clone(),
                path.path() == crate::connection::activity::Path::Local && !file_enabled,
                &on_results,
            )?;
            // Missing effective history is not an authenticated empty group list.
            // Accepted members may paint first; the query still reports the failure.
            if batch.group_policy.policy().enabled() && batch.groups.is_none() {
                return Err(unavailable());
            }
            Ok::<_, Error>(batch)
        };
        let batch = if path.path() == crate::connection::activity::Path::Local {
            local.await?
        } else {
            let (local_result, remote_result) = tokio::join!(
                local,
                member_read::remote(state, &search, &names, &formatted)
            );
            if !search.current(&state.peek()) || !path.current(&state.peek(), &formatted) {
                return Err(unavailable());
            }
            let acquired = remote_result?;
            let ids = acquired
                .pages
                .rows()?
                .iter()
                .map(|r| r["id"].as_str().map(str::to_owned).ok_or_else(unavailable))
                .collect::<Result<BTreeSet<_>, _>>()?;
            let _ = local_result;
            // The member response can include self. Observe its committed permission
            // before starting any group request, never while self intake is pending.
            let group_policy =
                Arc::new(group_policy::Observation::capture(&state.peek(), &search)?);
            let groups = local_groups(&state.peek(), &search, &group_policy);
            let interim = capture(
                state,
                &search,
                acquired.source.clone(),
                groups,
                group_policy.clone(),
                file_policy.clone(),
                names.clone(),
                preferences.clone(),
                &query,
                ids.clone(),
            )
            .await?;
            publish(state, search.clone(), interim, false, &on_results)?;
            let group_result = if group_policy.policy().enabled() {
                group_read::remote(state, &search, &names, &group_policy, &formatted)
                    .await
                    .map(Some)
            } else {
                Ok(None)
            };
            let (groups, group_error) = match group_result {
                Ok(value) => (value.map(|value| value.source), None),
                Err(error) => (None, Some(error)),
            };
            let batch = capture(
                state,
                &search,
                acquired.source,
                groups,
                group_policy,
                file_policy.clone(),
                names.clone(),
                preferences.clone(),
                &query,
                ids,
            )
            .await?;
            publish(
                state,
                search.clone(),
                batch.clone(),
                group_error.is_none() && !file_enabled,
                &on_results,
            )?;
            if let Some(error) = group_error {
                return Err(error);
            }
            batch
        };
        Ok::<_, Error>(batch)
    };
    tokio::pin!(ordinary);
    // File IO is polled alongside people/groups, but source installation waits
    // until their renderer observations finish. The original query owns both.
    let mut file_done = false;
    let mut arrival = None;
    let mut batch = tokio::select! { biased;
        _=search.lease.cancelled()=>return Ok(()),
        result=&mut ordinary=>result?,
        result=&mut file_remote=>{
            file_done=true;
            arrival=Some(result);
            ordinary.await?
        },
    };
    if let Some(result) = arrival {
        if let Some(acquired) = result? {
            acquired.publish(&mut state.write(), &search, &file_policy, &formatted)?;
            batch = recapture(state, &search, &batch, &query).await?;
        }
        if file_enabled {
            publish(state, search.clone(), batch.clone(), true, &on_results)?;
        }
    }
    // Only the settled final batch is watched: our own remote member/group
    // adoption legitimately invalidates earlier local/interim snapshots.
    let mut draft_changes_open = true;
    loop {
        tokio::select! {biased;
            _=search.lease.cancelled()=>return Ok(()),
            result=&mut file_remote, if !file_done => {
                file_done=true;
                if !path.current(&state.peek(), &formatted) { return Err(source_retired()); }
                if let Some(acquired)=result? {
                    acquired.publish(&mut state.write(), &search, &file_policy, &formatted)?;
                    batch=recapture(state, &search, &batch, &query).await?;
                }
                if file_enabled { publish(state, search.clone(), batch.clone(), true, &on_results)?; }
                continue;
            },
            result=changed.recv(), if draft_changes_open => match result {
                Some(result) => result?,
                None => draft_changes_open = false,
            },
            _=tokio::time::sleep(std::time::Duration::from_millis(100))=>{},
        };
        if !path.current(&state.peek(), &formatted) {
            return Err(source_retired());
        }
        if batch.current(&state.peek(), &search) {
            continue;
        }
        let mut next = (*batch).clone();
        // Draft presence is the one permitted refresh. All original member,
        // group, self-policy, renderer and context witnesses must still match.
        next.input
            .refresh_drafts(&state.peek(), &search)
            .map_err(|_| source_retired())?;
        if !next.current(&state.peek(), &search) {
            return Err(source_retired());
        }
        batch = Arc::new(next);
        publish(
            state,
            search.clone(),
            batch.clone(),
            !file_enabled || file_done,
            &on_results,
        )?;
    }
}

async fn recapture<L: Host>(
    state: Signal<ShellState>,
    search: &Search<L>,
    batch: &Batch,
    query: &str,
) -> Result<Arc<Batch>, Error> {
    capture(
        state,
        search,
        batch.source.clone(),
        batch.groups.clone(),
        batch.group_policy.clone(),
        batch.file_policy.clone(),
        batch.names.clone(),
        batch.preferences.clone(),
        query,
        batch.remote_ids.clone(),
    )
    .await
}
