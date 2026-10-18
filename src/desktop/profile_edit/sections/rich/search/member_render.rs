//! Profile member strings from the original source/policy and this query's
//! renderer. These observations grant no native caret or selection authority.
use super::{Search, group_render, group_source, member_names, render};
use crate::{
    directory_member_fetches::search::Snapshot, profile_edit::sections::rich::query::Lease,
    state::ShellState,
};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    cell::Cell,
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use super_platinum_core::{
    block_actions::{
        home::workflow::{directory_local as local, directory_selectors as selectors},
        name_policy,
    },
    slack::{
        Error,
        api::custom::texty::{groups, ranking},
    },
};
#[cfg(test)]
mod tests;

fn unavailable() -> Error {
    Error::Api("profile_member_renderer_unavailable".into())
}
fn missing(text: &str) -> ranking::Error {
    ranking::Error::MissingObservation(text.into())
}

pub(super) struct Observed {
    lease: Lease,
    source: Arc<Snapshot>,
    names: Arc<member_names::Observation>,
    groups: Option<group_render::Observed>,
    raw_query: String,
    pub(super) formatted_query: String,
    pub(super) query: local::PreparedQuery,
    rows: Vec<Value>,
    self_row: Value,
    comparisons: BTreeMap<String, String>,
    fields: BTreeMap<String, render::Field>,
    classes: BTreeMap<String, usize>,
    locale: String,
    demote: bool,
}
impl Observed {
    pub(super) fn current(&self, search: &Search, shell: &ShellState) -> bool {
        self.lease.current(shell)
            && search.current(shell)
            && self.source.current(shell)
            && self.names.current(shell, search)
            && self
                .groups
                .as_ref()
                .is_none_or(|groups| groups.current(shell))
    }
    pub(super) fn group_names(&self, row: &groups::Snapshot) -> Result<groups::Names, Error> {
        self.groups
            .as_ref()
            .ok_or_else(unavailable)?
            .names(row, |text| self.normalize(text))
    }
    pub(super) fn group_labels(&self) -> Option<&BTreeMap<String, String>> {
        self.groups.as_ref().map(|groups| groups.labels())
    }
    pub(super) fn rows(&self) -> &[Value] {
        &self.rows
    }
    pub(super) fn self_row(&self) -> &Value {
        &self.self_row
    }
    pub(super) fn comparisons(&self) -> &BTreeMap<String, String> {
        &self.comparisons
    }
    pub(super) fn raw_query(&self) -> &str {
        &self.raw_query
    }
    pub(super) fn locale(&self) -> &str {
        &self.locale
    }
    pub(super) fn ordinary_lowercase(&self, text: &str) -> Result<String, ranking::Error> {
        ordinary(&self.fields, text)
    }
    pub(super) fn normalize(&self, text: &str) -> Result<String, ranking::Error> {
        normalized(&self.fields, text)
    }
    pub(super) fn compare(&self, a: &str, b: &str) -> Result<Ordering, ranking::Error> {
        render::compare(&self.classes, a, b)
    }
    pub(super) fn demote_passthrough(&self) -> bool {
        self.demote
    }
}

pub(super) async fn capture(
    state: Signal<ShellState>,
    search: &Search,
    raw_query: &str,
    source: Arc<Snapshot>,
    names: Arc<member_names::Observation>,
) -> Result<Observed, Error> {
    capture_mixed(state, search, raw_query, source, names, None).await
}

pub(super) async fn capture_mixed(
    state: Signal<ShellState>,
    search: &Search,
    raw_query: &str,
    source: Arc<Snapshot>,
    names: Arc<member_names::Observation>,
    group_source: Option<Arc<group_source::Snapshot>>,
) -> Result<Observed, Error> {
    let current = || {
        let shell = state.peek();
        search.current(&shell)
            && source.current(&shell)
            && names.current(&shell, search)
            && group_source
                .as_ref()
                .is_none_or(|groups| groups.current(&shell))
    };
    if !current() {
        return Err(unavailable());
    }
    let original: Vec<_> = source.rows().iter().map(|r| r.source.clone()).collect();
    let actual_self = &names.gantry["self"];
    if actual_self["id"].as_str() != Some(search.context.session.user_id.as_str()) {
        return Err(unavailable());
    }
    let mut inputs = inputs(raw_query, &original, actual_self)?;
    let ticket = render::ticket()?;
    let formatted = render::formatted_query(raw_query);
    let first = render::lower(search, &current, &ticket, 0, formatted, &[]).await?;
    let query_normalized = local::normalize_lowered_query(&first.query);
    let second = render::lower(search, &current, &ticket, 1, &query_normalized, &[]).await?;
    render::same_locale(&first.locale, &second.locale)?;
    let query = local::PreparedQuery {
        normalized: query_normalized,
        matcher_normalized: local::normalize_lowered_query(&second.query),
    };
    for value in [&query.normalized, &query.matcher_normalized] {
        inputs.insert(value.clone());
        inputs.insert(render::model_query(value).into());
    }
    let mut fields = BTreeMap::new();
    let mut stage = 2;
    observe_fields(
        search,
        &current,
        &ticket,
        &first.locale,
        &mut stage,
        &mut fields,
        inputs,
    )
    .await?;
    let rows = original
        .iter()
        .map(|r| derive(r, &fields))
        .collect::<Result<Vec<_>, _>>()?;
    let self_row = derive(actual_self, &fields)?;
    let mut comparisons = BTreeMap::new();
    let mut extra = BTreeSet::new();
    let mut collation = BTreeSet::new();
    for row in &rows {
        let comparison = rank_fields(row, &names.policy, &mut extra, &mut collation)?;
        let id = row["id"].as_str().ok_or_else(unavailable)?;
        if comparisons.insert(id.into(), comparison).is_some() {
            return Err(unavailable());
        }
    }
    // Gantry self is an independently observed row. It may differ from source
    // inventory self; neither replaces the other's immutable matching fields.
    rank_fields(&self_row, &names.policy, &mut extra, &mut collation)?;
    let groups = if let Some(source) = &group_source {
        let observed = group_render::capture(
            search,
            &current,
            &ticket,
            &first.locale,
            &mut stage,
            source.clone(),
        )
        .await?;
        observed.extend_fields(&mut extra, &mut collation)?;
        Some(observed)
    } else {
        None
    };
    observe_fields(
        search,
        &current,
        &ticket,
        &first.locale,
        &mut stage,
        &mut fields,
        extra,
    )
    .await?;
    let collation: Vec<_> = collation.into_iter().collect();
    let reply: render::CollationReply = render::evaluate(
        search,
        &current,
        "profileChannelCollation",
        json!({"ticket":ticket,"names":collation}),
    )
    .await?;
    let demote = reply.demote_passthrough;
    let classes = render::decode_collation(&ticket, &first.locale, &collation, reply)?;
    if !current() {
        return Err(unavailable());
    }
    Ok(Observed {
        lease: search.lease.clone(),
        source,
        names,
        raw_query: raw_query.into(),
        formatted_query: formatted.into(),
        query,
        rows,
        self_row,
        comparisons,
        groups,
        fields,
        classes,
        locale: first.locale,
        demote,
    })
}

async fn observe_fields(
    search: &Search,
    current: &impl Fn() -> bool,
    ticket: &str,
    locale: &str,
    stage: &mut usize,
    fields: &mut BTreeMap<String, render::Field>,
    wanted: BTreeSet<String>,
) -> Result<(), Error> {
    let missing: Vec<_> = wanted
        .into_iter()
        .filter(|text| !fields.contains_key(text))
        .collect();
    for batch in missing.chunks(128) {
        let reply = render::lower(search, current, ticket, *stage, "", batch).await?;
        *stage = stage.checked_add(1).ok_or_else(unavailable)?;
        render::same_locale(locale, &reply.locale)?;
        fields.extend(reply.fields.into_iter().map(|f| (f.text.clone(), f)));
    }
    Ok(())
}

fn string(value: &Value) -> Result<&str, Error> {
    match value {
        Value::Null => Ok(""),
        Value::String(s) => Ok(s),
        _ => Err(unavailable()),
    }
}
fn inputs(raw: &str, rows: &[Value], actual_self: &Value) -> Result<BTreeSet<String>, Error> {
    let formatted = render::formatted_query(raw);
    let mut inputs = BTreeSet::from([
        String::new(),
        raw.into(),
        formatted.into(),
        render::model_query(formatted).into(),
    ]);
    for row in rows.iter().chain(std::iter::once(actual_self)) {
        if !row.is_object()
            || row["id"].as_str().is_none_or(str::is_empty)
            || (!row["profile"].is_null() && !row["profile"].is_object())
        {
            return Err(unavailable());
        }
        for key in ["real_name", "display_name", "email"] {
            inputs.insert(string(&row["profile"][key])?.into());
        }
    }
    Ok(inputs)
}
fn ordinary(
    fields: &BTreeMap<String, render::Field>,
    text: &str,
) -> Result<String, ranking::Error> {
    fields
        .get(text)
        .map(|f| f.lowered.clone())
        .ok_or_else(|| missing(text))
}
fn normalized(
    fields: &BTreeMap<String, render::Field>,
    text: &str,
) -> Result<String, ranking::Error> {
    fields
        .get(text)
        .map(|f| local::normalize_lowered_query(&f.locale_lowered))
        .ok_or_else(|| missing(text))
}
fn derive(raw: &Value, fields: &BTreeMap<String, render::Field>) -> Result<Value, Error> {
    let absent = Cell::new(false);
    let result = local::derive_member_names(raw, |text| {
        ordinary(fields, text).unwrap_or_else(|_| {
            absent.set(true);
            String::new()
        })
    })
    .ok_or_else(unavailable)?;
    if absent.get() {
        return Err(unavailable());
    }
    Ok(result)
}
fn rank_fields(
    row: &Value,
    policy: &name_policy::Snapshot,
    fields: &mut BTreeSet<String>,
    collation: &mut BTreeSet<String>,
) -> Result<String, Error> {
    let comparison = policy
        .member_name(row, "Name hidden")
        .map_err(|_| unavailable())?;
    let item = selectors::item(
        row,
        selectors::Names {
            comparison: &comparison,
            mpim: None,
            custom: None,
            custom_allowed: false,
        },
    )
    .ok_or_else(unavailable)?;
    if item["facts"]["member"] != true
        || !item["facts"]["channelType"].is_null()
        || item["facts"]["mpim"] == true
    {
        return Err(unavailable());
    }
    let raw = string(&item["name"])?;
    let best = string(&item["facts"]["bestName"])?;
    let best = if best.is_empty() { raw } else { best };
    let ordered = if comparison.is_empty() {
        raw
    } else {
        &comparison
    };
    collation.insert(ordered.into());
    for value in [
        raw,
        best,
        comparison.as_str(),
        string(&item["facts"]["firstName"])?,
        string(&item["facts"]["lastName"])?,
    ] {
        fields.insert(value.into());
        fields.extend(value.split('\n').map(str::to_owned));
    }
    Ok(comparison)
}
