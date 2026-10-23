//! Reached automatic mention lookup under the original Save, never a popup lease.
use super::*;
mod member;
mod member_eligibility;
use crate::{
    app_home::channel_fetches::canonical,
    profile_edit::sections::rich::search::{ChannelRank, ChannelStrings, beyond_boundaries},
};
use super_platinum_core::{
    slack_lists::comments::edit_seed::{
        local_search,
        prepare::{MentionKind, MentionNeed, MentionObservations},
    },
    state::directory_usergroups::Snapshot,
};

#[derive(Default)]
pub(super) struct Facts {
    channels: Option<Arc<canonical::Canonical>>,
    groups: Option<Snapshot>,
    ranking: Vec<Arc<ChannelRank>>,
    members: member::Facts,
}
impl Facts {
    pub(super) fn current(&self, shell: &ShellState, context: &Context) -> bool {
        self.members.current(shell)
            && self
                .channels
                .as_ref()
                .is_none_or(|c| c.current_for(shell, &context.session.team_id))
            && self.ranking.iter().all(|r| r.current(shell))
            && self.groups.as_ref().is_none_or(|g| {
                shell
                    .core
                    .workspaces
                    .get(&context.session.team_id)
                    .is_some_and(|w| {
                        g.current(
                            &w.directory_usergroups,
                            &context.account,
                            &context.session,
                            context.epoch,
                            &w.usergroups,
                        )
                    })
            })
    }
}

fn current(
    state: Signal<ShellState>,
    facts: &super::Facts,
    policy: &own_edit_policy::Observation,
) -> bool {
    let shell = state.peek();
    facts.current(&shell) && policy.current(&shell)
}

pub(super) async fn fill(
    state: Signal<ShellState>,
    facts: &mut super::Facts,
    policy: &own_edit_policy::Observation,
    proof: &Value,
    need: MentionNeed,
    observed: &mut MentionObservations,
) -> Result<(), Error> {
    if !current(state, facts, policy) {
        return Err(missing("owner"));
    }
    match need {
        MentionNeed::Search(lookup)
            if lookup.kind == MentionKind::Member
                && !observed.searches.iter().any(|s| s.lookup == lookup) =>
        {
            member::search(state, facts, policy, proof, lookup, observed).await?;
        }
        MentionNeed::Names(raw) if !observed.names.iter().any(|n| n.raw == raw) => {
            observed
                .names
                .push(local_search::member::names(&raw).map_err(|_| missing("member_names"))?);
        }
        MentionNeed::Normalization(text) if !observed.normalizations.contains_key(&text) => {
            let normalized =
                crate::profile_edit::sections::rich::search::normalize_member(&text, &|| {
                    current(state, facts, policy)
                })
                .await?;
            observed.normalizations.insert(text, normalized);
        }
        MentionNeed::History if observed.history.is_none() => {
            observed.history = Some(member::history(proof)?);
        }
        MentionNeed::HistoryQuery(text) if !observed.history_queries.contains_key(&text) => {
            let normalized =
                crate::profile_edit::sections::rich::search::history_query(&text, &|| {
                    current(state, facts, policy)
                })
                .await?;
            observed.history_queries.insert(text, normalized);
        }
        MentionNeed::Clock if observed.now_ms.is_none() => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| missing("clock"))?
                .as_secs_f64()
                * 1000.0;
            if !now.is_finite() {
                return Err(missing("clock"));
            }
            observed.now_ms = Some(now);
        }
        MentionNeed::Highlights if observed.highlight_words.is_none() => {
            let shell = state.peek();
            let workspace = shell
                .core
                .workspaces
                .get(&facts.context.session.team_id)
                .ok_or_else(|| missing("owner"))?;
            let groups = plain_observations::group_snapshot(
                &mut facts.mentions.groups,
                workspace,
                &facts.context,
            )?;
            observed.highlight_words = Some(plain_observations::highlights(
                &proof["userPreferences"],
                &facts.context.session.user_id,
                groups
                    .membership()
                    .ok_or_else(|| missing("highlight_membership"))?,
            )?);
        }
        MentionNeed::Search(lookup)
            if lookup.kind == MentionKind::Channel
                && !observed.searches.iter().any(|s| s.lookup == lookup) =>
        {
            if lookup.query.is_empty() {
                return Err(missing("empty_channel_lookup"));
            }
            if facts.mentions.channels.is_none() {
                facts.mentions.channels = Some(canonical::observed(
                    &state.peek(),
                    &facts.context.session.team_id,
                )?);
            }
            let source = facts.mentions.channels.clone().unwrap();
            let beyond = beyond_boundaries(&|| current(state, facts, policy)).await?;
            let plan = local_search::Plan::capture(&lookup, Some(beyond))
                .map_err(|_| missing("channel_plan"))?
                .ok_or_else(|| missing("channel_plan"))?;
            let rows = plan
                .name_rows(source.rows())
                .map_err(|_| missing("channel_names"))?
                .into_iter()
                .cloned()
                .collect::<Vec<_>>();
            let strings = Arc::new(
                ChannelStrings::capture(&lookup.query, plan.formatted_query(), &rows, &|| {
                    current(state, facts, policy)
                })
                .await?,
            );
            if strings.beyond_boundaries != beyond || !current(state, facts, policy) {
                return Err(missing("channel_options"));
            }
            // Resolve matching on a worker first so ranking selectors are read
            // only for actually matched rows, not unrelated canonical channels.
            let (worker_source, worker_strings, worker_lookup) =
                (source.clone(), strings.clone(), lookup.clone());
            let matched_rows = tokio::task::spawn_blocking(move || {
                let plan = local_search::Plan::capture(&worker_lookup, Some(beyond))?
                    .ok_or(local_search::Error::InvalidInput)?;
                let query = local_search::Query {
                    formatted: plan.formatted_query(),
                    prepared: &worker_strings.query,
                    beyond_boundary_eligible: worker_strings.beyond_boundary_eligible,
                };
                plan.matching(&query, worker_source.rows(), |row| worker_strings.name(row))
                    .map(|rows| rows.iter().map(|row| row.raw().clone()).collect::<Vec<_>>())
            })
            .await
            .map_err(|_| missing("worker"))?
            .map_err(|_| missing("channel_matching"))?;
            if !current(state, facts, policy) {
                return Err(missing("owner"));
            }
            let ranking = Arc::new(
                ChannelRank::capture(state, source.clone(), &matched_rows, proof, |shell| {
                    facts.current(shell) && policy.current(shell)
                })
                .await?,
            );
            let (worker_source, worker_strings, worker_lookup, worker_rank) = (
                source.clone(),
                strings.clone(),
                lookup.clone(),
                ranking.clone(),
            );
            let result = tokio::task::spawn_blocking(move || {
                let plan = local_search::Plan::capture(&worker_lookup, Some(beyond))?
                    .ok_or(local_search::Error::InvalidInput)?;
                let query = local_search::Query {
                    formatted: plan.formatted_query(),
                    prepared: &worker_strings.query,
                    beyond_boundary_eligible: worker_strings.beyond_boundary_eligible,
                };
                let matched =
                    plan.matching(&query, worker_source.rows(), |row| worker_strings.name(row))?;
                if matched.iter().map(|m| m.raw()).ne(matched_rows.iter()) {
                    return Err(local_search::Error::InvalidInput);
                }
                let normalize = |value: &str| worker_strings.normalize(value);
                let context = local_search::Context {
                    state: &worker_rank.selectors,
                    history: &worker_rank.history,
                    model: worker_rank.cache.model(),
                    now_ms: worker_rank.now_ms,
                    normalize: &normalize,
                };
                let mut cache = worker_rank
                    .cache
                    .similarity()
                    .lock()
                    .map_err(|_| local_search::Error::InvalidInput)?;
                plan.rank(&matched, &context, &mut cache, |a, b| {
                    worker_strings.compare(a, b)
                })
            })
            .await
            .map_err(|_| missing("worker"))?
            .map_err(|_| missing("channel_ranking"))?;
            if !current(state, facts, policy) || !ranking.current(&state.peek()) {
                return Err(missing("owner"));
            }
            for id in &result.latest_ids {
                canonical::hit(state, &source, id)?;
            }
            if !current(state, facts, policy) || !ranking.current(&state.peek()) {
                return Err(missing("owner"));
            }
            if fixture::plain_channels::named() {
                fixture::plain_channels::observe(
                    json!({"lookup":{"kind":"channel","query":lookup.query},
                    "requested":plan.requested(),"source":source.rows(),"options":strings.snapshot(),
                    "selectors":ranking.selectors,"results":result.search.rows,
                    "sourceCurrent":source.current(&state.peek()),"rankCurrent":ranking.current(&state.peek())}),
                );
            }
            facts.mentions.ranking.push(ranking);
            observed.searches.push(result.search);
        }
        _ => return Err(missing("automatic_mention_observation")),
    }
    if !current(state, facts, policy) {
        return Err(missing("owner"));
    }
    Ok(())
}
