//! Channel-record portion of retained 340b/4XO+. This does not apply posting
//! permissions or external-team side effects and cannot authorize a selection.
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Shape,
    MissingObservation,
}
type Result<T> = std::result::Result<T, Error>;

/// These are observations at the original ingress, not inferred from the
/// workspace credential count. OrdinaryClientV2 follows the reviewed actual
/// constructor's unified=true and app=client policy.
pub struct Context<'a> {
    pub self_id: &'a str,
    pub current_team: &'a str,
    /// Exact v7Vi.Kc exclusion identity; not interchangeable with ZT used for
    /// member-name suffixes, particularly in an enterprise logical store.
    pub omit_team: &'a str,
    pub boot_in_flight: bool,
    pub retain_public_membership_on_archive: bool,
    pub members: &'a BTreeMap<String, Value>,
    /// Exact ordinary JS toLowerCase, captured by the renderer under the same
    /// owner and source snapshot. No locale-query normalization is used here.
    pub lowercase: &'a dyn Fn(&str) -> Option<String>,
}

pub struct Processed {
    before: Vec<Value>,
    incoming: Vec<Value>,
    rows: Vec<Value>,
    updates: Vec<Value>,
    actions: Vec<Value>,
}
impl Processed {
    pub(super) fn before(&self) -> &[Value] {
        &self.before
    }
    pub(super) fn incoming(&self) -> &[Value] {
        &self.incoming
    }
    pub(super) fn rows(&self) -> &[Value] {
        &self.rows
    }
    pub(super) fn updates(&self) -> &[Value] {
        &self.updates
    }
    pub(super) fn actions(&self) -> &[Value] {
        &self.actions
    }
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_some_and(|n| n != 0.0),
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}
pub(super) fn id(row: &Value) -> Result<&str> {
    row.get("id")
        .and_then(Value::as_str)
        .filter(|id| {
            id.len() > 1
                && id.len() <= 255
                && matches!(id.as_bytes()[0], b'C' | b'D' | b'G')
                && id.bytes().all(|b| b.is_ascii_alphanumeric())
        })
        .ok_or(Error::Shape)
}
fn object(v: &Value) -> Result<&Map<String, Value>> {
    v.as_object().ok_or(Error::Shape)
}
fn strings(v: Option<&Value>) -> Result<Vec<String>> {
    match v {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| v.as_str().map(str::to_owned).ok_or(Error::Shape))
            .collect(),
        _ => Err(Error::Shape),
    }
}
fn unique(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect()
}
fn unknown(v: &Value) -> bool {
    truth(&v["isUnknown"]) || truth(&v["isNonExistent"])
}
fn public(v: &Value) -> bool {
    truth(&v["is_channel"])
        && !truth(&v["is_mpim"])
        && !truth(&v["is_private"])
        && !truth(&v["is_group"])
}
fn name(v: &Value) -> Result<Option<&str>> {
    if truth(&v["isUnknown"]) {
        return Ok(None);
    }
    match v.get("name") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        _ => Err(Error::Shape),
    }
}
fn member_team<'a>(m: &'a Value, current: &'a str) -> Result<&'a str> {
    let teams = strings(m.pointer("/enterprise_user/teams"))?;
    if m["team_id"] == current
        || teams.iter().any(|id| id == current)
        || truth(&m["is_slackbot"])
        || m["id"] == "USLACKBOT"
    {
        return Ok(current);
    }
    match m.get("team_id") {
        None | Some(Value::Null) => Ok(""),
        Some(Value::String(s)) => Ok(s),
        _ => Err(Error::Shape),
    }
}
fn internal_names(row: &mut Value, context: &Context<'_>) -> Result<()> {
    if truth(&row["is_mpim"]) {
        let initial = if truth(&row["isUnknown"]) {
            Some(Value::Null)
        } else {
            row.get("name").cloned()
        };
        match &initial {
            Some(v) => row["_internal_name"] = v.clone(),
            None => {
                row.as_object_mut().unwrap().remove("_internal_name");
            }
        }
        if !truth(&row["properties"]["has_custom_mpdm_name"]) {
            let peers = strings(row.get("members"))?
                .into_iter()
                .filter(|id| id != context.self_id)
                .map(|id| context.members.get(&id).ok_or(Error::MissingObservation))
                .collect::<Result<Vec<_>>>()?;
            let generated = if peers.len() < 2 {
                initial
            } else {
                let mut names = Vec::new();
                for peer in peers {
                    let n = peer["name"].as_str().ok_or(Error::MissingObservation)?;
                    names.push(if truth(&peer["isExternal"]) {
                        format!("{n}_{}", member_team(peer, context.current_team)?)
                    } else {
                        n.into()
                    });
                }
                Some(format!("@{}", names.join(",")).into())
            };
            match generated {
                Some(n) => row["name"] = n,
                None => {
                    row.as_object_mut().unwrap().remove("name");
                }
            }
        }
    } else if truth(&row["is_im"]) {
        if !truth(&row["name"]) {
            if let Some(user) = row.get("user").cloned() {
                row["name"] = user;
            }
        }
        let peer = row["user"]
            .as_str()
            .and_then(|id| context.members.get(id))
            .ok_or(Error::MissingObservation)?;
        if !unknown(peer) {
            row["name"] = peer["name"]
                .as_str()
                .ok_or(Error::MissingObservation)?
                .into();
        }
    }
    let raw = if truth(&row["is_im"])
        || (truth(&row["is_mpim"]) && !truth(&row["properties"]["has_custom_mpdm_name"]))
    {
        match row.get("name") {
            None | Some(Value::Null) => "",
            Some(Value::String(s)) => s,
            _ => return Err(Error::Shape),
        }
    } else {
        name(row)?.unwrap_or("")
    };
    row["_name_lc"] = (context.lowercase)(raw)
        .ok_or(Error::MissingObservation)?
        .into();
    Ok(())
}

fn merged_before_names(before: Option<&Value>, incoming: &Value) -> Result<Value> {
    id(incoming)?;
    let mut wire = object(incoming)?.clone();
    for key in [
        "unread_count",
        "is_shared",
        "pending_connected_team_ids",
        "pending_shared",
        "shared_team_ids",
    ] {
        wire.remove(key);
    }
    let current = before.filter(|v| !unknown(v));
    let row = if let Some(old) = current {
        let mut next = object(old)?.clone();
        for (key, value) in &wire {
            match key.as_str() {
                "members" => {
                    if !truth(&incoming["is_mpim"]) {
                        continue;
                    }
                    let mut members = strings(Some(value))?;
                    members.extend(strings(next.get(key))?);
                    next.insert(key.clone(), serde_json::json!(unique(members)));
                }
                "name" if truth(&incoming["is_mpim"]) => {
                    if (truth(&incoming["properties"]["has_custom_mpdm_name"])
                        || truth(&old["properties"]["has_custom_mpdm_name"]))
                        && truth(value)
                    {
                        next.insert(key.clone(), value.clone());
                    }
                }
                "pinned_items_count" if !next.contains_key(key) && value == 0 => {}
                _ => {
                    if next.get(key).is_some_and(|old| acquired_same(old, value)) {
                        continue;
                    }
                    next.insert(key.clone(), value.clone());
                }
            }
        }
        Value::Object(next)
    } else {
        let mut next = Value::Object(wire);
        for key in ["is_general", "is_channel", "is_group", "is_mpim", "is_im"] {
            next[key] = truth(&next[key]).into();
        }
        if public(&next) {
            for key in ["is_org_mandatory", "is_org_default"] {
                next[key] = truth(&next[key]).into();
            }
        }
        for key in ["topic", "purpose"] {
            if !truth(&next[key]) {
                next[key] = serde_json::json!({});
            }
        }
        next["is_member"] = truth(&next["is_member"]).into();
        if truth(&next["is_im"]) || truth(&next["is_mpim"]) {
            next["opened_this_session"] = false.into();
            next["is_member"] = true.into();
        }
        if !truth(&next["scroll_top"]) {
            next["scroll_top"] = (-1).into();
        }
        next["history_is_being_fetched"] = truth(&next["history_is_being_fetched"]).into();
        for key in ["unread_highlights", "unreads"] {
            if !truth(&next[key]) {
                next[key] = serde_json::json!([]);
            }
        }
        next["has_fetched_history_after_scrollback"] = false.into();
        next
    };
    Ok(row)
}

fn process(before: Option<&Value>, incoming: &Value, c: &Context<'_>) -> Result<Value> {
    let current = before.filter(|v| !unknown(v));
    let mut row = merged_before_names(before, incoming)?;
    // The source merge preserves old array references when values are equal.
    // A still-present derived field is recomputed only when that raw companion
    // actually changes. A later context change alone is not a rewrite trigger.
    let recompute = |raw: &str, derived: &str| {
        current.is_none_or(|old| old.get(raw) != row.get(raw) || !truth(&row[derived]))
    };
    let internal_changed = recompute("internal_team_ids", "internalTeamIds");
    let connected_changed = recompute("connected_team_ids", "connectedTeamIds");
    let limited_changed = recompute("connected_limited_team_ids", "connectedLimitedTeamIds");
    if internal_changed {
        let internal = match row.get("internal_team_ids") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(a)) => a
                .iter()
                .filter(|v| !v.is_null())
                .map(|v| v.as_str().map(str::to_owned).ok_or(Error::Shape))
                .collect::<Result<Vec<_>>>()?,
            _ => return Err(Error::Shape),
        };
        row["internalTeamIds"] = serde_json::json!(internal);
    }
    if connected_changed {
        row["connectedTeamIds"] = serde_json::json!(unique(
            strings(row.get("connected_team_ids"))?
                .into_iter()
                .filter(|id| id != c.omit_team)
                .collect()
        ));
    }
    if limited_changed {
        row["connectedLimitedTeamIds"] =
            serde_json::json!(unique(strings(row.get("connected_limited_team_ids"))?));
    }
    internal_names(&mut row, c)?;
    if current.is_some() && c.boot_in_flight && truth(&row["is_migrating"]) {
        row["is_migrating"] = truth(&incoming["is_migrating"]).into();
    }
    if public(&row)
        && truth(&row["is_member"])
        && truth(&row["is_archived"])
        && !c.retain_public_membership_on_archive
    {
        row["is_member"] = false.into();
    }
    if truth(&row["name"]) {
        row["isUnknown"] = false.into();
        row["isNonExistent"] = false.into();
    }
    row["fromAnotherTeam"] = false.into(); // Actual ordinary client-v2 u79s.ss branch.
    row.as_object_mut().unwrap().remove("latest");
    row.as_object_mut().unwrap().remove("last_read");
    Ok(row)
}

/// Actual4XO+ processes every input against the same pre-dispatch collection.
/// Final bulk insertion keeps the first ID slot but takes the last processed row.
pub fn batch(before: &[Value], incoming: &[Value], context: &Context<'_>) -> Result<Processed> {
    if before.len() > 100_000
        || incoming.len() > 100_000
        || context.self_id.is_empty()
        || context.current_team.is_empty()
    {
        return Err(Error::Shape);
    }
    let mut indexes = BTreeMap::new();
    for (index, row) in before.iter().enumerate() {
        if indexes.insert(id(row)?.to_owned(), index).is_some() {
            return Err(Error::Shape);
        }
    }
    let mut updates = Vec::with_capacity(incoming.len());
    for row in incoming {
        updates.push(process(
            indexes.get(id(row)?).map(|i| &before[*i]),
            row,
            context,
        )?);
    }
    let actions = action_rows(before, incoming, &updates)?;
    let mut rows = before.to_vec();
    for row in &actions {
        let key = id(row)?;
        if let Some(index) = indexes.get(key) {
            rows[*index] = row.clone();
        } else {
            indexes.insert(key.into(), rows.len());
            rows.push(row.clone());
        }
    }
    Ok(Processed {
        before: before.to_vec(),
        incoming: incoming.to_vec(),
        rows,
        updates,
        actions,
    })
}

// Only the actual 4XO+ changed-object dispatch reaches the live alias reducer.
// Processed::updates retains every observed processor return for native repair;
// those are not all reducer actions. JSON has no shared references between an
// acquired wire object and the retained source object. 340b keeps the old slot
// when +URr shallowEqual succeeds, including flat primitive objects/arrays.
// A fresh nested reference can still cause an action despite equal JSON values.
fn action_rows(
    before_rows: &[Value],
    incoming: &[Value],
    observed: &[Value],
) -> Result<Vec<Value>> {
    let before: BTreeMap<_, _> = before_rows
        .iter()
        .map(|row| Ok((id(row)?, row)))
        .collect::<Result<_>>()?;
    if incoming.len() != observed.len() {
        return Err(Error::Shape);
    }
    let mut changed = Vec::new();
    for (wire, row) in incoming.iter().zip(observed) {
        let id = id(row)?;
        let Some(old) = before.get(id) else {
            changed.push(row.clone());
            continue;
        };
        if unknown(old) || !json_same(old, row) || fresh_nested_assignment(old, wire)? {
            changed.push(row.clone());
        }
    }
    Ok(changed)
}
fn json_same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| json_same(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, a)| b.get(k).is_some_and(|b| json_same(a, b)))
        }
        _ => a == b,
    }
}
fn reference(v: &Value) -> bool {
    v.is_array() || v.is_object()
}
fn primitive_same(a: &Value, b: &Value) -> bool {
    !reference(a) && !reference(b) && json_same(a, b)
}
// Actual +URr default shallowEqual, for separately acquired JSON values.
// Object/array child references cannot be shared with retained source objects.
fn acquired_same(a: &Value, b: &Value) -> bool {
    if primitive_same(a, b) {
        return true;
    }
    if !reference(a) || !reference(b) {
        return false;
    }
    fn entries(value: &Value) -> BTreeMap<String, &Value> {
        match value {
            Value::Object(object) => object.iter().map(|(k, v)| (k.clone(), v)).collect(),
            Value::Array(array) => array
                .iter()
                .enumerate()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            _ => BTreeMap::new(),
        }
    }
    let a = entries(a);
    let b = entries(b);
    a.len() == b.len()
        && a.iter()
            .all(|(key, value)| b.get(key).is_some_and(|other| primitive_same(value, other)))
}
fn fresh_nested_assignment(old: &Value, wire: &Value) -> Result<bool> {
    let object = wire.as_object().ok_or(Error::Shape)?;
    for (key, value) in object {
        if matches!(
            key.as_str(),
            "members"
                | "name"
                | "latest"
                | "last_read"
                | "unread_count"
                | "is_shared"
                | "pending_connected_team_ids"
                | "pending_shared"
                | "shared_team_ids"
        ) {
            continue;
        }
        let Some(previous) = old.get(key) else {
            continue;
        };
        // This helper is consulted for JSON-equal final rows only. An acquired
        // nested reference is distinct from the retained one even when contents
        // compare equal. Null and primitives retain JS value equality.
        if reference(value) && reference(previous) {
            if key == "pinned_items_count" {
                return Ok(true);
            }
            let nested = match value {
                Value::Array(values) => values.iter().any(reference),
                Value::Object(values) => values.values().any(reference),
                _ => false,
            };
            if nested {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

mod classification;
pub use classification::{
    ClassificationKind, ClassificationState, ClassificationStep, ClassifiedRow,
};
