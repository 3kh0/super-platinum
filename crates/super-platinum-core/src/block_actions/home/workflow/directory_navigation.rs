//! Retained 53Yl IA history reducer and l24D navigation-history projection.
//! This module owns no IO, clock, navigation hook, account or storage namespace.
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

mod number;

pub const SOURCE_STORAGE_KEY: &str = "client-ia-history";
pub const PUSH_LIMIT: usize = 1000;

/// Actual storage/JSON acquisition outcome. Unavailable acquisition is not one
/// of these observations and must be handled by the caller before initialization.
pub enum StorageRead {
    Missing,
    ReadFailed,
    InvalidJson,
    Parsed(Value),
}

#[derive(Debug, PartialEq)]
pub enum Error {
    /// Source JSON.parse accepts this, but it is not an operable stack. Do not
    /// replace valid non-array JSON with a fabricated observed empty history.
    NonArrayStorage(Value),
    /// Class-method assignment to a primitive entry would throw in the source.
    PrimitiveEntry { index: usize },
    /// OrdinaryToPrimitive cannot obtain a primitive from this JSON value.
    /// All preceding position assignments remain observable after this error.
    Coercion { index: usize, field: CoercionField },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoercionField {
    PrimaryView,
    Timestamp,
}

/// A handle has JavaScript object identity. Clone the handle to dispatch the
/// same entry again; constructing an equal payload creates a distinct object.
#[derive(Clone, Debug)]
pub struct Entry(Arc<Mutex<Value>>);
impl Entry {
    pub fn new(raw: Value) -> Self {
        Self(Arc::new(Mutex::new(raw)))
    }
    pub fn raw(&self) -> Value {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    pub fn same_object(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    fn strict_equal(&self, other: &Self) -> bool {
        if self.same_object(other) {
            return true;
        }
        let (a, b) = (self.raw(), other.raw());
        match (&a, &b) {
            (Value::Array(_) | Value::Object(_), _) | (_, Value::Array(_) | Value::Object(_)) => {
                false
            }
            (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
            _ => a == b,
        }
    }
}

/// Parent schedules these exact captured arrays, preserving timer order.
/// A write failure is reported by the parent; it does not roll back reducer state.
#[derive(Clone, Debug)]
pub enum Write {
    None,
    Deferred(DeferredWrite),
    Immediate(Value),
}
/// The source timer closes over a shallow array, not serialized JSON. A later
/// projection can mutate shared entry positions before this timer executes.
#[derive(Clone, Debug)]
pub struct DeferredWrite(Vec<Entry>);
impl DeferredWrite {
    pub fn snapshot_when_run(&self) -> Value {
        Value::Array(self.0.iter().map(Entry::raw).collect())
    }
}

#[derive(Debug)]
pub struct History {
    stack: Vec<Entry>,
}
impl History {
    pub fn initialize(read: StorageRead) -> Result<Self, Error> {
        let raw = match read {
            StorageRead::Missing | StorageRead::ReadFailed | StorageRead::InvalidJson => vec![],
            StorageRead::Parsed(Value::Array(rows)) => rows,
            StorageRead::Parsed(other) => return Err(Error::NonArrayStorage(other)),
        };
        // Source restoration neither caps nor deduplicates. JSON restoration
        // allocates a fresh object for each row, even when payloads are equal.
        Ok(Self {
            stack: raw.into_iter().map(Entry::new).collect(),
        })
    }
    pub fn entries(&self) -> &[Entry] {
        &self.stack
    }
    pub fn snapshot(&self) -> Value {
        Value::Array(self.stack.iter().map(Entry::raw).collect())
    }
    /// `excluded_primary_views` is actual Object.values(XLMm.mH), not a guessed
    /// list of non-channel views. Source accepts other non-channel entries.
    pub fn push(&mut self, entry: Option<Entry>, excluded_primary_views: &[Value]) -> Write {
        let Some(entry) = entry else {
            return Write::None;
        };
        let raw = entry.raw();
        if !truthy(&raw)
            || raw
                .get("primaryView")
                .is_some_and(|v| excluded_primary_views.contains(v))
        {
            return Write::None;
        }
        self.stack.insert(0, entry);
        self.stack.truncate(PUSH_LIMIT);
        Write::Deferred(DeferredWrite(self.stack.clone()))
    }
    /// Remove only the first occurrence of this exact object reference.
    pub fn remove(&mut self, entry: &Entry) -> Write {
        let Some(index) = self.stack.iter().position(|v| v.strict_equal(entry)) else {
            return Write::None;
        };
        self.stack.remove(index);
        Write::Deferred(DeferredWrite(self.stack.clone()))
    }
    /// Source removeAllHistoryEntries keeps stack.slice(0,1).
    pub fn remove_all(&mut self) -> Write {
        self.stack.truncate(1);
        Write::Deferred(DeferredWrite(self.stack.clone()))
    }
    /// Source deleteHistoryStack clears everything and writes synchronously.
    pub fn delete_stack(&mut self) -> Write {
        self.stack.clear();
        Write::Immediate(self.snapshot())
    }
    /// Actual Sonic, model team and T68h app-metadata capability observations.
    /// Positions are assigned to every entry BEFORE filtering. Aliased entries
    /// therefore share their last assigned position, as in the shallow JS copy.
    pub fn project(
        &mut self,
        sonic: bool,
        model_team: Option<&str>,
        multi_team_capable: bool,
    ) -> Result<BTreeMap<String, Navigation>, Error> {
        let mut result = BTreeMap::new();
        if !sonic {
            return Ok(result);
        }
        for (index, entry) in self.stack.iter().enumerate() {
            let mut raw = entry.0.lock().unwrap_or_else(|e| e.into_inner());
            match &mut *raw {
                Value::Object(map) => {
                    map.insert("position".into(), json!(index + 1));
                }
                // Arrays accept an extra JS property, but JSON serialization
                // ignores it and they have no primaryView eligible for output.
                Value::Array(_) => {}
                _ => return Err(Error::PrimitiveEntry { index }),
            }
        }
        // Source completes Array.filter for every row before its forEach reads
        // timestamps. A later primaryView coercion can fail before an earlier ts.
        let mut eligible = Vec::new();
        for (index, entry) in self.stack.iter().enumerate() {
            let raw = entry.raw();
            // this.teamId defaults null; missing entry.teamId is undefined and
            // does not equal it. Explicit null does equal a null model team.
            let team_matches = match (raw.get("teamId"), model_team) {
                (Some(Value::Null), None) => true,
                (Some(Value::String(team)), Some(model)) => team == model,
                _ => false,
            };
            if !multi_team_capable && !team_matches {
                continue;
            }
            let Some(view) = raw.get("primaryView").filter(|v| truthy(v)) else {
                continue;
            };
            let id = number::string(view).map_err(|()| Error::Coercion {
                index,
                field: CoercionField::PrimaryView,
            })?;
            if channel_id(&id) {
                eligible.push((index, id, raw));
            }
        }
        for (index, id, raw) in eligible {
            if result.contains_key(&id) {
                continue;
            }
            result.insert(
                id,
                Navigation {
                    ts: number::number(raw.get("ts")).map_err(|()| Error::Coercion {
                        index,
                        field: CoercionField::Timestamp,
                    })? / 1000.0,
                    position: raw["position"].as_u64().unwrap() as usize,
                },
            );
        }
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Navigation {
    /// Seconds, including source NaN/Infinity when a malformed timestamp coerces
    /// that way. Do not replace with message/read timestamps or current time.
    pub ts: f64,
    pub position: usize,
}
/// JSON.stringify projection semantics: non-finite numeric timestamps become
/// null. Use typed Navigation when the consumer must distinguish NaN/Infinity.
pub fn projection_json(rows: &BTreeMap<String, Navigation>) -> Value {
    Value::Object(
        rows.iter()
            .map(|(id, row)| (id.clone(), json!({"ts":row.ts,"position":row.position})))
            .collect::<Map<_, _>>(),
    )
}
fn channel_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    bytes.len() >= 9
        && matches!(bytes[0], b'C' | b'G' | b'D' | b'Z')
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

#[cfg(test)]
mod tests;
