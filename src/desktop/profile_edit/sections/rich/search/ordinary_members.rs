//! List members, permitted groups and gated files share one rank pass and owner.
use super::*;
use crate::directory_member_fetches::search as intake;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::slack::api::custom::texty::{
    file_matching, groups, member_matching, member_ranking,
};

mod run;
pub(crate) use run::run_list_members;

#[derive(Clone)]
pub(crate) struct MemberBatch {
    values: Vec<(Value, String)>,
    witness: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>,
    // Passive capture diagnostic; local/interim choices retain normal authority.
    pub(crate) settled: bool,
    diagnostic: Arc<dyn Fn(&ShellState) -> Value + Send + Sync>,
    canvas_fallback: Arc<dyn Fn(&ShellState) -> Value + Send + Sync>,
}
impl MemberBatch {
    pub(crate) fn title_users_proof(&self, s: &ShellState) -> Value {
        (self.diagnostic)(s)
    }
    pub(crate) fn canvas_fallback_proof(&self, s: &ShellState) -> Value {
        (self.canvas_fallback)(s)
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        (self.witness)(shell)
    }
    pub(crate) fn choices(&self, shell: &ShellState) -> Option<Vec<(Value, String)>> {
        self.current(shell).then(|| self.values.clone())
    }
}
#[derive(Clone)]
struct Batch {
    source: Arc<intake::Snapshot>,
    groups: Option<Arc<group_source::Snapshot>>,
    group_policy: Arc<group_policy::Observation>,
    file_policy: Arc<file_policy::Observation>,
    files: Option<Arc<file_source::Snapshot>>,
    title_users: Option<Arc<file_choice::users::Users>>,
    names: Arc<member_names::Observation>,
    preferences: Arc<ranking::Preferences>,
    renderer: Arc<member_render::Observed>,
    policy: Arc<member_policy::Observation>,
    input: member_rank_input::Input,
    context: Value,
    remote_ids: BTreeSet<String>,
    labels: BTreeMap<String, String>,
}
impl Batch {
    fn current(&self, shell: &ShellState, search: &Search<impl Host>) -> bool {
        search.current(shell)
            && self.source.current(shell)
            && self
                .groups
                .as_ref()
                .is_none_or(|source| source.current(shell))
            && self.group_policy.current(shell, search)
            && self.file_policy.current(shell, search)
            && self
                .files
                .as_ref()
                .is_none_or(|source| source.current(shell))
            && self.title_users.as_ref().is_none_or(|u| u.current(shell))
            && self.renderer.current(search, shell)
            && self.policy.current(shell, search)
            && self.input.current(shell, search)
            && search
                .lease
                .member_context(shell)
                .is_ok_and(|v| v.as_ref() == Some(&self.context))
    }
}

fn source_retired() -> Error {
    Error::Api("list_completion_source_retired".into())
}
fn local_groups(
    shell: &ShellState,
    search: &Search<impl Host>,
    policy: &group_policy::Observation,
) -> Option<Arc<group_source::Snapshot>> {
    policy
        .policy()
        .enabled()
        .then(|| {
            group_source::Snapshot::capture(shell, search.lease.team())
                .ok()
                .map(Arc::new)
        })
        .flatten()
}
async fn capture<L: Host>(
    state: Signal<ShellState>,
    search: &Search<L>,
    source: Arc<intake::Snapshot>,
    groups: Option<Arc<group_source::Snapshot>>,
    group_policy: Arc<group_policy::Observation>,
    file_policy: Arc<file_policy::Observation>,
    names: Arc<member_names::Observation>,
    preferences: Arc<ranking::Preferences>,
    query: &str,
    remote_ids: BTreeSet<String>,
) -> Result<Arc<Batch>, Error> {
    let files = if file_policy.policy().enabled() {
        let source = file_source::Snapshot::capture(&state.peek(), search.lease.team())?;
        if !source.unavailable_ids().is_empty() {
            return Err(unavailable());
        }
        Some(source)
    } else {
        None
    };
    capture_bound(
        state,
        search,
        source,
        groups,
        group_policy,
        file_policy,
        names,
        preferences,
        query,
        remote_ids,
        files,
    )
    .await
}

async fn capture_bound<L: Host>(
    state: Signal<ShellState>,
    search: &Search<L>,
    source: Arc<intake::Snapshot>,
    groups: Option<Arc<group_source::Snapshot>>,
    group_policy: Arc<group_policy::Observation>,
    file_policy: Arc<file_policy::Observation>,
    names: Arc<member_names::Observation>,
    preferences: Arc<ranking::Preferences>,
    query: &str,
    remote_ids: BTreeSet<String>,
    files: Option<Arc<file_source::Snapshot>>,
) -> Result<Arc<Batch>, Error> {
    if !search.current(&state.peek())
        || !names.current(&state.peek(), search)
        || !source.current(&state.peek())
        || files.as_ref().is_some_and(|f| !f.current(&state.peek()))
    {
        return Err(unavailable());
    }
    let context = search
        .lease
        .member_context(&state.peek())?
        .ok_or_else(unavailable)?;
    let rows = source
        .rows()
        .iter()
        .map(|r| r.source.clone())
        .collect::<Vec<_>>();
    let policy = Arc::new(member_policy::capture(state, search, &rows, preferences.clone()).await?);
    let renderer = Arc::new(
        member_render::capture_with_files(
            state,
            search,
            query,
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
        |t| renderer.ordinary_lowercase(t),
        files.clone(),
    )
    .await?;
    let title_users = if let Some(files) = &files {
        let matched = file_matching::matches(
            &renderer.query,
            files.rows(),
            renderer.query_has_link().map_err(|_| unavailable())?,
            input.file_ignore_fuzzy()?,
            |row| renderer.template_context(row),
            |row| renderer.file_names(row),
        )
        .map_err(|_| unavailable())?;
        let selected = matched
            .iter()
            .map(|row| row.id.to_owned())
            .collect::<Vec<_>>();
        let (next, users) =
            file_choice::users::acquire(state, search, &names, files, source.clone(), &selected)
                .await?;
        if !Arc::ptr_eq(&next, &source) {
            // Only the helper's accepted original election advances this source.
            // Rebuild names/rank inputs under the same still-current query and
            // immutable file/policy owners; no remote search is repeated.
            return Box::pin(capture_bound(
                state,
                search,
                next,
                groups,
                group_policy,
                file_policy,
                names,
                preferences,
                query,
                remote_ids,
                Some(files.clone()),
            ))
            .await;
        }
        users
    } else {
        None
    };
    let label_rows = rows
        .iter()
        .filter_map(|row| match policy.allows(row) {
            Ok(true) => Some(Ok(row)),
            Ok(false) => None,
            Err(e) => Some(Err(e)),
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let current = || {
        search.current(&state.peek())
            && source.current(&state.peek())
            && renderer.current(search, &state.peek())
    };
    let mut labels = BTreeMap::new();
    for chunk in label_rows.chunks(128) {
        let input = chunk
            .iter()
            .map(|row| super::super::opening::member_label(row))
            .collect::<Result<Vec<_>, _>>()?;
        let ticket = render::ticket()?;
        let normalized: Value = render::evaluate(
            search,
            &current,
            "listMemberLabels",
            serde_json::json!({"ticket":ticket,"labels":input}),
        )
        .await?;
        if normalized["ticket"] != ticket
            || normalized["labels"]
                .as_array()
                .is_none_or(|v| v.len() != chunk.len())
        {
            return Err(unavailable());
        }
        for (row, label) in chunk.iter().zip(normalized["labels"].as_array().unwrap()) {
            labels.insert(
                row["id"].as_str().ok_or_else(unavailable)?.to_owned(),
                label.as_str().ok_or_else(unavailable)?.to_owned(),
            );
        }
    }
    let batch = Arc::new(Batch {
        source,
        groups,
        group_policy,
        file_policy,
        files,
        title_users,
        names,
        preferences,
        renderer,
        policy,
        input,
        context,
        remote_ids,
        labels,
    });
    if !batch.current(&state.peek(), search) {
        return Err(unavailable());
    }
    Ok(batch)
}
fn publish<L: Host>(
    state: Signal<ShellState>,
    search: Arc<Search<L>>,
    batch: Arc<Batch>,
    settled: bool,
    on_results: &impl Fn(Signal<ShellState>, MemberBatch) -> Result<(), Error>,
) -> Result<(), Error> {
    if !batch.current(&state.peek(), &search) {
        return Err(unavailable());
    }
    let renderer = &batch.renderer;
    let mut eligible = BTreeSet::new();
    for row in batch.source.rows() {
        if batch.policy.allows(&row.source)? {
            eligible.insert(
                row.source["id"]
                    .as_str()
                    .ok_or_else(unavailable)?
                    .to_owned(),
            );
        }
    }
    let matched = member_matching::members(
        &renderer.formatted_query,
        &renderer.query,
        renderer.rows(),
        &batch.remote_ids,
        batch.input.passthrough,
        matches!(renderer.query.normalized.as_str(), "me" | "you"),
        |row| row["id"].as_str().is_some_and(|id| eligible.contains(id)),
    )
    .map_err(|_| unavailable())?;
    let group_rows = batch
        .groups
        .as_ref()
        .map(|source| source.rows())
        .unwrap_or(&[]);
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
    let thread_users = serde_json::from_value::<Vec<String>>(batch.context["threadUsers"].clone())
        .map_err(|_| unavailable())?;
    if batch.context["membershipObserved"] != true {
        return Err(unavailable());
    }
    let observed = batch.context["membership"]
        .as_object()
        .ok_or_else(unavailable)?;
    let membership = matched
        .iter()
        .map(|row| {
            let value = match observed.get(row.id) {
                None => false,
                Some(v) => {
                    v["isKnown"].as_bool().ok_or_else(unavailable)?
                        && v["isMember"].as_bool().ok_or_else(unavailable)?
                }
            };
            Ok((row.id.to_owned(), value))
        })
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
    let channel = batch.context.get("channel").ok_or_else(unavailable)?;
    let normalize = |text: &str| renderer.normalize(text);
    let ranked = {
        let mut cache = search
            .ranking
            .similarity()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        member_ranking::rank_list_mixed_files(
            &renderer.formatted_query,
            &matched,
            &matched_groups,
            &matched_files,
            &member_ranking::ListContext {
                current_channel: (!channel.is_null()).then_some(channel),
                current_thread_users: &thread_users,
                channel_membership: &membership,
            },
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
    let mut file_choices = Vec::new();
    let values = ranked
        .iter()
        .map(|r| {
            if r.original_index < matched.len() {
                let id = matched[r.original_index].id;
                choice(false, id, &batch.labels)
            } else if r.original_index < matched.len() + matched_groups.len() {
                let group = matched_groups
                    .get(r.original_index - matched.len())
                    .ok_or_else(unavailable)?;
                choice(
                    true,
                    group.id,
                    renderer.group_labels().ok_or_else(unavailable)?,
                )
            } else {
                let row = matched_files
                    .get(r.original_index - matched.len() - matched_groups.len())
                    .ok_or_else(unavailable)?;
                let selected = Arc::new(file_choice::capture_with_users(
                    &state.peek(),
                    &search,
                    batch.names.clone(),
                    batch.files.clone().ok_or_else(unavailable)?,
                    row.id,
                    batch.title_users.clone(),
                )?);
                let value = (selected.value().clone(), selected.label().to_owned());
                file_choices.push(selected);
                Ok(value)
            }
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let diagnostics = file_choices.clone();
    let diagnostic = Arc::new(move |s: &ShellState| {
        let values = diagnostics
            .iter()
            .filter_map(|c| {
                c.title_users_proof(s).map(|p| {
                    (
                        c.value()["file_id"].as_str().unwrap_or_default().to_owned(),
                        p,
                    )
                })
            })
            .collect::<serde_json::Map<String, Value>>();
        Value::Object(values)
    });
    let fallback_choices = file_choices.clone();
    let fallback_search = search.clone();
    let canvas_fallback = Arc::new(move |s: &ShellState| {
        Value::Object(
            fallback_choices
                .iter()
                .filter_map(|choice| {
                    choice
                        .canvas_fallback_proof(s, &fallback_search)
                        .map(|proof| {
                            (
                                choice.value()["file_id"]
                                    .as_str()
                                    .unwrap_or_default()
                                    .to_owned(),
                                proof,
                            )
                        })
                })
                .collect(),
        )
    });
    let witness = Arc::new(move |shell: &ShellState| {
        batch.current(shell, &search)
            && file_choices
                .iter()
                .all(|choice| choice.current(shell, &search))
    });
    on_results(
        state,
        MemberBatch {
            values,
            witness,
            settled,
            diagnostic,
            canvas_fallback,
        },
    )
}

// Labels have already crossed the same renderer's NFC observation. Do not
// derive them from token IDs, trim handles, or use existing-token label rules.
fn choice(
    group: bool,
    id: &str,
    labels: &BTreeMap<String, String>,
) -> Result<(Value, String), Error> {
    let value = if group {
        serde_json::json!({"type":"usergroup","usergroup_id":id})
    } else {
        serde_json::json!({"type":"user","user_id":id})
    };
    Ok((value, labels.get(id).cloned().ok_or_else(unavailable)?))
}

#[cfg(test)]
mod tests;
