//! Owned original matcher values permit draft-only reranking without renewed
//! reads, changed assignments, new renderer observations or recomputed matches.
use super::core::RankObservations;
use super::*;
use super_platinum_core::block_actions::home::workflow::directory_local::Scored;

struct Match {
    item: Value,
    id: String,
    score: u32,
    details: Option<BTreeMap<&'static str, u32>>,
    exact: Option<bool>,
    exact_details: Option<BTreeMap<&'static str, bool>>,
    self_member: bool,
    passthrough: bool,
}
pub(super) struct Seed {
    matched: Vec<Match>,
    labels: Vec<String>,
    observations: RankObservations,
}
impl Seed {
    pub(super) fn capture(
        matched: &[Scored<'_>],
        labels: Vec<String>,
        observations: RankObservations,
    ) -> Self {
        Self {
            matched: matched
                .iter()
                .map(|m| Match {
                    item: m.item.clone(),
                    id: m.id.into(),
                    score: m.score,
                    details: m.score_details.clone(),
                    exact: m.is_exact_match,
                    exact_details: m.exact_match_details.clone(),
                    self_member: m.is_self,
                    passthrough: m.is_flannel_fuzzy_passthrough,
                })
                .collect(),
            labels,
            observations,
        }
    }
    pub(super) fn rank(&self, q: &Query, input: &ranking::Input) -> Result<Vec<Row>, Error> {
        let matched = self
            .matched
            .iter()
            .map(|m| Scored {
                item: &m.item,
                id: &m.id,
                score: m.score,
                score_details: m.details.clone(),
                is_exact_match: m.exact,
                exact_match_details: m.exact_details.clone(),
                is_self: m.self_member,
                is_flannel_fuzzy_passthrough: m.passthrough,
            })
            .collect::<Vec<_>>();
        let mut similarity = input.cache.similarity.lock().map_err(|_| unavailable())?;
        core::rank_members(
            q.member(),
            q.path(),
            &matched,
            &self.labels,
            &RankInputs {
                state: &input.selectors,
                history: &input.history,
                model: &input.cache.model,
                now_ms: input.now_ms,
            },
            &self.observations,
            &mut similarity,
        )
        .map_err(|_| unavailable())?
        .into_iter()
        .map(|index| {
            let m = self.matched.get(index).ok_or_else(unavailable)?;
            Ok(Row {
                id: m.id.clone(),
                label: self.labels.get(index).ok_or_else(unavailable)?.clone(),
                raw: m.item.clone(),
            })
        })
        .collect()
    }
}
