//! Profile member suggestions over the accepted shared collection. Local results
//! can appear while remote acquisition runs; each publication owns its witnesses.
use super::*;
use crate::directory_member_fetches::search as intake;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::slack::api::custom::texty::{
    file_matching, groups, member_matching, member_ranking,
};

#[derive(Clone)]
struct Batch {
    source: Arc<intake::Snapshot>,
    groups: Option<Arc<group_source::Snapshot>>,
    group_policy: Arc<group_policy::Observation>,
    file_policy: Arc<file_policy::Observation>,
    files: Option<Arc<file_source::Snapshot>>,
    names: Arc<member_names::Observation>,
    preferences: Arc<ranking::Preferences>,
    renderer: Arc<member_render::Observed>,
    policy: Arc<member_policy::Observation>,
    input: member_rank_input::Input,
    eligible: BTreeSet<String>,
    remote_ids: BTreeSet<String>,
}
impl Batch {
    fn current(&self, shell: &ShellState, search: &Search) -> bool {
        search.current(shell)
            && self.source.current(shell)
            && self.group_policy.current(shell, search)
            && self.file_policy.current(shell, search)
            && self.files.as_ref().is_none_or(|s| s.current(shell))
            && self
                .groups
                .as_ref()
                .is_none_or(|source| source.current(shell))
            && self.renderer.current(search, shell)
            && self.policy.current(shell, search)
            && self.input.current(shell, search)
    }
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
    let prior = search.preferences(&state.peek());
    let preferences = ranking::capture_preferences(state, &search, prior).await?;
    if !search.retain_preferences(&mut state.write(), preferences.clone()) {
        return Err(unavailable());
    }
    let names = Arc::new(member_names::capture(state, &search, preferences.clone()).await?);
    let group_policy = Arc::new(group_policy::Observation::capture(&state.peek(), &search)?);
    let file_policy = Arc::new(file_policy::Observation::capture(
        &state.peek(),
        &search,
        preferences.clone(),
    )?);
    if file_policy.policy().enabled() {
        file_read::initialize(&mut state.write(), &search, names.clone(), &file_policy)?;
    }
    let acquisition_state = state;
    let file_remote = async {
        if path.path() != crate::connection::activity::Path::Local && file_policy.policy().enabled()
        {
            file_read::remote(
                acquisition_state,
                &search,
                names.clone(),
                &file_policy,
                &formatted,
            )
            .await
        } else {
            Ok(None)
        }
    };
    tokio::pin!(file_remote);
    let (changes, mut changed) = tokio::sync::mpsc::unbounded_channel();
    ranking::schedule_drafts(acquisition_state, search.clone(), move |_, result| {
        let _ = changes.send(result);
    })?;
    let local = async {
        let source = Arc::new(intake::snapshot(
            &acquisition_state.peek(),
            search.lease.team(),
            &search.lease.member_generation(),
        )?);
        let batch = capture(
            acquisition_state,
            &search,
            source,
            if group_policy.policy().enabled() {
                // Legacy/interrupted group history must not hide accepted people.
                // The remote leg reports its own failure; no empty history is minted.
                group_source::Snapshot::capture(&acquisition_state.peek(), search.lease.team())
                    .ok()
                    .map(Arc::new)
            } else {
                None
            },
            group_policy.clone(),
            file_policy.clone(),
            names.clone(),
            preferences.clone(),
            &raw_query,
            BTreeSet::new(),
        )
        .await?;
        publish(
            acquisition_state,
            search.clone(),
            batch.clone(),
            &on_results,
        )?;
        if batch.group_policy.policy().enabled() && batch.groups.is_none() {
            return Err(unavailable());
        }
        Ok::<_, Error>(batch)
    };
    let ordinary = async {
        let batch = if path.path() == crate::connection::activity::Path::Local {
            local.await?
        } else {
            let (local_result, remote_result) = tokio::join!(
                local,
                member_read::remote(acquisition_state, &search, &names, &formatted)
            );
            if !search.current(&acquisition_state.peek())
                || !path.current(&acquisition_state.peek(), &formatted)
            {
                return Err(unavailable());
            }
            let acquired = remote_result?;
            let remote_ids = acquired
                .pages
                .rows()?
                .iter()
                .map(|r| r["id"].as_str().map(str::to_owned).ok_or_else(unavailable))
                .collect::<Result<BTreeSet<_>, _>>()?;
            let _ = local_result;
            // Member acquisition may include self and change its policy. Finish
            // that owned intake before observing permission for the group's first
            // request; a pending self row must never authorize that request.
            let group_policy = Arc::new(group_policy::Observation::capture(
                &acquisition_state.peek(),
                &search,
            )?);
            let interim = capture(
                acquisition_state,
                &search,
                acquired.source.clone(),
                if group_policy.policy().enabled() {
                    group_source::Snapshot::capture(&acquisition_state.peek(), search.lease.team())
                        .ok()
                        .map(Arc::new)
                } else {
                    None
                },
                group_policy.clone(),
                file_policy.clone(),
                names.clone(),
                preferences.clone(),
                &raw_query,
                remote_ids.clone(),
            )
            .await?;
            publish(acquisition_state, search.clone(), interim, &on_results)?;
            let group_result = if group_policy.policy().enabled() {
                group_read::remote(
                    acquisition_state,
                    &search,
                    &names,
                    &group_policy,
                    &formatted,
                )
                .await
                .map(Some)
            } else {
                Ok(None)
            };
            let (group_source, group_error) = match group_result {
                Ok(value) => (value.map(|v| v.source), None),
                Err(error) => (None, Some(error)),
            };
            let batch = capture(
                acquisition_state,
                &search,
                acquired.source,
                group_source,
                group_policy.clone(),
                file_policy.clone(),
                names.clone(),
                preferences.clone(),
                &raw_query,
                remote_ids,
            )
            .await?;
            publish(
                acquisition_state,
                search.clone(),
                batch.clone(),
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
    // Poll both branches together. Initial and interim people publications run
    // inside ordinary; an unresolved file request never blocks those rows.
    let mut file_done = false;
    let mut file_arrival = None;
    let mut batch = tokio::select! {
        biased;
        _ = search.lease.cancelled() => return Ok(()),
        result = &mut ordinary => result?,
        result = &mut file_remote => {
            file_done = true;
            file_arrival = Some(result);
            ordinary.await?
        }
    };
    if let Some(arrival) = file_arrival {
        if let Some(acquired) = arrival? {
            acquired.publish(&mut state.write(), &search, &file_policy, &formatted)?;
            batch = recapture(state, &search, &batch, &raw_query).await?;
            publish(state, search.clone(), batch.clone(), &on_results)?;
        }
    }
    let mut drafts_done = false;
    loop {
        if drafts_done && file_done {
            return Ok(());
        }
        let result = tokio::select! {
            biased;
            _ = search.lease.cancelled() => return Ok(()),
            result = &mut file_remote, if !file_done => {
                file_done = true;
                let Some(acquired) = result? else { continue; };
                if !path.current(&state.peek(), &formatted) { return Err(unavailable()); }
                // Reuse the same member/group acquisition, not another remote
                // search. Only the newly published file source is reselected.
                acquired.publish(&mut state.write(), &search, &file_policy, &formatted)?;
                batch = recapture(state, &search, &batch, &raw_query).await?;
                publish(state, search.clone(), batch.clone(), &on_results)?;
                continue;
            }
            result = changed.recv(), if !drafts_done => match result {
                Some(value) => value,
                None => { drafts_done = true; continue; }
            },
        };
        result?;
        if !path.current(&state.peek(), &formatted) {
            return Err(unavailable());
        }
        if batch.current(&state.peek(), &search) {
            continue;
        }
        let mut next = (*batch).clone();
        next.input.refresh_drafts(&state.peek(), &search)?;
        batch = Arc::new(next);
        publish(state, search.clone(), batch.clone(), &on_results)?;
    }
}
async fn recapture(
    state: Signal<ShellState>,
    search: &Search,
    batch: &Batch,
    raw_query: &str,
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
        raw_query,
        batch.remote_ids.clone(),
    )
    .await
}
async fn capture(
    state: Signal<ShellState>,
    search: &Search,
    source: Arc<intake::Snapshot>,
    groups: Option<Arc<group_source::Snapshot>>,
    group_policy: Arc<group_policy::Observation>,
    file_policy: Arc<file_policy::Observation>,
    names: Arc<member_names::Observation>,
    preferences: Arc<ranking::Preferences>,
    raw_query: &str,
    remote_ids: BTreeSet<String>,
) -> Result<Arc<Batch>, Error> {
    let rows: Vec<_> = source.rows().iter().map(|r| r.source.clone()).collect();
    let policy = Arc::new(member_policy::capture(state, search, &rows, preferences.clone()).await?);
    let mut eligible = BTreeSet::new();
    for row in &rows {
        if policy.allows(row)? {
            eligible.insert(row["id"].as_str().ok_or_else(unavailable)?.to_owned());
        }
    }
    let files = if file_policy.policy().enabled() {
        let snapshot = file_source::Snapshot::capture(&state.peek(), search.lease.team())?;
        if !snapshot.unavailable_ids().is_empty() {
            return Err(unavailable());
        }
        Some(snapshot)
    } else {
        None
    };
    let renderer = Arc::new(
        member_render::capture_with_files(
            state,
            search,
            raw_query,
            source.clone(),
            names.clone(),
            groups.clone(),
            files.clone(),
        )
        .await?,
    );
    let input = member_rank_input::capture_with_files(
        state,
        search,
        names.clone(),
        source.clone(),
        preferences.clone(),
        &rows,
        |text| renderer.ordinary_lowercase(text),
        files.clone(),
    )
    .await?;
    let batch = Arc::new(Batch {
        source,
        groups,
        group_policy,
        file_policy,
        files,
        names,
        preferences,
        renderer,
        policy,
        input,
        eligible,
        remote_ids,
    });
    if !batch.current(&state.peek(), search) {
        return Err(unavailable());
    }
    Ok(batch)
}
fn publish(
    mut state: Signal<ShellState>,
    search: Arc<Search>,
    batch: Arc<Batch>,
    on_results: &impl Fn(query::Presentation),
) -> Result<(), Error> {
    if !batch.current(&state.peek(), &search) {
        return Err(unavailable());
    }
    let renderer = &batch.renderer;
    let matched = member_matching::members(
        &renderer.formatted_query,
        &renderer.query,
        renderer.rows(),
        &batch.remote_ids,
        batch.input.passthrough,
        matches!(renderer.query.normalized.as_str(), "me" | "you"),
        |row| {
            row["id"]
                .as_str()
                .is_some_and(|id| batch.eligible.contains(id))
        },
    )
    .map_err(|_| unavailable())?;
    let group_rows = batch.groups.as_ref().map(|s| s.rows()).unwrap_or(&[]);
    let matched_groups = groups::matches(
        &renderer.query,
        group_rows,
        |row| batch.group_policy.policy().allows(row, None),
        |row| renderer.group_names(row),
    )?;
    let matched_files = if let Some(files) = &batch.files {
        file_matching::matches(
            &renderer.query,
            files.rows(),
            renderer.query_has_link().map_err(|_| unavailable())?,
            batch.input.file_ignore_fuzzy()?,
            |row| renderer.template_context(row),
            |row| renderer.file_names(row),
        )
        .map_err(|_| unavailable())?
    } else {
        Vec::new()
    };
    let normalize = |text: &str| renderer.normalize(text);
    let ranked = {
        let mut cache = search
            .ranking
            .similarity()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        member_ranking::rank_mixed_files(
            &renderer.formatted_query,
            &matched,
            &matched_groups,
            &matched_files,
            &member_ranking::Context {
                state: &batch.input.selectors,
                history: &batch.input.history,
                model: search.ranking.model(),
                now_ms: batch.input.now_ms,
                normalize: &normalize,
            },
            &mut cache,
            |row| {
                row["id"]
                    .as_str()
                    .and_then(|id| renderer.comparisons().get(id))
                    .cloned()
                    .ok_or(member_ranking::Error::InvalidInput)
            },
            |a, b| renderer.compare(a, b),
            renderer.demote_passthrough(),
        )
        .map_err(|_| unavailable())?
    };
    let ids = ranked
        .iter()
        .map(|r| {
            if r.original_index < matched.len() {
                Ok(query::MentionId::Member(
                    matched[r.original_index].id.to_owned(),
                ))
            } else if r.original_index < matched.len() + matched_groups.len() {
                matched_groups
                    .get(r.original_index - matched.len())
                    .map(|g| query::MentionId::Group(g.id.to_owned()))
                    .ok_or_else(unavailable)
            } else {
                matched_files
                    .get(r.original_index - matched.len() - matched_groups.len())
                    .map(|f| query::MentionId::File(f.id.to_owned()))
                    .ok_or_else(unavailable)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut file_choices = BTreeMap::new();
    for id in &ids {
        if let query::MentionId::File(id) = id {
            let choice = file_choice::capture(
                &state.peek(),
                &search,
                batch.names.clone(),
                batch.files.clone().ok_or_else(unavailable)?,
                id,
            )?;
            file_choices.insert(id.clone(), Arc::new(choice));
        }
    }
    let (owner, retained) = (search.clone(), batch.clone());
    let witness = Arc::new(move |shell: &ShellState| retained.current(shell, &owner));
    let presentation = query::publish_all_checked(
        &mut state.write(),
        &search.lease,
        batch.source.clone(),
        batch.groups.clone(),
        &ids,
        renderer.group_labels().unwrap_or(&BTreeMap::new()),
        file_choices,
        Some(search.clone()),
        witness,
    )
    .ok_or_else(unavailable)?;
    on_results(presentation);
    Ok(())
}
