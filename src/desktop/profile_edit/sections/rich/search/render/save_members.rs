//! Save-owned member/group strings. No popup Search, Lease or result authority.
use super::{Field, compare, decode_collation, model_query, same_locale, save, ticket};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    time::Duration,
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

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Facts {
    ticket: String,
    query: String,
    locale: String,
    presentation: Value,
    maybe_self: bool,
    fields: Vec<GroupField>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GroupField {
    text: String,
    lowered: String,
}

pub(crate) struct MemberStrings {
    pub(crate) query: local::PreparedQuery,
    pub(crate) beyond_boundaries: bool,
    pub(crate) beyond_boundary_eligible: Option<bool>,
    pub(crate) maybe_self: bool,
    original: Vec<Value>,
    rows: Vec<Value>,
    self_row: Value,
    comparisons: BTreeMap<String, String>,
    groups: Vec<groups::Snapshot>,
    group_lower: BTreeMap<String, String>,
    fields: BTreeMap<String, Field>,
    classes: BTreeMap<String, usize>,
    raw: String,
    formatted: String,
    locale: String,
    options: save::Options,
    eligibility: Option<save::Eligibility>,
    demote: bool,
}

impl MemberStrings {
    /// The caller retains original source, policy and Save custody. These
    /// borrowed inputs are cloned before the first native await; no recapture.
    pub(crate) async fn capture(
        raw: &str,
        formatted: &str,
        members: &[Value],
        actual_self: &Value,
        groups: &[groups::Snapshot],
        policy: &name_policy::Snapshot,
        current: &impl Fn() -> bool,
    ) -> Result<Self, Error> {
        if !current() || members.len() + groups.len() > 32768 {
            return Err(unavailable());
        }
        let original = members.to_vec();
        let actual_self = actual_self.clone();
        let groups = groups.to_vec();
        let mut inputs = member_inputs(raw, formatted, &original, &actual_self)?;
        let ticket = ticket()?;
        let options = save::options(&ticket, current).await?;
        let first = save::lower(current, &ticket, 0, formatted, &[]).await?;
        let normalized = local::normalize_lowered_query(&first.query);
        let second = save::lower(current, &ticket, 1, &normalized, &[]).await?;
        same_locale(&first.locale, &second.locale)?;
        let query = local::PreparedQuery {
            normalized,
            matcher_normalized: local::normalize_lowered_query(&second.query),
        };
        let eligibility = if options.enabled {
            let reply: save::Eligibility = save::evaluate(
                current,
                save::Function::Eligibility,
                json!({"ticket":ticket,"query":query.matcher_normalized}),
            )
            .await?;
            reply.validate(&ticket, &query.matcher_normalized, &first.locale)?;
            Some(reply)
        } else {
            None
        };
        inputs.extend([
            query.normalized.clone(),
            query.matcher_normalized.clone(),
            model_query(&query.normalized).to_owned(),
        ]);
        let group_inputs = groups
            .iter()
            .map(|g| group_string(&g.effective()["name"]).map(str::to_owned))
            .collect::<Result<BTreeSet<_>, _>>()?;
        let mut group_lower = BTreeMap::new();
        let mut maybe_self = None;
        // One empty batch is intentional: KhGM is observed even without groups.
        let group_inputs: Vec<_> = group_inputs.into_iter().collect();
        for name in &group_inputs {
            bounded(name)?;
        }
        for batch in if group_inputs.is_empty() {
            vec![&[][..]]
        } else {
            group_inputs.chunks(128).collect()
        } {
            let facts = facts(current, &ticket, &query.normalized, batch).await?;
            validate_facts(&facts, &ticket, &query.normalized, &first.locale, batch)?;
            if maybe_self.is_some_and(|v| v != facts.maybe_self) {
                return Err(unavailable());
            }
            maybe_self = Some(facts.maybe_self);
            group_lower.extend(facts.fields.into_iter().map(|f| (f.text, f.lowered)));
        }
        let mut fields = BTreeMap::new();
        let mut stage = 2;
        observe(
            current,
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
        let self_row = derive(&actual_self, &fields)?;
        let mut comparisons = BTreeMap::new();
        let mut extra = BTreeSet::new();
        let mut collation = BTreeSet::new();
        for row in &rows {
            let comparison = rank_fields(row, policy, &mut extra, &mut collation)?;
            if comparisons
                .insert(
                    row["id"].as_str().ok_or_else(unavailable)?.into(),
                    comparison,
                )
                .is_some()
            {
                return Err(unavailable());
            }
        }
        rank_fields(&self_row, policy, &mut extra, &mut collation)?;
        for group in &groups {
            group_fields(group, &group_lower, &mut extra, &mut collation)?;
        }
        observe(
            current,
            &ticket,
            &first.locale,
            &mut stage,
            &mut fields,
            extra,
        )
        .await?;
        // The automatic search rank callback consumes ordinary-lowercased
        // VRXO names. Keep both observations: group and selector strings may
        // independently remain raw, and neither is an insertion-name receipt.
        let lowered: BTreeSet<_> = collation
            .iter()
            .map(|s| ordinary(&fields, s))
            .collect::<Result<_, _>>()
            .map_err(|_| unavailable())?;
        observe(
            current,
            &ticket,
            &first.locale,
            &mut stage,
            &mut fields,
            lowered.clone(),
        )
        .await?;
        collation.extend(lowered);
        let collation: Vec<_> = collation.into_iter().collect();
        let reply: super::CollationReply = save::evaluate(
            current,
            save::Function::Collation,
            json!({"ticket":ticket,"names":collation}),
        )
        .await?;
        let demote = reply.demote_passthrough;
        let classes = decode_collation(&ticket, &first.locale, &collation, reply)?;
        if options != save::options(&ticket, current).await? || !current() {
            return Err(unavailable());
        }
        Ok(Self {
            beyond_boundaries: options.enabled,
            beyond_boundary_eligible: eligibility.as_ref().map(|v| v.eligible),
            maybe_self: maybe_self.ok_or_else(unavailable)?,
            query,
            original,
            rows,
            self_row,
            comparisons,
            groups,
            group_lower,
            fields,
            classes,
            raw: raw.into(),
            formatted: formatted.into(),
            locale: first.locale,
            options,
            eligibility,
            demote,
        })
    }
    pub(crate) fn rows(&self) -> &[Value] {
        &self.rows
    }
    pub(crate) fn original_rows(&self) -> &[Value] {
        &self.original
    }
    pub(crate) fn comparison(&self, row: &Value) -> Result<String, ranking::Error> {
        if !self
            .original
            .iter()
            .zip(&self.rows)
            .any(|(raw, derived)| row == raw || row == derived)
        {
            return Err(missing("original Save member"));
        }
        let name = row["id"]
            .as_str()
            .and_then(|id| self.comparisons.get(id))
            .ok_or_else(|| missing("Save member comparison"))?;
        ordinary(&self.fields, name)
    }
    pub(crate) fn self_row(&self) -> &Value {
        &self.self_row
    }
    pub(crate) fn comparisons(&self) -> &BTreeMap<String, String> {
        &self.comparisons
    }
    pub(crate) fn ordinary_lowercase(&self, text: &str) -> Result<String, ranking::Error> {
        ordinary(&self.fields, text)
    }
    /// hNDQ for local matching/rank only, never resolver 8KmV.Pc.
    pub(crate) fn normalize(&self, text: &str) -> Result<String, ranking::Error> {
        self.fields
            .get(text)
            .map(|f| local::normalize_lowered_query(&f.locale_lowered))
            .ok_or_else(|| missing(text))
    }
    pub(crate) fn compare(&self, a: &str, b: &str) -> Result<Ordering, ranking::Error> {
        compare(&self.classes, a, b)
    }
    pub(crate) fn group_names(&self, row: &groups::Snapshot) -> Result<groups::Names, Error> {
        if !self
            .groups
            .iter()
            .any(|g| g.source() == row.source() && g.effective() == row.effective())
        {
            return Err(unavailable());
        }
        groups::Names::observe(
            row,
            |text| self.group_lower.get(text).cloned().ok_or_else(unavailable),
            |text| self.normalize(text).map_err(|_| unavailable()),
        )
    }
    pub(crate) fn demote_passthrough(&self) -> bool {
        self.demote
    }
    pub(crate) fn locale(&self) -> &str {
        &self.locale
    }
    pub(crate) fn snapshot(&self) -> Value {
        json!({"rawQuery":self.raw,"formattedQuery":self.formatted,"locale":self.locale,
            "query":{"normalized":self.query.normalized,"matcherNormalized":self.query.matcher_normalized},
            "options":self.options,"eligibility":self.eligibility,"maybeSelf":self.maybe_self,
            "presentation":english(),"originalMembers":self.original,"members":self.rows,
            "self":self.self_row,"comparisons":self.comparisons,"groupLower":self.group_lower,
            "groups":self.groups.iter().map(|g| json!({"source":g.source(),"effective":g.effective()})).collect::<Vec<_>>(),
            "fields":self.fields.values().map(|f| json!({"text":f.text,"lowered":f.lowered,"localeLowered":f.locale_lowered})).collect::<Vec<_>>(),
            "classes":self.classes,"demotePassthrough":self.demote})
    }
}

/// 8KmV.Pc: J3F first, then ordinary JavaScript lowercase. No trim/collapse.
pub(crate) async fn normalize_member(
    text: &str,
    current: &impl Fn() -> bool,
) -> Result<String, Error> {
    bounded(text)?;
    let transformed = local::normalize_name_source(text);
    let ticket = ticket()?;
    let reply = save::lower(current, &ticket, 0, "", &[transformed.clone()]).await?;
    reply
        .fields
        .into_iter()
        .find(|f| f.text == transformed)
        .map(|f| f.lowered)
        .ok_or_else(unavailable)
}
/// tyx1: default Ojx4 prefix processing, then exactly one hNDQ pass.
pub(crate) async fn history_query(
    text: &str,
    current: &impl Fn() -> bool,
) -> Result<String, Error> {
    bounded(text)?;
    let reply = save::lower(current, &ticket()?, 0, model_query(text), &[]).await?;
    Ok(local::normalize_lowered_query(&reply.query))
}

fn english() -> Value {
    json!({"locale":"en-US","origin":"application-English","me":"me","you":"you"})
}
fn validate_facts(
    reply: &Facts,
    ticket: &str,
    query: &str,
    locale: &str,
    inputs: &[String],
) -> Result<(), Error> {
    same_locale(locale, &reply.locale)?;
    if reply.ticket != ticket
        || reply.query != query
        || reply.presentation != english()
        || reply.maybe_self != matches!(query, "me" | "you")
        || reply.fields.len() != inputs.len()
        || reply.fields.iter().zip(inputs).any(|(a, b)| &a.text != b)
    {
        return Err(unavailable());
    }
    Ok(())
}
async fn facts(
    current: &impl Fn() -> bool,
    ticket: &str,
    query: &str,
    names: &[String],
) -> Result<Facts, Error> {
    if !current() {
        return Err(unavailable());
    }
    let input = json!({"ticket":ticket,"query":query,"names":names});
    let script = format!(
        "{}\ndioxus.send(saveMemberFacts({input}));await dioxus.recv();",
        include_str!("save_members.js")
    );
    let mut eval = dioxus::document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(5), eval.recv::<Option<Facts>>())
        .await
        .map_err(|_| unavailable())
        .and_then(|r| r.map_err(|_| unavailable()))
        .and_then(|r| r.ok_or_else(unavailable));
    let _ = eval.send(());
    if !current() {
        return Err(unavailable());
    }
    result
}
async fn observe(
    current: &impl Fn() -> bool,
    ticket: &str,
    locale: &str,
    stage: &mut usize,
    fields: &mut BTreeMap<String, Field>,
    wanted: BTreeSet<String>,
) -> Result<(), Error> {
    if fields.len() + wanted.len() > 262144 {
        return Err(unavailable());
    }
    let missing: Vec<_> = wanted
        .into_iter()
        .filter(|s| !fields.contains_key(s))
        .collect();
    for text in &missing {
        bounded(text)?;
    }
    for batch in missing.chunks(128) {
        let reply = save::lower(current, ticket, *stage, "", batch).await?;
        *stage = stage.checked_add(1).ok_or_else(unavailable)?;
        same_locale(locale, &reply.locale)?;
        fields.extend(reply.fields.into_iter().map(|f| (f.text.clone(), f)));
    }
    Ok(())
}
fn bounded(s: &str) -> Result<(), Error> {
    if s.encode_utf16().count() > 262144 {
        Err(unavailable())
    } else {
        Ok(())
    }
}
fn string(v: &Value) -> Result<&str, Error> {
    match v {
        Value::Null => Ok(""),
        Value::String(s) => Ok(s),
        _ => Err(unavailable()),
    }
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        _ => true,
    }
}
fn group_string(v: &Value) -> Result<&str, Error> {
    if truth(v) { string(v) } else { Ok("") }
}
fn member_inputs(
    raw: &str,
    formatted: &str,
    rows: &[Value],
    actual_self: &Value,
) -> Result<BTreeSet<String>, Error> {
    bounded(raw)?;
    bounded(formatted)?;
    let mut out = BTreeSet::from([
        String::new(),
        raw.into(),
        formatted.into(),
        model_query(formatted).into(),
    ]);
    let mut ids = BTreeSet::new();
    for (index, row) in rows.iter().chain(std::iter::once(actual_self)).enumerate() {
        if !row.is_object()
            || row["id"].as_str().is_none_or(str::is_empty)
            || (!row["profile"].is_null() && !row["profile"].is_object())
        {
            return Err(unavailable());
        }
        if index < rows.len() && !ids.insert(row["id"].as_str().unwrap()) {
            return Err(unavailable());
        }
        for key in ["real_name", "display_name", "email"] {
            out.insert(string(&row["profile"][key])?.into());
        }
    }
    Ok(out)
}
fn ordinary(fields: &BTreeMap<String, Field>, text: &str) -> Result<String, ranking::Error> {
    fields
        .get(text)
        .map(|f| f.lowered.clone())
        .ok_or_else(|| missing(text))
}
fn derive(raw: &Value, fields: &BTreeMap<String, Field>) -> Result<Value, Error> {
    let absent = Cell::new(false);
    let value = local::derive_member_names(raw, |text| {
        ordinary(fields, text).unwrap_or_else(|_| {
            absent.set(true);
            String::new()
        })
    })
    .ok_or_else(unavailable)?;
    if absent.get() {
        Err(unavailable())
    } else {
        Ok(value)
    }
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
    collation.insert(
        if comparison.is_empty() {
            raw
        } else {
            &comparison
        }
        .into(),
    );
    for text in [
        raw,
        if best.is_empty() { raw } else { best },
        &comparison,
        string(&item["facts"]["firstName"])?,
        string(&item["facts"]["lastName"])?,
    ] {
        fields.insert(text.into());
        fields.extend(text.split('\n').map(str::to_owned));
    }
    Ok(comparison)
}
fn group_fields(
    row: &groups::Snapshot,
    lower: &BTreeMap<String, String>,
    fields: &mut BTreeSet<String>,
    collation: &mut BTreeSet<String>,
) -> Result<(), Error> {
    let raw = row.effective();
    let name = group_string(&raw["name"])?;
    let stored = group_string(&raw["_name_lc"])?;
    let scoring = if stored.is_empty() {
        lower.get(name).ok_or_else(unavailable)?.as_str()
    } else {
        stored
    };
    let handle = group_string(&raw["handle"])?;
    let stripped = handle.strip_prefix('@').unwrap_or(handle);
    let prefixed = if handle.is_empty() {
        String::new()
    } else {
        format!("@{stripped}")
    };
    let final_name = if name.is_empty() {
        group_string(&raw["string"])?
    } else {
        name
    };
    for text in [name, scoring, handle, stripped, &prefixed, final_name] {
        fields.insert(text.into());
        fields.extend(text.split('\n').map(str::to_owned));
    }
    collation.insert(
        if truth(&raw["is_usergroup"]) && !handle.is_empty() {
            handle
        } else {
            name
        }
        .into(),
    );
    collation.insert(final_name.into());
    Ok(())
}
fn missing(text: &str) -> ranking::Error {
    ranking::Error::MissingObservation(text.into())
}
fn unavailable() -> Error {
    Error::Api("list_save_member_strings_unavailable".into())
}

#[cfg(test)]
mod tests;
