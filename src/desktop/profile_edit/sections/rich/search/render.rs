//! Profile-owned renderer facts. Names never grant DOM/caret or selection custody.
use super::Search;
use crate::{app_home::channel_fetches::canonical, state::ShellState};
use dioxus::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering as AtomicOrdering},
    },
    time::Duration,
};
use super_platinum_core::{
    block_actions::home::workflow::directory_local::{PreparedQuery, normalize_lowered_query},
    slack::{
        Error,
        api::custom::texty::{matching, ranking},
    },
};

#[cfg(test)]
mod tests;

/// Ojx4.O with Searcher's shouldTrimEmoji/shouldTrimDocument=false. This is
/// before hNDQ and before choosing local/remote; whitespace is not trimmed here.
pub(super) fn formatted_query(raw: &str) -> &str {
    raw.strip_prefix(['@', '\u{ff20}', '#', '\u{ff03}', '/', '\u{ff0f}'])
        .unwrap_or(raw)
}
pub(super) fn model_query(raw: &str) -> &str {
    raw.strip_prefix([
        ':', '@', '\u{ff20}', '#', '\u{ff03}', '/', '\u{ff0f}', '+', '\u{ff0b}',
    ])
    .unwrap_or(raw)
}

pub(super) struct Observed {
    lease: super::super::query::Lease,
    source: Arc<canonical::Canonical>,
    rows: BTreeMap<String, Value>,
    raw_query: String,
    pub(super) formatted_query: String,
    pub(super) query: PreparedQuery,
    locale: String,
    fields: BTreeMap<String, Field>,
    classes: BTreeMap<String, usize>,
    demote_passthrough: bool,
}
impl Observed {
    pub(super) fn current(&self, search: &Search, shell: &ShellState) -> bool {
        self.lease.current(shell)
            && search.current(shell)
            && self.source.current_for(shell, self.lease.team())
    }
    pub(super) fn raw_query(&self) -> &str {
        &self.raw_query
    }
    pub(super) fn locale(&self) -> &str {
        &self.locale
    }
    pub(super) fn demote_passthrough(&self) -> bool {
        self.demote_passthrough
    }
    pub(super) fn normalize(&self, text: &str) -> Result<String, ranking::Error> {
        self.fields
            .get(text)
            .map(|f| normalize_lowered_query(&f.locale_lowered))
            .ok_or_else(|| missing(text))
    }
    pub(super) fn channel_name(
        &self,
        row: &Value,
    ) -> Result<matching::ChannelName, ranking::Error> {
        if row["id"].as_str().and_then(|id| self.rows.get(id)) != Some(row) {
            return Err(missing("original channel row"));
        }
        channel_name(&self.fields, row)
    }
    pub(super) fn compare(&self, a: &str, b: &str) -> Result<Ordering, ranking::Error> {
        compare(&self.classes, a, b)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Field {
    pub(super) text: String,
    pub(super) lowered: String,
    pub(super) locale_lowered: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LowerReply {
    pub(super) ticket: String,
    pub(super) stage: usize,
    pub(super) locale: String,
    pub(super) query: String,
    pub(super) fields: Vec<Field>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CollationReply {
    pub(super) ticket: String,
    pub(super) locale: String,
    pub(super) collation_locale: String,
    pub(super) classes: Vec<(usize, usize)>,
    pub(super) demote_passthrough: bool,
}

/// Only caller-selected ordinary rows from this exact canonical receipt. No
/// Home/Activity owner or inferred typed channel is used to acquire strings.
pub(super) async fn capture(
    state: Signal<ShellState>,
    search: &Search,
    raw_query: &str,
    source: Arc<canonical::Canonical>,
    rows: &[Value],
) -> Result<Observed, Error> {
    let mut fields = inputs(raw_query, rows)?;
    if !source_rows(&source, rows) {
        return Err(unavailable());
    }
    let current = || {
        let shell = state.peek();
        search.current(&shell) && source.current_for(&shell, search.lease.team())
    };
    let ticket = ticket()?;
    let formatted = formatted_query(raw_query);
    let first = lower(search, &current, &ticket, 0, formatted, &[]).await?;
    let normalized = normalize_lowered_query(&first.query);
    let second = lower(search, &current, &ticket, 1, &normalized, &[]).await?;
    same_locale(&first.locale, &second.locale)?;
    let query = PreparedQuery {
        normalized,
        matcher_normalized: normalize_lowered_query(&second.query),
    };
    fields.extend([query.normalized.clone(), query.matcher_normalized.clone()]);
    let fields: Vec<_> = fields.into_iter().collect();
    let mut observed = BTreeMap::new();
    for (stage, batch) in fields.chunks(128).enumerate() {
        let reply = lower(search, &current, &ticket, stage + 2, "", batch).await?;
        same_locale(&first.locale, &reply.locale)?;
        observed.extend(reply.fields.into_iter().map(|f| (f.text.clone(), f)));
    }
    let names: Vec<_> = rows
        .iter()
        .map(|row| {
            channel_name(&observed, row)
                .map(|n| n.comparison_lowercase)
                .map_err(|_| unavailable())
        })
        .collect::<Result<BTreeSet<_>, _>>()?
        .into_iter()
        .collect();
    // Model normalization may consume the ordinary-lowercase comparison too.
    let extra: Vec<_> = names
        .iter()
        .filter(|s| !observed.contains_key(*s))
        .cloned()
        .collect();
    for (stage, batch) in extra.chunks(128).enumerate() {
        let reply = lower(
            search,
            &current,
            &ticket,
            fields.len() + stage + 2,
            "",
            batch,
        )
        .await?;
        same_locale(&first.locale, &reply.locale)?;
        observed.extend(reply.fields.into_iter().map(|f| (f.text.clone(), f)));
    }
    let reply: CollationReply = evaluate(
        search,
        &current,
        "profileChannelCollation",
        json!({"ticket":ticket,"names":names}),
    )
    .await?;
    let demote_passthrough = reply.demote_passthrough;
    let classes = decode_collation(&ticket, &first.locale, &names, reply)?;
    if !current() {
        return Err(unavailable());
    }
    Ok(Observed {
        lease: search.lease.clone(),
        source,
        rows: rows
            .iter()
            .map(|row| (row["id"].as_str().unwrap().to_owned(), row.clone()))
            .collect(),
        raw_query: raw_query.into(),
        formatted_query: formatted.into(),
        query,
        locale: first.locale,
        fields: observed,
        classes,
        demote_passthrough,
    })
}

fn source_rows(source: &canonical::Canonical, rows: &[Value]) -> bool {
    rows.iter().all(|row| {
        row["id"]
            .as_str()
            .is_some_and(|id| source.row(id) == Some(row))
    })
}
fn inputs(raw: &str, rows: &[Value]) -> Result<BTreeSet<String>, Error> {
    let formatted = formatted_query(raw);
    let mut fields = BTreeSet::from([
        String::new(),
        raw.into(),
        formatted.into(),
        model_query(formatted).into(),
    ]);
    for row in rows {
        if !matching::ordinary_local(row) {
            return Err(unavailable());
        }
        let name = row["name"].as_str().ok_or_else(unavailable)?;
        fields.insert(name.into());
        fields.extend(name.split('\n').map(str::to_owned));
        if let Some(previous) = row.get("previous_names").filter(|v| !v.is_null()) {
            for value in previous.as_array().ok_or_else(unavailable)? {
                let name = value.as_str().ok_or_else(unavailable)?;
                if !name.starts_with("mpdm-") {
                    fields.insert(name.into());
                }
            }
        }
    }
    Ok(fields)
}
fn channel_name(
    fields: &BTreeMap<String, Field>,
    row: &Value,
) -> Result<matching::ChannelName, ranking::Error> {
    let raw = row["name"].as_str().ok_or(ranking::Error::InvalidInput)?;
    let field = fields.get(raw).ok_or_else(|| missing(raw))?;
    Ok(matching::ChannelName {
        comparison_lowercase: field.lowered.clone(),
        normalized: normalize_lowered_query(&field.locale_lowered),
    })
}
pub(super) fn compare(
    classes: &BTreeMap<String, usize>,
    a: &str,
    b: &str,
) -> Result<Ordering, ranking::Error> {
    Ok(classes
        .get(a)
        .ok_or_else(|| missing(a))?
        .cmp(classes.get(b).ok_or_else(|| missing(b))?))
}
fn missing(text: &str) -> ranking::Error {
    ranking::Error::MissingObservation(text.into())
}
pub(super) fn same_locale(expected: &str, actual: &str) -> Result<(), Error> {
    if expected.is_empty() || expected != actual {
        Err(unavailable())
    } else {
        Ok(())
    }
}
fn decode_lower(
    ticket: &str,
    stage: usize,
    fields: &[String],
    reply: LowerReply,
) -> Result<LowerReply, Error> {
    if reply.ticket != ticket
        || reply.stage != stage
        || reply.locale.is_empty()
        || reply.fields.len() != fields.len()
        || reply.fields.iter().zip(fields).any(|(a, b)| &a.text != b)
    {
        return Err(unavailable());
    }
    Ok(reply)
}
pub(super) fn decode_collation(
    ticket: &str,
    locale: &str,
    names: &[String],
    reply: CollationReply,
) -> Result<BTreeMap<String, usize>, Error> {
    same_locale(locale, &reply.locale)?;
    if reply.ticket != ticket
        || reply.collation_locale != "en-US"
        || reply.classes.len() != names.len()
        || names.iter().collect::<BTreeSet<_>>().len() != names.len()
    {
        return Err(unavailable());
    }
    let mut seen = BTreeSet::new();
    let mut classes = BTreeMap::new();
    let mut previous = 0;
    for (position, (index, class)) in reply.classes.into_iter().enumerate() {
        if index >= names.len()
            || !seen.insert(index)
            || (position == 0 && class != 0)
            || class < previous
            || class > previous + 1
        {
            return Err(unavailable());
        }
        previous = class;
        classes.insert(names[index].clone(), class);
    }
    Ok(classes)
}
pub(super) async fn lower(
    search: &Search,
    current: &impl Fn() -> bool,
    ticket: &str,
    stage: usize,
    query: &str,
    fields: &[String],
) -> Result<LowerReply, Error> {
    let reply = evaluate(
        search,
        current,
        "profileChannelLower",
        json!({"ticket":ticket,"stage":stage,"query":query,"fields":fields}),
    )
    .await?;
    decode_lower(ticket, stage, fields, reply)
}
pub(super) async fn evaluate<T: serde::de::DeserializeOwned>(
    search: &Search,
    current: &impl Fn() -> bool,
    function: &str,
    input: Value,
) -> Result<T, Error> {
    if !current() {
        return Err(unavailable());
    }
    // Only fixed function names above reach this private evaluator. JSON is a
    // value expression. No interpolated text becomes executable source.
    let script = format!(
        "{}\ndioxus.send({function}({input}));await dioxus.recv();",
        include_str!("render.js")
    );
    let mut eval = dioxus::document::eval(&script);
    let result = tokio::select! {
        biased;
        _ = search.lease.cancelled() => Err(unavailable()),
        result = tokio::time::timeout(Duration::from_secs(5), eval.recv::<Option<T>>()) =>
            result.map_err(|_| unavailable()).and_then(|r| r.map_err(|_| unavailable())).and_then(|r| r.ok_or_else(unavailable)),
    };
    let _ = eval.send(());
    if !current() {
        return Err(unavailable());
    }
    result
}
pub(super) fn ticket() -> Result<String, Error> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_update(AtomicOrdering::Relaxed, AtomicOrdering::Relaxed, |n| {
        n.checked_add(1)
    })
    .map(|n| n.to_string())
    .map_err(|_| unavailable())
}
fn unavailable() -> Error {
    Error::Api("profile_channel_renderer_unavailable".into())
}
