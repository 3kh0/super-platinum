//! Share-owned matching and mixed-family ordering. Ranking never grants posting.
use super::*;
use crate::app_home::channel_fetches::canonical::Canonical;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use super_platinum_core::slack::{Error, PreparedRequest};
use super_platinum_core::workflow_share::search::{self as protocol, ranking as ranked};
mod observations;
mod render;

/// Synthetic fixture diagnostics carry stage names only; live failures retain
/// their original sanitized handling and never log request/session values.
pub(super) fn at(stage: &str, error: Error) -> Error {
    if !crate::fixture::is_fixture()
        || matches!(&error, Error::Api(code) if code.starts_with("share_ranking_stage:"))
    {
        error
    } else {
        Error::Api(format!("share_ranking_stage:{stage}"))
    }
}

pub(super) struct Ordered {
    pub(super) ids: Vec<String>,
    pub(super) proof: Value,
    input: observations::Input,
}
impl Ordered {
    pub(super) fn current(
        &self,
        s: &ShellState,
        q: &Query,
        guard: &impl Fn(&ShellState) -> bool,
    ) -> bool {
        self.input.current(s, q, guard)
    }
}

pub(super) async fn order(
    state: Signal<ShellState>,
    q: &Query,
    canonical: Arc<Canonical>,
    people: &[peer::Candidate],
    channels: &[Value],
    remote_members: &[String],
    remote_channels: &[String],
    guard: &impl Fn(&ShellState) -> bool,
) -> Result<Ordered, Error> {
    let plan = protocol::Query::new(q.expected["query"].as_str().ok_or_else(unavailable)?)?;
    let wire = plan.wire().to_owned();
    let input = observations::capture(state, q, canonical, people, channels, guard)
        .await
        .map_err(|e| at("observations", e))?;
    let channel_fields = render::fields(
        state,
        q,
        &input.normalized,
        input.channel_names.iter().cloned().collect(),
        guard,
    )
    .await
    .map_err(|e| at("channel-renderer-fields", e))?;
    if !input.current(&state.peek(), q, guard) {
        return Err(unavailable());
    }
    let members = input.members.clone();
    let channels = input.channels.clone();
    let member_names = input.member_names.clone();
    let channel_names = input.channel_names.clone();
    let prepared = input.normalized.query.clone();
    let remote_members = remote_members.iter().cloned().collect::<BTreeSet<_>>();
    let remote_channels = remote_channels.iter().cloned().collect::<BTreeSet<_>>();
    let query = wire.clone();
    let facts = ranked::MatchFacts {
        // KhGM's aliases in the native application's current English catalog.
        query_is_maybe_self: matches!(prepared.normalized.as_str(), "me" | "you"),
        // Share does not supply hHTo.matchBeyondWordBoundaries.
        can_match_beyond_word_boundaries: false,
        passthrough_remote_fuzzy_filtering: input.passthrough,
    };
    let rows = tokio::task::spawn_blocking(move || {
        let comparisons = members
            .iter()
            .zip(&member_names)
            .chain(channels.iter().zip(&channel_names))
            .map(|(row, name)| {
                row["id"]
                    .as_str()
                    .map(|id| (id.to_owned(), name.clone()))
                    .ok_or_else(unavailable)
            })
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        let members =
            ranked::match_members(&query, &prepared, &members, &remote_members, &facts, |_| {
                true
            });
        let names = channel_names
            .iter()
            .zip(&channels)
            .map(|(name, row)| {
                let id = row["id"].as_str().ok_or_else(unavailable)?;
                let normalized = channel_fields.get(name).cloned().ok_or_else(unavailable)?;
                Ok((id.to_owned(), normalized))
            })
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        let channels = ranked::match_channels(
            &query,
            &prepared,
            &channels,
            &remote_channels,
            &facts,
            |_| true,
            |row| names[row["id"].as_str().expect("captured canonical ID")].clone(),
            |_| String::new(),
        );
        // Matcher order is source order. Pick comparison names by original ID,
        // never by a post-filter positional index.
        let member_names = members
            .iter()
            .map(|row| comparisons.get(row.id).cloned().ok_or_else(unavailable))
            .collect::<Result<Vec<_>, _>>()?;
        let channel_names = channels
            .iter()
            .map(|row| comparisons.get(row.id).cloned().ok_or_else(unavailable))
            .collect::<Result<Vec<_>, _>>()?;
        ranked::Rows::new(&members, &member_names, &channels, &channel_names)
            .map_err(|e| at(&format!("matched-rows:{e:?}"), unavailable()))
    })
    .await
    .map_err(|_| at("matcher-task", unavailable()))?
    .map_err(|e| at("matching", e))?;
    if !input.current(&state.peek(), q, guard) {
        return Err(unavailable());
    }
    let observed = render::rank(
        state,
        q,
        &wire,
        &input.normalized,
        ranked::ObservationPlan::capture(&render::generation(q), &wire, rows.clone())
            .map_err(|_| unavailable())?,
        guard,
    )
    .await
    .map_err(|e| at("rank-renderer-fields", e))?;
    let matches=rows.identities().iter().map(|i|json!({"id":i.id,"index":i.index,"kind":match i.kind {protocol::Kind::Members=>"members",protocol::Kind::Channels=>"channels"}})).collect::<Vec<_>>();
    let groups = crate::fixture::is_fixture().then(|| rows.groups().clone());
    let cache = input.cache.clone();
    let selectors = input.selectors.clone();
    let history = input.history.clone();
    let now_ms = input.now_ms;
    let raw_query = wire.clone();
    let result = tokio::task::spawn_blocking(move || {
        let context = ranked::RankInputs {
            state: &selectors,
            history: &history,
            model: cache.model(),
            now_ms,
        };
        let mut similarity = cache.similarity().lock().map_err(|_| unavailable())?;
        ranked::rank(&raw_query, &rows, &context, &observed, &mut similarity)
            .map_err(|e| at(&format!("rank:{e:?}"), unavailable()))
    })
    .await
    .map_err(|_| at("rank-task", unavailable()))?
    .map_err(|e| at("ranking", e))?;
    if !input.current(&state.peek(), q, guard) {
        return Err(unavailable());
    }
    let ids = result
        .rows
        .iter()
        .map(|r| r.identity.id.clone())
        .collect::<Vec<_>>();
    let mut proof = json!({"source":ranked::SOURCE,"query":wire,"inputCount":input.members.len()+input.channels.len(),
        "matches":matches,"reads":input.reads,"nowMillis":input.now_ms,
        "observationSource":input.observation_source,"observationReads":input.observation_reads,
        "results":result.rows.iter().map(|r|json!({"id":r.identity.id,"index":r.identity.index,
            "kind":match r.identity.kind {protocol::Kind::Members=>"members",protocol::Kind::Channels=>"channels"},
            "modelScore":r.model_score,"features":r.features})).collect::<Vec<_>>(),
        "channelLatestIds":result.channel_latest_ids});
    if crate::fixture::is_fixture() {
        proof["selectors"] = input.selectors.clone();
        proof["passthrough"] = input.passthrough.into();
        proof["groups"] = groups.unwrap_or(Value::Null);
        proof["memberItems"] = json!(input.members);
        proof["channelItems"] = json!(input.channels);
    }
    Ok(Ordered { ids, proof, input })
}

async fn request(
    state: Signal<ShellState>,
    q: &Query,
    request: PreparedRequest,
    guard: &impl Fn(&ShellState) -> bool,
) -> Result<Value, Error> {
    let (raw, _) = read::dispatch(state, q, request, false, guard).await?;
    if !q.current(&state.peek())
        || !guard(&state.peek())
        || !native::current(q, None).await
        || !q.current(&state.peek())
        || !guard(&state.peek())
    {
        return Err(unavailable());
    }
    Ok(raw)
}
