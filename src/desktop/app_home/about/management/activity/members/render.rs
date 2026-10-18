//! Exact renderer observations, batched without mixing generations/locales.
//! Query and names use different source case conversion rules.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::block_actions::{
    home::workflow::directory_local::{PreparedQuery, normalize_lowered_query},
    workflow_management::activity_members::{ObservationPlan, RankObservations, RendererReply},
};

pub(super) struct Normalized {
    pub(super) query: PreparedQuery,
    generation: String,
    source: String,
    locale: String,
    names: BTreeMap<String, String>,
}
impl Normalized {
    pub(super) fn belongs_to(&self, query: &Query) -> bool {
        self.generation == query.generation() && self.source == query.text()
    }
    pub(super) fn locale(&self) -> &str {
        &self.locale
    }
    pub(super) fn name(&self, raw: &str) -> Option<&str> {
        self.names.get(raw).map(String::as_str)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LowerReply {
    generation: String,
    stage: usize,
    locale: String,
    query: String,
    names: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldsReply {
    generation: String,
    stage: usize,
    locale: String,
    fields: Vec<String>,
}
#[derive(Deserialize)]
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
    names: BTreeSet<String>,
) -> Result<Normalized, Error> {
    let first = lower(state, q, 0, q.text(), &[]).await?;
    let normalized = normalize_lowered_query(&first.query);
    let second = lower(state, q, 1, &normalized, &[]).await?;
    if second.locale != first.locale {
        return Err(unavailable());
    }
    let query = PreparedQuery {
        normalized,
        matcher_normalized: normalize_lowered_query(&second.query),
    };
    let names: Vec<_> = names.into_iter().collect();
    let mut result = BTreeMap::new();
    for (index, batch) in names.chunks(128).enumerate() {
        let reply = lower(state, q, index + 2, "", batch).await?;
        if reply.locale != first.locale {
            return Err(unavailable());
        }
        result.extend(batch.iter().cloned().zip(reply.names));
    }
    if !q.current(&state.peek()) {
        return Err(unavailable());
    }
    Ok(Normalized {
        query,
        generation: q.generation(),
        source: q.text().to_owned(),
        locale: first.locale,
        names: result,
    })
}

pub(super) async fn rank(
    state: Signal<ShellState>,
    q: &Query,
    normalized: &Normalized,
    plan: ObservationPlan,
) -> Result<RankObservations, Error> {
    if plan.generation() != q.generation() || !normalized.belongs_to(q) || !q.current(&state.peek())
    {
        return Err(unavailable());
    }
    let mut lowered = Vec::with_capacity(plan.normalization_fields().len());
    for (stage, batch) in plan.normalization_fields().chunks(128).enumerate() {
        let input = serde_json::json!({"generation":q.generation(),"stage":stage,"fields":batch});
        let reply: FieldsReply = evaluate(
            state,
            q,
            include_str!("normalization.js"),
            "workflowDirectoryFields",
            input,
        )
        .await?;
        if !receipt(
            &reply.generation,
            reply.stage,
            &reply.locale,
            reply.fields.len(),
            q,
            stage,
            batch.len(),
        ) || reply.locale != normalized.locale
        {
            return Err(unavailable());
        }
        lowered.extend(reply.fields);
    }
    let input = serde_json::json!({"generation":q.generation(),"names":plan.collation_names()});
    let reply: CollationReply = evaluate(
        state,
        q,
        include_str!("collation.js"),
        "workflowRankingCollation",
        input,
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
async fn lower(
    state: Signal<ShellState>,
    q: &Query,
    stage: usize,
    query: &str,
    names: &[String],
) -> Result<LowerReply, Error> {
    if names.len() > 128 {
        return Err(unavailable());
    }
    let input =
        serde_json::json!({"generation":q.generation(),"stage":stage,"query":query,"names":names});
    let reply: LowerReply = evaluate(
        state,
        q,
        include_str!("normalization.js"),
        "workflowDirectoryLower",
        input,
    )
    .await?;
    if !receipt(
        &reply.generation,
        reply.stage,
        &reply.locale,
        reply.names.len(),
        q,
        stage,
        names.len(),
    ) {
        return Err(unavailable());
    }
    Ok(reply)
}

/// Keep duplicate fields and their exact encounter order. This receipt is for
/// the owned hydration Plan, not locale-normalized query matching.
pub(super) async fn hydrate(
    state: Signal<ShellState>,
    q: &Query,
    pending: &super_platinum_core::state::directory_members::Pending,
) -> Result<
    super_platinum_core::block_actions::workflow_management::member_hydration::RendererReply,
    Error,
> {
    hydrate_fields(state, q, pending.generation(), pending.fields()).await
}

pub(super) async fn hydrate_fields(
    state: Signal<ShellState>,
    q: &Query,
    generation: &str,
    fields: &[String],
) -> Result<
    super_platinum_core::block_actions::workflow_management::member_hydration::RendererReply,
    Error,
> {
    if generation != q.generation() || !q.current(&state.peek()) {
        return Err(unavailable());
    }
    let mut lowered = Vec::with_capacity(fields.len());
    let mut locale = None;
    for (stage, fields) in fields.chunks(128).enumerate() {
        let reply = lower(state, q, stage, "", fields).await?;
        if locale.as_ref().is_some_and(|value| value != &reply.locale) {
            return Err(unavailable());
        }
        locale = Some(reply.locale);
        lowered.extend(reply.names);
    }
    if !q.current(&state.peek()) {
        return Err(unavailable());
    }
    Ok(
        super_platinum_core::block_actions::workflow_management::member_hydration::RendererReply {
            generation: q.generation(),
            operation: "ordinary-toLowerCase".into(),
            fields: fields.to_vec(),
            lowered,
        },
    )
}
fn receipt(
    generation: &str,
    actual_stage: usize,
    locale: &str,
    count: usize,
    q: &Query,
    stage: usize,
    expected: usize,
) -> bool {
    generation == q.generation() && actual_stage == stage && !locale.is_empty() && count == expected
}
async fn evaluate<T: serde::de::DeserializeOwned>(
    state: Signal<ShellState>,
    q: &Query,
    helper: &str,
    function: &str,
    input: Value,
) -> Result<T, Error> {
    if !q.current(&state.peek()) {
        return Err(unavailable());
    }
    // Only the two fixed helpers and three literal functions above call this.
    // JSON payload is a value expression, never user-generated source code.
    let script = format!("{helper}\ndioxus.send({function}({input}));await dioxus.recv();");
    native::query(state, q).await?;
    let mut eval = dioxus::document::eval(&script);
    let response = tokio::time::timeout(Duration::from_secs(5), eval.recv::<Option<T>>()).await;
    let _ = eval.send(());
    if !q.current(&state.peek()) {
        return Err(unavailable());
    }
    native::query(state, q).await?;
    response
        .map_err(|_| unavailable())?
        .map_err(|_| unavailable())?
        .ok_or_else(unavailable)
}
