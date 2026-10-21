//! Bounded canonical file display facts, from 4OY3 / kbkl / XNVs / GOOR.
//! This incomplete collection is neither a File model nor navigation authority.
//! Callers own the original raw carrier and account/read/event fences. JSON
//! omission represents source `undefined`; it never means an inherited permalink.
use serde_json::{Map, Value};
use std::{collections::BTreeMap, sync::Arc};

mod projection;
mod sidecar;
pub use projection::should_show_file;
pub use sidecar::{embedded_files, first_file_id};

const MAX_ROWS: usize = 4096;
const MAX_BATCH: usize = 1000;
const MAX_BATCH_BYTES: usize = 2 * 1024 * 1024;
const MAX_STORE_BYTES: usize = 8 * 1024 * 1024;
const FIELDS: &[&str] = &[
    "id",
    "name",
    "permalink",
    "mode",
    "file_access",
    "isNonExistent",
    "is_deleted",
    "is_tombstoned",
    "restriction_type",
    "is_hidden_by_limit",
    "isUnknown",
    "updated",
    "created",
    "timestamp",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Shape,
    Identity,
    Limit,
    Revision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Link<'a> {
    pub name: &'a str,
    pub permalink: &'a str,
}

#[derive(Debug)]
struct Batch {
    raw: Vec<Value>,
    bytes: usize,
}
#[derive(Debug)]
struct Row {
    id: String,
    revision: u64,
    batch: Arc<Batch>,
    index: usize,
    predecessor: Option<Value>,
    effective: Value,
}

/// Owned immutable witness. Replacing an ID retires its old witness even when
/// the replacement happens to have equal fields or follows invalidation.
#[derive(Clone, Debug)]
pub struct Observation(Arc<Row>);
impl Observation {
    /// Ltli excludes Salesforce and XNVs tombstones from known-file freshness.
    /// Native deleted/tombstoned facts additionally refuse acquisition; this is
    /// eligibility only, never a freshness trigger or navigation authority.
    pub fn refreshable(&self) -> bool {
        !truth(&self.source()["is_salesforce_file"])
            && self.effective()["mode"] != "tombstone"
            && !["isNonExistent", "is_tombstoned", "is_deleted"]
                .iter()
                .any(|key| truth(&self.effective()[*key]))
    }
    pub fn id(&self) -> &str {
        &self.0.id
    }
    pub fn revision(&self) -> u64 {
        self.0.revision
    }
    pub fn source(&self) -> &Value {
        &self.0.batch.raw[self.0.index]
    }
    pub fn batch(&self) -> &[Value] {
        &self.0.batch.raw
    }
    pub fn predecessor(&self) -> Option<&Value> {
        self.0.predecessor.as_ref()
    }
    pub fn effective(&self) -> &Value {
        &self.0.effective
    }
    pub fn current(&self, store: &Store) -> bool {
        store
            .rows
            .get(self.id())
            .is_some_and(|row| Arc::ptr_eq(row, &self.0))
    }
    pub fn link(&self) -> Option<Link<'_>> {
        projection::link(self.effective())
    }
}

#[derive(Clone, Debug, Default)]
pub struct Store {
    rows: BTreeMap<String, Arc<Row>>,
    revision: u64,
}
impl Store {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn get(&self, id: &str) -> Option<Observation> {
        self.rows.get(id).cloned().map(Observation)
    }
    /// Atomic batch. Every occurrence uses the same pre-batch predecessor;
    /// assignment is ordered, so the last duplicate replaces earlier results.
    pub fn observe_batch(&mut self, files: &[Value]) -> Result<(), Error> {
        if files.len() > MAX_BATCH {
            return Err(Error::Limit);
        }
        let bytes = serde_json::to_vec(files).map_err(|_| Error::Shape)?.len();
        if bytes > MAX_BATCH_BYTES {
            return Err(Error::Limit);
        }
        for file in files {
            raw_id(file)?;
        }
        if files.is_empty() {
            return Ok(());
        }
        let revision = self.revision.checked_add(1).ok_or(Error::Revision)?;
        let batch = Arc::new(Batch {
            raw: files.to_vec(),
            bytes,
        });
        let mut rows = self.rows.clone();
        for (index, file) in files.iter().enumerate() {
            let id = raw_id(file)?;
            let predecessor = self.rows.get(id).map(|row| row.effective.clone());
            let effective = projection::project(predecessor.as_ref(), file);
            rows.insert(
                id.to_owned(),
                Arc::new(Row {
                    id: id.to_owned(),
                    revision,
                    batch: batch.clone(),
                    index,
                    predecessor,
                    effective,
                }),
            );
        }
        if rows.len() > MAX_ROWS || retained_bytes(&rows)? > MAX_STORE_BYTES {
            return Err(Error::Limit);
        }
        self.rows = rows;
        self.revision = revision;
        Ok(())
    }
    /// Also advances for an absent ID so an in-flight absent-row reader can
    /// fence against `revision()`. This does not invent a deleted source row.
    pub fn invalidate(&mut self, id: &str) -> Result<(), Error> {
        valid_id(id)?;
        let revision = self.revision.checked_add(1).ok_or(Error::Revision)?;
        self.rows.remove(id);
        self.revision = revision;
        Ok(())
    }
}

fn retained_bytes(rows: &BTreeMap<String, Arc<Row>>) -> Result<usize, Error> {
    let mut batches = std::collections::BTreeSet::new();
    let mut total = 0usize;
    for row in rows.values() {
        if batches.insert(Arc::as_ptr(&row.batch) as usize) {
            total += row.batch.bytes;
        }
        total += serde_json::to_vec(&(&row.effective, &row.predecessor))
            .map_err(|_| Error::Shape)?
            .len();
        if total > MAX_STORE_BYTES {
            return Err(Error::Limit);
        }
    }
    Ok(total)
}
fn valid_id(id: &str) -> Result<(), Error> {
    if id.starts_with('F')
        && id.len() > 1
        && id.len() <= 128
        && id.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        Ok(())
    } else {
        Err(Error::Identity)
    }
}
fn raw_id(file: &Value) -> Result<&str, Error> {
    let id = file
        .as_object()
        .ok_or(Error::Shape)?
        .get("id")
        .and_then(Value::as_str)
        .ok_or(Error::Identity)?;
    valid_id(id)?;
    Ok(id)
}
fn facts(raw: &Value) -> Map<String, Value> {
    FIELDS
        .iter()
        .filter_map(|key| raw.get(*key).map(|v| ((*key).into(), v.clone())))
        .collect()
}
fn truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        _ => true,
    }
}

#[cfg(test)]
mod known_tests;
#[cfg(test)]
mod tests;
