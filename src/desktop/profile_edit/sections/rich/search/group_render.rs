//! Group strings supplement the SAME Profile member normalization/collation.
//! No independent ranking, query authority, or insertion admission lives here.
use super::{Search, group_source, render};
use crate::state::ShellState;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use super_platinum_core::slack::{
    Error,
    api::custom::texty::{groups, ranking},
};
#[cfg(test)]
mod tests;
fn unavailable() -> Error {
    Error::Api("profile_group_renderer_unavailable".into())
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null | Value::Bool(false) => false,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64().is_some_and(|v| v != 0.0),
        _ => true,
    }
}
fn string(v: &Value) -> Result<&str, Error> {
    if !truth(v) {
        Ok("")
    } else {
        v.as_str().ok_or_else(unavailable)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Field {
    id: String,
    lowered: String,
    label: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    ticket: String,
    stage: usize,
    locale: String,
    inputs: Vec<Value>,
    fields: Vec<Field>,
}

pub(super) struct Observed {
    source: Arc<group_source::Snapshot>,
    fields: BTreeMap<String, Field>,
    originals: BTreeMap<String, groups::Snapshot>,
    labels: BTreeMap<String, String>,
}
impl Observed {
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.source.current(shell)
    }
    pub(super) fn labels(&self) -> &BTreeMap<String, String> {
        &self.labels
    }
    pub(super) fn names(
        &self,
        row: &groups::Snapshot,
        normalize: impl Fn(&str) -> Result<String, ranking::Error>,
    ) -> Result<groups::Names, Error> {
        if !original_pair(&self.originals, row) {
            return Err(unavailable());
        }
        let field = self.fields.get(row.id()).ok_or_else(unavailable)?;
        groups::Names::observe(
            row,
            |_| Ok(field.lowered.clone()),
            |text| normalize(text).map_err(|_| unavailable()),
        )
    }
    pub(super) fn extend_fields(
        &self,
        fields: &mut BTreeSet<String>,
        collation: &mut BTreeSet<String>,
    ) -> Result<(), Error> {
        for row in self.source.rows() {
            extend(
                row,
                self.fields.get(row.id()).ok_or_else(unavailable)?,
                fields,
                collation,
            )?;
        }
        Ok(())
    }
}
fn original_pair(originals: &BTreeMap<String, groups::Snapshot>, row: &groups::Snapshot) -> bool {
    originals.get(row.id()).is_some_and(|original| {
        original.source() == row.source() && original.effective() == row.effective()
    })
}
fn input(row: &groups::Snapshot) -> Result<Value, Error> {
    let raw = row.effective();
    let mut input = json!({"id":row.id(),"name":string(&raw["name"])?});
    // Missing and null have different literal-concatenation behavior in 5cyv.
    if let Some(handle) = raw.get("handle") {
        input["handle"] = handle.clone();
    }
    Ok(input)
}
fn decode(
    ticket: &str,
    stage: usize,
    locale: &str,
    inputs: &[Value],
    reply: Reply,
) -> Result<Vec<Field>, Error> {
    render::same_locale(locale, &reply.locale)?;
    if reply.ticket != ticket
        || reply.stage != stage
        || reply.inputs != inputs
        || reply.fields.len() != inputs.len()
        || reply
            .fields
            .iter()
            .zip(inputs)
            .any(|(field, input)| input["id"].as_str() != Some(field.id.as_str()))
    {
        return Err(unavailable());
    }
    Ok(reply.fields)
}
pub(super) async fn capture(
    search: &Search,
    current: &impl Fn() -> bool,
    ticket: &str,
    locale: &str,
    stage: &mut usize,
    source: Arc<group_source::Snapshot>,
) -> Result<Observed, Error> {
    let inputs = source
        .rows()
        .iter()
        .map(input)
        .collect::<Result<Vec<_>, _>>()?;
    let mut fields = BTreeMap::new();
    for batch in inputs.chunks(128) {
        let reply: Reply = render::evaluate(
            search,
            current,
            "profileGroupStrings",
            json!({"ticket":ticket,"stage":*stage,"rows":batch}),
        )
        .await?;
        for field in decode(ticket, *stage, locale, batch, reply)? {
            if fields.insert(field.id.clone(), field).is_some() {
                return Err(unavailable());
            }
        }
        *stage = stage.checked_add(1).ok_or_else(unavailable)?;
    }
    let labels = fields
        .iter()
        .map(|(id, field)| (id.clone(), field.label.clone()))
        .collect();
    let originals = source
        .rows()
        .iter()
        .map(|row| (row.id().to_owned(), row.clone()))
        .collect();
    Ok(Observed {
        source,
        fields,
        labels,
        originals,
    })
}
fn extend(
    row: &groups::Snapshot,
    field: &Field,
    fields: &mut BTreeSet<String>,
    collation: &mut BTreeSet<String>,
) -> Result<(), Error> {
    if field.id != row.id() {
        return Err(unavailable());
    }
    let raw = row.effective();
    let name = string(&raw["name"])?;
    let stored = string(&raw["_name_lc"])?;
    let scoring = if stored.is_empty() {
        field.lowered.as_str()
    } else {
        stored
    };
    let handle = string(&raw["handle"])?;
    let stripped = handle.strip_prefix('@').unwrap_or(handle);
    let prefixed = if handle.is_empty() {
        String::new()
    } else {
        format!("@{stripped}")
    };
    let final_name = if name.is_empty() {
        raw["string"].as_str().unwrap_or("")
    } else {
        name
    };
    for text in [
        name,
        scoring,
        handle,
        stripped,
        prefixed.as_str(),
        final_name,
    ] {
        fields.insert(text.into());
        fields.extend(text.split('\n').map(str::to_owned));
    }
    let initial = if truth(&raw["is_usergroup"]) && !handle.is_empty() {
        handle
    } else {
        name
    };
    collation.insert(initial.into());
    // Ejs0's final name is actual item.name, independently of initial handle.
    collation.insert(final_name.into());
    Ok(())
}
