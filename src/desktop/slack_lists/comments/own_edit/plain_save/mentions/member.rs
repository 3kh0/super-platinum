//! Atomic local member/group search retained by the original plain Save.
use super::*;
use crate::{
    directory_member_fetches::search as source,
    profile_edit::sections::rich::search::{
        MemberStrings, SaveGroupSource, SaveMemberNames, SaveMemberRank,
    },
};
use super_platinum_core::{
    composer_emoji::history::History,
    slack_lists::comments::edit_seed::{local_search::member as local, prepare::MentionLookup},
};

#[derive(Default)]
pub(super) struct Facts {
    source: Option<Arc<source::Snapshot>>,
    groups: Option<Arc<SaveGroupSource>>,
    names: Option<Arc<SaveMemberNames>>,
    eligibility: Option<Arc<member_eligibility::Observation>>,
    ranking: Vec<Arc<SaveMemberRank>>,
}
impl Facts {
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.source.as_ref().is_none_or(|v| v.current(shell))
            && self.groups.as_ref().is_none_or(|v| v.current(shell))
            && self.names.as_ref().is_none_or(|v| v.current(shell))
            && self.ranking.iter().all(|v| v.current(shell))
    }
}

pub(super) fn history(proof: &Value) -> Result<History, Error> {
    super_platinum_core::block_actions::name_policy::UserPrefs::decode(&proof["userPreferences"])
        .map_err(|_| missing("member_history"))?;
    match proof["userPreferences"]["prefs"].get("frecency") {
        Some(value) => History::from_pref(value).map_err(|_| missing("member_history")),
        None => Ok(History::default()), // absent only in a complete fresh response
    }
}

fn matching<'a>(
    plan: &local::Plan,
    strings: &'a MemberStrings,
    groups: &'a SaveGroupSource,
    eligibility: &member_eligibility::Observation,
) -> Result<local::Matched<'a>, local::Error> {
    let members = local::Members::capture(strings.original_rows(), strings.rows())?;
    plan.matching(
        &local::Query {
            formatted: plan.formatted_query(),
            prepared: &strings.query,
            maybe_self: Some(strings.maybe_self),
            beyond_boundary_eligible: strings.beyond_boundary_eligible,
        },
        &members,
        groups.rows(),
        |r| eligibility.member(r),
        |g| eligibility.group(g),
        |g| strings.group_names(g),
    )
}

pub(super) async fn search(
    state: Signal<ShellState>,
    facts: &mut super::super::Facts,
    policy: &own_edit_policy::Observation,
    proof: &Value,
    lookup: MentionLookup,
    observed: &mut MentionObservations,
) -> Result<(), Error> {
    if lookup.query.is_empty() || !current(state, facts, policy) {
        return Err(missing("member_owner"));
    }
    if facts.mentions.members.source.is_none() {
        // Both categories must be current in this same serial read. Neither is
        // initialized or repaired merely to make an unavailable search empty.
        let shell = state.peek();
        let team = &facts.context.session.team_id;
        let source = Arc::new(source::snapshot(&shell, team, "list-plain-save-members")?);
        let groups = Arc::new(SaveGroupSource::capture(&shell, team)?);
        facts.mentions.members.source = Some(source);
        facts.mentions.members.groups = Some(groups);
    }
    let source = facts
        .mentions
        .members
        .source
        .clone()
        .ok_or_else(|| missing("members"))?;
    let groups = facts
        .mentions
        .members
        .groups
        .clone()
        .ok_or_else(|| missing("member_groups"))?;
    let beyond = beyond_boundaries(&|| current(state, facts, policy)).await?;
    let flags = member_eligibility::flags(&proof["assignments"], beyond)?;
    let plan = local::Plan::capture(&lookup, flags)
        .map_err(|_| missing("member_plan"))?
        .ok_or_else(|| missing("member_plan"))?;
    if facts.mentions.members.eligibility.is_none() {
        let transport = facts.context.transport.clone();
        let live = || current(state, facts, policy);
        let eligibility = member_eligibility::acquire(
            &facts.context,
            &source,
            flags,
            &proof["assignments"],
            &live,
            |request| {
                let transport = transport.clone();
                let live = &live;
                async move {
                    if crate::fixture::is_fixture() {
                        fixture::plain_member_agents::request(state, &request).await
                    } else {
                        transport.execute_current(request, live).await
                    }
                }
            },
        )
        .await?;
        if !current(state, facts, policy) {
            return Err(missing("member_owner"));
        }
        facts.mentions.members.eligibility = Some(Arc::new(eligibility));
    }
    let eligibility = facts
        .mentions
        .members
        .eligibility
        .clone()
        .ok_or_else(|| missing("member_eligibility"))?;
    if facts.mentions.members.names.is_none() {
        let transport = facts.context.transport.clone();
        let names = SaveMemberNames::capture(
            state,
            policy.authority(),
            proof,
            |shell| facts.current(shell) && policy.current(shell),
            |request| {
                let transport = transport.clone();
                async move {
                    if crate::fixture::is_fixture() {
                        if !fixture::plain_members::named() {
                            return Err(missing("closed_member_reader"));
                        }
                        fixture::completion::request(state, &request)
                            .await
                            .map(|(raw, _)| raw)
                    } else {
                        transport.execute(request).await
                    }
                }
            },
        )
        .await?;
        facts.mentions.members.names = Some(Arc::new(names));
    }
    let names = facts
        .mentions
        .members
        .names
        .clone()
        .ok_or_else(|| missing("member_policy"))?;
    let rows = source
        .rows()
        .iter()
        .map(|r| r.source.clone())
        .collect::<Vec<_>>();
    let strings = Arc::new(
        MemberStrings::capture(
            &lookup.query,
            plan.formatted_query(),
            &rows,
            &names.gantry["self"],
            groups.rows(),
            &names.policy,
            &|| current(state, facts, policy),
        )
        .await?,
    );
    if strings.beyond_boundaries != beyond || !current(state, facts, policy) {
        return Err(missing("member_options"));
    }
    let (s, g, e, l) = (
        strings.clone(),
        groups.clone(),
        eligibility.clone(),
        lookup.clone(),
    );
    let matched_rows = tokio::task::spawn_blocking(move || {
        let plan = local::Plan::capture(&l, flags)?.ok_or(local::Error::InvalidInput)?;
        matching(&plan, &s, &g, &e).map(|m| m.members().cloned().collect::<Vec<_>>())
    })
    .await
    .map_err(|_| missing("worker"))?
    .map_err(|_| missing("member_matching"))?;
    if !current(state, facts, policy) {
        return Err(missing("owner"));
    }
    let ranking = Arc::new(
        SaveMemberRank::capture(
            state,
            names.clone(),
            source.clone(),
            groups.clone(),
            &matched_rows,
            proof,
            |s| strings.ordinary_lowercase(s),
            |shell| facts.current(shell) && policy.current(shell),
        )
        .await?,
    );
    let (s, g, e, l, r) = (
        strings.clone(),
        groups.clone(),
        eligibility.clone(),
        lookup.clone(),
        ranking.clone(),
    );
    let result = tokio::task::spawn_blocking(move || {
        let plan = local::Plan::capture(&l, flags)?.ok_or(local::Error::InvalidInput)?;
        let matched = matching(&plan, &s, &g, &e)?;
        if matched.members().ne(matched_rows.iter()) {
            return Err(local::Error::InvalidInput);
        }
        let normalize = |text: &str| s.normalize(text);
        let context = local::Context {
            state: &r.selectors,
            history: &r.history,
            model: r.cache.model(),
            now_ms: r.now_ms,
            normalize: &normalize,
        };
        let mut cache = r
            .cache
            .similarity()
            .lock()
            .map_err(|_| local::Error::InvalidInput)?;
        plan.rank(
            &matched,
            &context,
            &mut cache,
            |row| s.comparison(row),
            |a, b| s.compare(a, b),
            s.demote_passthrough(),
        )
    })
    .await
    .map_err(|_| missing("worker"))?
    .map_err(|_| missing("member_ranking"))?;
    if !current(state, facts, policy) || !ranking.current(&state.peek()) {
        return Err(missing("owner"));
    }
    if fixture::plain_members::named() {
        fixture::plain_members::observe(json!({"lookup":{"kind":"member","query":lookup.query},
            "requested":plan.requested(),"source":rows,
            "groups":groups.rows().iter().map(|g| json!({"source":g.source(),"effective":g.effective()})).collect::<Vec<_>>(),
            "options":strings.snapshot(),"eligibility":eligibility.snapshot(),"namePolicy":{"gantry":names.gantry,"enterprise":names.enterprise,"defaultWorkspace":names.default_workspace},
            "selectors":ranking.selectors,"results":result.search.rows,
            "sourceCurrent":source.current(&state.peek()),"groupsCurrent":groups.current(&state.peek()),"rankCurrent":ranking.current(&state.peek())}));
    }
    facts.mentions.members.ranking.push(ranking);
    observed.searches.push(result.search);
    Ok(())
}
