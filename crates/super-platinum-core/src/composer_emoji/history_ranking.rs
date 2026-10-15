//! Stateful-history counterpart of ranking::rank_without_personal_history.
//! All data is captured by the caller; this module neither fetches nor invents it.
use super::history::{History, Stats};
use super::model::{Metadata, Model, features};
use super::ranking::{Candidate, Ranked, SimilarityCache};
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RankError {
    InvalidClock,
    NonFiniteNameScore,
    NonFiniteShortlist,
    NonFiniteModel,
}

impl std::fmt::Display for RankError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidClock => "ranking clock unavailable",
            Self::NonFiniteNameScore => "non-finite emoji name score",
            Self::NonFiniteShortlist => "non-finite history cannot define a stable shortlist",
            Self::NonFiniteModel => "non-finite emoji model score",
        })
    }
}

impl std::error::Error for RankError {}

pub struct Context<'a> {
    pub history: &'a History,
    pub now_ms: f64,
    pub model: &'a Model,
    pub metadata: &'a Metadata,
    pub source: &'a str,
    pub exact_boost: bool,
}

#[derive(Clone, Debug)]
pub struct Scored {
    pub candidate: Candidate,
    pub stats: Stats,
    pub bonus: f64,
    pub fuzzy_bonus: Option<f64>,
}

/// Sort and score every candidate before applying the shortlist. In particular,
/// hit/item normalization uses the complete matching set, never the final 25.
pub fn prepare(
    query: &str,
    mut candidates: Vec<Candidate>,
    limit: usize,
    context: &Context<'_>,
    cache: &mut SimilarityCache,
    compare_names: impl Fn(&str, &str) -> Ordering,
) -> Result<Vec<Scored>, RankError> {
    if !context.now_ms.is_finite() {
        return Err(RankError::InvalidClock);
    }
    if candidates
        .iter()
        .any(|c| c.name_score.is_some_and(|s| !s.is_finite()))
    {
        return Err(RankError::NonFiniteNameScore);
    }
    let shortlist = limit.saturating_mul(3).min(500);
    let model_only = candidates.len() < shortlist;
    candidates.sort_by(|a, b| match (a.name_score, b.name_score) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a_score), Some(b_score)) => a_score
            .partial_cmp(&b_score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| compare_names(&a.name, &b.name)),
    });
    let (mut ordered, rest): (Vec<_>, Vec<_>) = candidates.into_iter().partition(|c| c.exact);
    ordered.extend(rest);
    let ids: Vec<_> = ordered.iter().map(|c| c.id.as_str()).collect();
    let stats = context.history.score(query, &ids, context.now_ms);
    let mut scored: Vec<_> = ordered
        .into_iter()
        .zip(stats)
        .map(|(candidate, stats)| {
            let mut fuzzy_bonus = None;
            let mut bonus = if candidate.exact {
                100.0 + if context.exact_boost { 300.0 } else { 100.0 }
            } else {
                let finite_name = candidate
                    .name_score
                    .is_some_and(|s| s != 0.0 && s.is_finite());
                let value = cache.similarity(query, &candidate.name)
                    * if finite_name { 150.0 } else { 50.0 };
                if !finite_name {
                    fuzzy_bonus = Some(value);
                }
                value
            };
            if candidate.is_emoji && matches!(candidate.name.as_str(), "thumbsup" | "point_up") {
                bonus += 1.0;
            }
            Scored {
                candidate,
                stats,
                bonus,
                fuzzy_bonus,
            }
        })
        .collect();
    if !model_only {
        // NaN makes the JS comparator non-total. Different sort engines cannot
        // promise the same order, and Rust's sort may panic on inconsistency.
        // Refuse that snapshot before sorting; do not invent a fallback order.
        if scored.iter().any(|r| !(r.stats.raw + r.bonus).is_finite()) {
            return Err(RankError::NonFiniteShortlist);
        }
        scored.sort_by(|a, b| {
            (b.stats.raw + b.bonus)
                .partial_cmp(&(a.stats.raw + a.bonus))
                .unwrap_or(Ordering::Equal)
        });
    }
    Ok(scored)
}

pub fn rank(
    query: &str,
    candidates: Vec<Candidate>,
    limit: usize,
    context: &Context<'_>,
    cache: &mut SimilarityCache,
    compare_names: impl Fn(&str, &str) -> Ordering,
) -> Result<Vec<Ranked>, RankError> {
    let mut prepared = prepare(query, candidates, limit, context, cache, compare_names)?;
    prepared.truncate(limit.saturating_mul(3).min(500));
    let count = prepared.len();
    let mut scored: Vec<_> = prepared
        .into_iter()
        .map(|row| {
            let vector = features(query, &row, count, context.now_ms, context.metadata);
            Ranked {
                model_score: context.model.score(&vector, context.source),
                bonus: row.bonus,
                candidate: row.candidate,
            }
        })
        .collect();
    // Finite weights can still overflow during addition/multiplication.
    if scored.iter().any(|r| !r.model_score.is_finite()) {
        return Err(RankError::NonFiniteModel);
    }
    scored.sort_by(|a, b| {
        b.model_score
            .partial_cmp(&a.model_score)
            .unwrap_or(Ordering::Equal)
    });
    let tail = scored.split_off(scored.len().min(2));
    let (mut exact, rest): (Vec<_>, Vec<_>) = tail.into_iter().partition(|r| r.candidate.exact);
    exact.extend(rest);
    scored.extend(exact);
    scored.truncate(limit);
    Ok(scored)
}
