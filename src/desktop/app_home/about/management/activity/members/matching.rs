//! Matching/ranking of a completed shared source projection. The caller must
//! retain the shared Ready receipt and recheck it before presenting/selecting.
use super::*;
mod seed;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::block_actions::workflow_management::{
    activity_members::{
        self as core, Eligibility, MatchFacts, Member, ObservationPlan, RankInputs,
    },
    member_hydration::Current,
};

pub(super) struct Row {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) raw: Value,
}
pub(super) struct Ranked {
    pub(super) rows: Vec<Row>,
    source: Vec<Current>,
    candidates: Vec<Value>,
    agents: agents::Acquired,
    ranking: ranking::Input,
    seed: seed::Seed,
    valid: bool,
}
impl Ranked {
    /// Required in addition to the caller's actual Ready.current check. A raw
    /// row/list equality alone cannot grant an ordered-store authority stamp.
    pub(super) fn current(&self, s: &ShellState, q: &Query, source: &[Current]) -> bool {
        self.valid
            && self.source == source
            && q.current(s)
            && self.agents.current(s, q, &self.candidates)
            && self.ranking.current(s, q, &self.candidates)
    }
    pub(super) fn refresh_drafts(
        &mut self,
        s: &ShellState,
        q: &Query,
        source: &[Current],
    ) -> Result<(), Error> {
        self.valid = false;
        if self.source != source || !self.agents.current(s, q, &self.candidates) {
            return Err(unavailable());
        }
        self.ranking.refresh_drafts(s, q, &self.candidates)?;
        self.rows = self.seed.rank(q, &self.ranking)?;
        self.valid = true;
        Ok(())
    }
}

pub(super) async fn capture(
    state: Signal<ShellState>,
    q: &Query,
    source: &[Current],
    remote: &BTreeSet<String>,
    names: &names::Observation,
    fresh: impl Fn(&ShellState) -> bool,
) -> Result<Ranked, Error> {
    if !fresh(&state.peek()) || !names.current(&state.peek(), q) {
        return Err(unavailable());
    }
    // th9D false is independent from profile-only/agent eligibility. Unknown
    // interaction is not fabricated as false or as a remote response receipt.
    let mut ids = BTreeSet::new();
    let mut members = Vec::new();
    let mut eligible = BTreeSet::new();
    for row in source {
        let member = Member::decode(&row.source)?;
        if !ids.insert(member.user().id.clone()) {
            return Err(unavailable());
        }
        if row.interaction == Some(false) {
            continue;
        }
        if member.eligibility(&q.owner.ctx.session.user_id, Some(true), None, None)
            == Eligibility::Denied
        {
            continue;
        }
        eligible.insert(member.user().id.clone());
        members.push(row.source.clone());
    }
    let normalized = render::normalize(state, q, BTreeSet::new()).await?;
    if !fresh(&state.peek()) || !names.current(&state.peek(), q) {
        return Err(unavailable());
    }
    let passthrough = if remote.is_empty() {
        false
    } else {
        let ctx = &q.owner.ctx;
        let raw = read::request(
            state,
            q,
            super_platinum_core::slack::api::list_experiments(&ctx.client, &ctx.session),
        )
        .await?;
        assignment(&raw, "autocomplete_flannel_fuzzy_match_passthrough")? == Some("treatment")
    };
    // AjOk supplies no matchBeyondWordBoundaries override; retained fRHe also
    // omits it. hHTo therefore does not construct that optional matcher.
    let facts = MatchFacts {
        query_is_maybe_self: matches!(normalized.query.normalized.as_str(), "me" | "you"),
        can_match_beyond_word_boundaries: false,
        passthrough_remote_fuzzy_filtering: passthrough,
    };
    let matched = core::match_members(
        q.member(),
        q.path(),
        &normalized.query,
        &members,
        remote,
        &facts,
        |row| row["id"].as_str().is_some_and(|id| eligible.contains(id)),
    );
    // hHTo.filterMembers rejects RK50.z1 unknown/nonexistent records before
    // empty-query or remote-fuzzy admission. The shared matcher owns this;
    // the source store must retain terminal placeholders in their original slots.
    let candidates = matched
        .iter()
        .map(|row| row.item.clone())
        .collect::<Vec<_>>();
    if !fresh(&state.peek()) {
        return Err(unavailable());
    }
    // Profile/list reads happen only for actual local/fuzzy matches, before
    // frecency sorting. Matching itself never admits an unknown agent.
    let agents = agents::acquire(state, q, &candidates).await?;
    if !fresh(&state.peek()) || !names.current(&state.peek(), q) {
        return Err(unavailable());
    }
    let matched = matched
        .into_iter()
        .filter(|row| agents.allowed(q, row.item))
        .collect::<Vec<_>>();
    let labels = matched
        .iter()
        .map(|row| {
            names
                .policy
                .member_name(row.item, "Name hidden")
                .map_err(|_| unavailable())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut rank = ranking::capture(state, q, names, &candidates).await?;
    let plan = ObservationPlan::capture(&q.generation(), q.member(), q.path(), &matched, &labels)
        .map_err(|_| unavailable())?;
    let observations = render::rank(state, q, &normalized, plan).await?;
    // A lazy draft sibling may publish during the renderer await. Adopt only
    // those bits using this same retained seed; no new request is needed.
    rank.refresh_drafts(&state.peek(), q, &candidates)?;
    if !fresh(&state.peek())
        || !rank.current(&state.peek(), q, &candidates)
        || !agents.current(&state.peek(), q, &candidates)
    {
        return Err(unavailable());
    }
    let seed = seed::Seed::capture(&matched, labels, observations);
    let rows = seed.rank(q, &rank)?;
    let result = Ranked {
        rows,
        source: source.to_vec(),
        candidates,
        agents,
        ranking: rank,
        seed,
        valid: true,
    };
    if !fresh(&state.peek()) || !result.current(&state.peek(), q, source) {
        return Err(unavailable());
    }
    Ok(result)
}

#[cfg(test)]
mod placeholder_tests {
    use super::*;
    use super_platinum_core::block_actions::home::workflow::directory_local;

    #[test]
    fn terminal_placeholders_cannot_enter_empty_or_remote_fuzzy_results() {
        let (s, _) = super::super::tests::setup();
        let mut owner = s
            .app_home
            .about
            .management
            .surface
            .as_ref()
            .unwrap()
            .owner
            .ctx
            .session
            .clone();
        owner.user_id = "U00000999".into();
        let rows = vec![
            serde_json::json!({"id":"U00000001","_real_name_normalized_lc":"alice"}),
            serde_json::json!({"id":"U00000002","name":"U00000002","isUnknown":true,"profile":{}}),
            serde_json::json!({"id":"U00000003","name":"","isNonExistent":true,"profile":{}}),
        ];
        let before = rows.clone();
        let remote = rows
            .iter()
            .map(|row| row["id"].as_str().unwrap().to_owned())
            .collect();
        for raw in ["", "unrelated"] {
            let query = core::Query::capture(&owner, raw).unwrap();
            let prepared = directory_local::prepare_query(raw, str::to_lowercase);
            let matched = core::match_members(
                &query,
                core::SearchPath::Remote,
                &prepared,
                &rows,
                &remote,
                &MatchFacts {
                    query_is_maybe_self: false,
                    can_match_beyond_word_boundaries: false,
                    passthrough_remote_fuzzy_filtering: true,
                },
                |_| true,
            );
            assert_eq!(
                matched.iter().map(|row| row.id).collect::<Vec<_>>(),
                vec!["U00000001"]
            );
        }
        assert_eq!(rows, before);
    }
}

fn assignment<'a>(raw: &'a Value, key: &str) -> Result<Option<&'a str>, Error> {
    if raw["ok"] != true || raw.get("error").is_some_and(|e| !e.is_null()) {
        return Err(unavailable());
    }
    raw["assignments"]
        .as_object()
        .ok_or_else(unavailable)?
        .get(key)
        .map(|v| v["group"].as_str().ok_or_else(unavailable))
        .transpose()
}
