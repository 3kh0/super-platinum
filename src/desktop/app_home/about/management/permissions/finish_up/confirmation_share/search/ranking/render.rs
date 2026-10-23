//! Renderer-owned casing and numeric collation for one original Share query.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::{
    block_actions::home::workflow::directory_local::{PreparedQuery, normalize_lowered_query},
    workflow_share::search::ranking::{ObservationPlan, RankObservations, RendererReply},
};

const LOWER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/desktop/app_home/workflows/entity_normalization.js"
));
const COLLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/desktop/app_home/workflows/ranking_renderer.js"
));

pub(super) struct Normalized {
    pub(super) query: PreparedQuery,
    generation: String,
    source: String,
    locale: String,
    names: BTreeMap<String, String>,
}
impl Normalized {
    pub(super) fn locale(&self) -> &str {
        &self.locale
    }
    pub(super) fn name(&self, source: &str) -> Option<&str> {
        self.names.get(source).map(String::as_str)
    }
}
pub(super) fn generation(q: &Query) -> String {
    format!(
        "{}:{}:{}",
        q.generation, q.expected["queryRevision"], q.expected["queryInput"]
    )
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LowerReply {
    generation: String,
    stage: usize,
    locale: String,
    query: String,
    names: Vec<String>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldsReply {
    generation: String,
    stage: usize,
    locale: String,
    fields: Vec<String>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CollationReply {
    generation: String,
    locale: String,
    collation_locale: String,
    classes: Vec<(usize, usize)>,
    demote_passthrough: bool,
}

pub(super) async fn normalize(
    state: Signal<ShellState>,
    q: &Query,
    typed: &str,
    names: BTreeSet<String>,
    guard: &impl Fn(&ShellState) -> bool,
) -> Result<Normalized, Error> {
    let first = lower(state, q, guard, 0, typed, &[]).await?;
    let normalized = normalize_lowered_query(&first.query);
    // jytj runs hNDQ again after the first transliteration/whitespace pass.
    let second = lower(state, q, guard, 1, &normalized, &[]).await?;
    if first.locale != second.locale {
        return Err(unavailable());
    }
    let query = PreparedQuery {
        normalized,
        matcher_normalized: normalize_lowered_query(&second.query),
    };
    let names: Vec<_> = names.into_iter().collect();
    let mut lowered = BTreeMap::new();
    for (index, fields) in names.chunks(128).enumerate() {
        let reply = lower(state, q, guard, index + 2, "", fields).await?;
        if reply.locale != first.locale {
            return Err(unavailable());
        }
        lowered.extend(fields.iter().cloned().zip(reply.names));
    }
    Ok(Normalized {
        query,
        generation: generation(q),
        source: typed.into(),
        locale: first.locale,
        names: lowered,
    })
}

async fn lower(
    state: Signal<ShellState>,
    q: &Query,
    guard: &impl Fn(&ShellState) -> bool,
    stage: usize,
    query: &str,
    names: &[String],
) -> Result<LowerReply, Error> {
    let input = json!({"generation":generation(q),"stage":stage,"query":query,"names":names});
    let reply: LowerReply = native::observe(
        state,
        q,
        guard,
        LOWER,
        &format!("workflowDirectoryLower({input})"),
    )
    .await?;
    if reply.generation != generation(q)
        || reply.stage != stage
        || reply.locale.is_empty()
        || reply.names.len() != names.len()
    {
        return Err(unavailable());
    }
    Ok(reply)
}

/// hNDQ for channel display names, distinct from ordinary member-field casing.
pub(super) async fn fields(
    state: Signal<ShellState>,
    q: &Query,
    normalized: &Normalized,
    names: BTreeSet<String>,
    guard: &impl Fn(&ShellState) -> bool,
) -> Result<BTreeMap<String, String>, Error> {
    if normalized.generation != generation(q) {
        return Err(unavailable());
    }
    let names: Vec<_> = names.into_iter().collect();
    let mut result = BTreeMap::new();
    for (stage, fields) in names.chunks(128).enumerate() {
        let input = json!({"generation":generation(q),"stage":stage,"fields":fields});
        let reply: FieldsReply = native::observe(
            state,
            q,
            guard,
            LOWER,
            &format!("workflowDirectoryFields({input})"),
        )
        .await?;
        if reply.generation != generation(q)
            || reply.stage != stage
            || reply.locale != normalized.locale
            || reply.fields.len() != fields.len()
        {
            return Err(unavailable());
        }
        result.extend(
            fields.iter().cloned().zip(
                reply
                    .fields
                    .into_iter()
                    .map(|v| normalize_lowered_query(&v)),
            ),
        );
    }
    Ok(result)
}

pub(super) async fn rank(
    state: Signal<ShellState>,
    q: &Query,
    typed: &str,
    normalized: &Normalized,
    plan: ObservationPlan,
    guard: &impl Fn(&ShellState) -> bool,
) -> Result<RankObservations, Error> {
    if normalized.generation != generation(q)
        || normalized.source != typed
        || plan.generation() != generation(q)
    {
        return Err(unavailable());
    }
    let mut lowered = Vec::with_capacity(plan.normalization_fields().len());
    for (stage, fields) in plan.normalization_fields().chunks(128).enumerate() {
        let input = json!({"generation":generation(q),"stage":stage,"fields":fields});
        let reply: FieldsReply = native::observe(
            state,
            q,
            guard,
            LOWER,
            &format!("workflowDirectoryFields({input})"),
        )
        .await?;
        if reply.generation != generation(q)
            || reply.stage != stage
            || reply.locale != normalized.locale
            || reply.fields.len() != fields.len()
        {
            return Err(unavailable());
        }
        lowered.extend(reply.fields);
    }
    let input = json!({"generation":generation(q),"names":plan.collation_names()});
    let reply: CollationReply = native::observe(
        state,
        q,
        guard,
        COLLATE,
        &format!("workflowRankingCollation({input})"),
    )
    .await?;
    plan.decode(
        &normalized.locale,
        RendererReply {
            generation: reply.generation,
            locale: reply.locale,
            collation_locale: reply.collation_locale,
            lowered,
            classes: reply.classes,
            demote_passthrough: reply.demote_passthrough,
        },
    )
    .map_err(|_| unavailable())
}
