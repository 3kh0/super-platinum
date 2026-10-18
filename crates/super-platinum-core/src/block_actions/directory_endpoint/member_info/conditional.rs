//! 2gUC conditional users/info fields from actual retained member preimages.
use super::*;
use crate::block_actions::workflow_management::member_hydration::Current;

#[cfg(test)]
mod tests;

const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Conditional {
    updated: BTreeMap<String, u64>,
    cannot_interact: Vec<String>,
    disconnected: Vec<String>,
}
impl Read {
    /// Captures only requested, known source rows, in request encounter order.
    /// `Current::interaction == None` is observed absence (th9D returns true),
    /// not an unknown observation. Missing or duplicate requested rows refuse.
    /// This is a request plan, not proof of current account/source ownership.
    pub fn conditional(ids: Vec<String>, rows: &[Current]) -> Result<Self, Error> {
        let mut read = Self::capture(ids)?;
        let mut conditional = Conditional {
            updated: BTreeMap::new(),
            cannot_interact: Vec::new(),
            disconnected: Vec::new(),
        };
        let requested: BTreeSet<_> = read.ids.iter().map(String::as_str).collect();
        let mut found = BTreeMap::new();
        for row in rows {
            if let Some(id) = row.source["id"].as_str()
                && requested.contains(id)
                && found.insert(id, row).is_some()
            {
                return Err(unavailable());
            }
        }
        for id in &read.ids {
            let row = found.get(id.as_str()).ok_or_else(unavailable)?;
            let version = version(row.source.get("updated"))?;
            conditional.updated.insert(id.clone(), version);
            if version != 0 {
                if row.interaction == Some(false) {
                    conditional.cannot_interact.push(id.clone());
                }
                // etUg.NB is already established by capture's supported ID.
                // p28t.q uses raw isExternal && deleted, not the ledger mark.
                if truthy(&row.source["isExternal"]) && truthy(&row.source["deleted"]) {
                    conditional.disconnected.push(id.clone());
                }
            }
        }
        read.conditional = Some(conditional);
        Ok(read)
    }
}
impl Conditional {
    pub(super) fn apply(&self, body: &mut Value) {
        body.as_object_mut().expect("object").remove("ids");
        body["updated_ids"] = json!(self.updated);
        if !self.cannot_interact.is_empty() {
            body["cannot_interact"] = json!(self.cannot_interact);
        }
        if !self.disconnected.is_empty() {
            body["disconnected"] = json!(self.disconnected);
        }
    }
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|v| v != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}
fn version(value: Option<&Value>) -> Result<u64, Error> {
    let Some(value) = value.filter(|v| truthy(v)) else {
        return Ok(0);
    };
    // Native supported boundary: observed nonnegative safe integer versions.
    // Do not coerce truthy strings, booleans, objects or malformed numbers.
    numeric_version(value).ok_or_else(unavailable)
}
fn numeric_version(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .filter(|v| *v <= MAX_SAFE_INTEGER)
        .or_else(|| {
            let number = value.as_f64()?;
            (number.is_finite()
                && number >= 0.0
                && number <= MAX_SAFE_INTEGER as f64
                && number.fract() == 0.0)
                .then_some(number as u64)
        })
}
pub(super) fn valid_version(value: &Value) -> bool {
    numeric_version(value).is_some()
}
pub(super) fn valid_ids(value: Option<&Value>, updated: &serde_json::Map<String, Value>) -> bool {
    let Some(value) = value else {
        return true;
    };
    let Some(ids) = value.as_array().filter(|ids| !ids.is_empty()) else {
        return false;
    };
    let mut seen = BTreeSet::new();
    ids.iter().all(|id| {
        id.as_str().is_some_and(|id| {
            seen.insert(id)
                && updated
                    .get(id)
                    .and_then(numeric_version)
                    .is_some_and(|v| v != 0)
        })
    })
}
