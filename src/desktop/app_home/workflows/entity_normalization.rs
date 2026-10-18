//! Renderer-owned case conversion; no guessed locale or raw-name approximation.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::block_actions::home::workflow::directory_local::{
    PreparedQuery, normalize_lowered_query,
};

pub(super) struct Normalized {
    pub(super) query: PreparedQuery,
    lowered: BTreeMap<String, String>,
    locale: String,
}
impl Normalized {
    pub(super) fn locale(&self) -> &str {
        &self.locale
    }
    /// Only strings supplied to the actual renderer acquisition are available.
    pub(super) fn name(&self, source: &str) -> Option<&str> {
        self.lowered.get(source).map(String::as_str)
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    generation: String,
    stage: usize,
    locale: String,
    query: String,
    names: Vec<String>,
}

pub(super) async fn capture(
    state: Signal<ShellState>,
    owner: &entity_search::Query,
    names: BTreeSet<String>,
) -> Result<Normalized, Error> {
    let names: Vec<_> = names.into_iter().collect();
    // The first two captures intentionally differ: jytj applies hNDQ again,
    // after the first J3F pass may have exposed new leading whitespace.
    let first = lower(state, owner, 0, &owner.text, &[]).await?;
    let normalized = normalize_lowered_query(&first.query);
    let second = lower(state, owner, 1, &normalized, &[]).await?;
    if first.locale != second.locale {
        return Err(unavailable());
    }
    let query = PreparedQuery {
        normalized,
        matcher_normalized: normalize_lowered_query(&second.query),
    };
    let mut lowered = BTreeMap::new();
    for (batch, values) in names.chunks(128).enumerate() {
        let reply = lower(state, owner, batch + 2, "", values).await?;
        if reply.locale != first.locale {
            return Err(unavailable());
        }
        lowered.extend(values.iter().cloned().zip(reply.names));
    }
    if !owner.current(&state.peek()) {
        return Err(unavailable());
    }
    Ok(Normalized {
        query,
        lowered,
        locale: first.locale,
    })
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldReply {
    generation: String,
    stage: usize,
    locale: String,
    fields: Vec<String>,
}
pub(super) async fn fields(
    state: Signal<ShellState>,
    owner: &entity_search::Query,
    casing: &Normalized,
    fields: BTreeSet<String>,
) -> Result<BTreeMap<String, String>, Error> {
    let generation = owner.generation.to_string();
    let fields: Vec<_> = fields.into_iter().collect();
    let mut result = BTreeMap::new();
    for (stage, values) in fields.chunks(128).enumerate() {
        if !owner.current(&state.peek()) {
            return Err(unavailable());
        }
        let request = serde_json::json!({"generation":generation,"stage":stage,"fields":values});
        let script = format!(
            "{}\ndioxus.send(workflowDirectoryFields({}));await dioxus.recv();",
            include_str!("entity_normalization.js"),
            request
        );
        let mut eval = dioxus::document::eval(&script);
        let response =
            tokio::time::timeout(Duration::from_secs(5), eval.recv::<Option<FieldReply>>()).await;
        let _ = eval.send(());
        if !owner.current(&state.peek()) {
            return Err(unavailable());
        }
        let response = response
            .map_err(|_| unavailable())?
            .map_err(|_| unavailable())?
            .ok_or_else(unavailable)?;
        if response.generation != generation
            || response.stage != stage
            || response.locale != casing.locale
            || response.fields.len() != values.len()
        {
            return Err(unavailable());
        }
        result.extend(
            values.iter().cloned().zip(
                response
                    .fields
                    .into_iter()
                    .map(|v| normalize_lowered_query(&v)),
            ),
        );
    }
    Ok(result)
}

async fn lower(
    state: Signal<ShellState>,
    owner: &entity_search::Query,
    stage: usize,
    query: &str,
    names: &[String],
) -> Result<Reply, Error> {
    if !owner.current(&state.peek()) {
        return Err(unavailable());
    }
    let generation = owner.generation.to_string();
    let request = serde_json::json!({
        "generation":generation,"stage":stage,"query":query,"names":names,
    });
    let script = format!(
        "{}\ndioxus.send(workflowDirectoryLower({}));await dioxus.recv();",
        include_str!("entity_normalization.js"),
        request,
    );
    let mut eval = dioxus::document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(5), eval.recv::<Option<Reply>>()).await;
    // Keep the evaluator alive until the receipt is received; release also on
    // refusal/timeout. A late reply never becomes new query authority.
    let _ = eval.send(());
    if !owner.current(&state.peek()) {
        return Err(unavailable());
    }
    let reply = result
        .map_err(|_| unavailable())?
        .map_err(|_| unavailable())?
        .ok_or_else(unavailable)?;
    if reply.generation != generation
        || reply.stage != stage
        || reply.names.len() != names.len()
        || reply.locale.is_empty()
    {
        return Err(unavailable());
    }
    Ok(reply)
}
fn unavailable() -> Error {
    Error::Api("home_workflow_normalization_unavailable".into())
}
