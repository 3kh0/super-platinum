//! Nonpersistent Profile title facts from actual singleton file receipts.
//!
//! This explicitly new cache starts empty. It is not Slack's global file store,
//! a restored cache, a file inventory, or authority for shares/actions/ranking.
//! Retained 4OY3 processFile supplies the supported projection; unresolved
//! resets poison that identity rather than reuse a formerly current label.
use crate::slack::Error;
use serde_json::{Map, Value};
use std::{collections::BTreeMap, sync::Arc};

mod batch;
mod candidate;
mod category;
mod info;
mod order;
pub use batch::{BatchEntry, BatchReceipt, CandidateObservation, Collection};
pub use category::{CategoryObservation, Exclusion};
pub use info::OrderedFileInfo;
pub use order::ChannelMapOrder;

const MAX_ROWS: usize = 128;
const MAX_ROW_BYTES: usize = 1_048_576;
const MAX_TOTAL_BYTES: usize = 8_388_608;
const MAX_NODES: usize = 10_000;
const MAX_DEPTH: usize = 32;

/// Title/status facts only. Other actual wire fields stay in `source` and must
/// not be treated as a processed file model by a later consumer.
const FIELDS: &[&str] = &[
    "id",
    "mode",
    "filetype",
    "subtype",
    "name",
    "title",
    "title_blocks",
    "isUnknown",
    "isNonExistent",
    "is_deleted",
    "is_tombstoned",
    "restriction_type",
    "is_locked",
    "is_hidden_by_limit",
    "file_access",
    "permalink",
    "updated",
    "created",
    "timestamp",
    "is_external",
    "external_type",
    "is_skill",
    "canvas_template_mode",
    "list_template_mode",
    "is_huddle_canvas",
    "is_huddle_transcript_canvas",
    "huddle_summary_id",
    "huddle_transcript_file_id",
];

fn unavailable() -> Error {
    Error::Api("profile_file_title_source_unavailable".into())
}
fn malformed() -> Error {
    Error::Api("profile_file_title_source_malformed".into())
}
fn truth(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(false) => false,
        Value::String(text) => !text.is_empty(),
        Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0),
        _ => true,
    }
}

struct Row {
    receipt: Arc<Value>,
    source: Value,
    effective: Option<Value>,
    candidate: Option<Value>,
    category: Option<Value>,
    channel_map_order: Option<Arc<Vec<String>>>,
    bytes: usize,
}

/// Clones retain the same immutable history witness. Replacing a row retires
/// that row's observations; receiving another file does not.
#[derive(Clone, Default)]
pub struct Store {
    identity: Arc<()>,
    rows: BTreeMap<String, Arc<Row>>,
    order: Vec<String>,
}

#[derive(Clone)]
pub struct Observation {
    identity: Arc<()>,
    id: String,
    row: Arc<Row>,
}
impl Observation {
    /// The complete original raw carrier, before 20Mt's files.info mapping.
    pub fn receipt(&self) -> &Value {
        &self.row.receipt
    }
    pub fn source(&self) -> &Value {
        &self.row.source
    }
    pub fn effective(&self) -> &Value {
        self.row.effective.as_ref().expect("observed available row")
    }
    pub fn current(&self, store: &Store) -> bool {
        Arc::ptr_eq(&self.identity, &store.identity)
            && store
                .rows
                .get(&self.id)
                .is_some_and(|row| Arc::ptr_eq(row, &self.row))
    }
}
impl Store {
    pub fn new() -> Self {
        Self::default()
    }
    /// Accept one raw file object or a positive files.info envelope. This does
    /// not implement batch processing: original batch rows use one preimage,
    /// and must not be simulated by a loop over this singleton API.
    ///
    /// Malformed/error receipts make no changes. A structurally valid but
    /// unsupported update revokes this ID and retains its actual raw receipt.
    /// Once poisoned, this history cannot be healed by guessing a full row;
    /// the owner must construct a genuinely new title cache.
    pub fn ingest(&mut self, received: &Value) -> Result<(), Error> {
        self.ingest_observed(received, None, None)
    }

    /// Original files.info singleton, including the actual team and same-byte
    /// channel_map order. This does not simulate the separate batch projector.
    pub fn ingest_info(&mut self, team: &str, received: &OrderedFileInfo) -> Result<(), Error> {
        if team.is_empty() {
            return Err(malformed());
        }
        self.ingest_observed(received.raw(), Some(team), received.order.info_keys())
    }

    fn ingest_observed(
        &mut self,
        received: &Value,
        team: Option<&str>,
        order: Option<&[String]>,
    ) -> Result<(), Error> {
        let (mut nodes, mut bytes) = (MAX_NODES, MAX_ROW_BYTES);
        bounded(received, 0, &mut nodes, &mut bytes)?;
        let envelope = received.as_object().ok_or_else(malformed)?;
        let info = envelope.contains_key("ok")
            || (!envelope.contains_key("id") && envelope.contains_key("file"));
        let raw = if info {
            if received["ok"] != true || received.get("error").is_some_and(|v| !v.is_null()) {
                return Err(malformed());
            }
            received.get("file").ok_or_else(malformed)?
        } else {
            received
        };
        let incoming = raw.as_object().ok_or_else(malformed)?;
        let id = raw["id"]
            .as_str()
            .filter(|id| valid_id(id))
            .ok_or_else(malformed)?;
        if !self.rows.contains_key(id) && self.rows.len() >= MAX_ROWS {
            return Err(malformed());
        }
        let previous = self.rows.get(id);
        // 20Mt fetchFileById's normal positive envelope overrides is_deleted.
        // Other outer response fields affect unported comments/content/shares,
        // not this title-facts projection. Keep the original receipt separately.
        let mut mapped = incoming.clone();
        if info {
            mapped.insert("is_deleted".into(), Value::Bool(false));
        }
        let effective = if previous.is_some_and(|row| row.effective.is_none()) {
            None
        } else {
            project(&mapped, previous.and_then(|row| row.effective.as_ref())).ok()
        };
        // Separate selector facts do not widen the existing title observation.
        let candidate = if previous.is_some_and(|row| row.candidate.is_none()) {
            None
        } else {
            candidate::project(
                &mapped,
                previous.and_then(|row| row.candidate.as_ref()),
                team,
                order,
            )
            .ok()
        };
        let channel_map_order = if incoming.contains_key("channel_map") {
            order.map(|keys| Arc::new(keys.to_vec()))
        } else {
            previous.and_then(|row| row.channel_map_order.clone())
        };
        let category = category::project(&mapped, previous.map(AsRef::as_ref));
        let bytes = received.to_string().len()
            + raw.to_string().len()
            + effective
                .as_ref()
                .map_or(0, |value| value.to_string().len());
        let bytes = bytes
            + candidate
                .as_ref()
                .map_or(0, |value| value.to_string().len());
        let bytes = bytes + category.as_ref().map_or(0, |v| v.to_string().len());
        let retained: usize = self
            .rows
            .iter()
            .filter(|(key, _)| key.as_str() != id)
            .map(|(_, row)| row.bytes)
            .sum();
        if retained + bytes > MAX_TOTAL_BYTES {
            return Err(malformed());
        }
        let available = effective.is_some();
        if !self.rows.contains_key(id) {
            self.order.push(id.into());
        }
        self.rows.insert(
            id.into(),
            Arc::new(Row {
                receipt: Arc::new(received.clone()),
                source: raw.clone(),
                effective,
                candidate,
                category,
                channel_map_order,
                bytes,
            }),
        );
        if available {
            Ok(())
        } else {
            Err(unavailable())
        }
    }
    pub fn row(&self, id: &str) -> Result<Observation, Error> {
        let row = self
            .rows
            .get(id)
            .filter(|row| row.effective.is_some())
            .ok_or_else(unavailable)?;
        Ok(Observation {
            identity: self.identity.clone(),
            id: id.into(),
            row: row.clone(),
        })
    }
    /// Raw receipt access is not available-title authority, including after an
    /// unsupported update poisoned a previously valid ID.
    pub fn source(&self, id: &str) -> Option<&Value> {
        self.rows.get(id).map(|row| &row.source)
    }
}

fn valid_id(id: &str) -> bool {
    id.len() >= 2
        && id.len() <= 255
        && id.starts_with('F')
        && id.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

fn project(incoming: &Map<String, Value>, previous: Option<&Value>) -> Result<Value, Error> {
    let raw = Value::Object(incoming.clone());
    // These unported reset branches can delete predecessor title fields. Do
    // not perform an ordinary merge and call it the source's effective row.
    for source in previous.into_iter().chain(std::iter::once(&raw)) {
        if ["canvas_template_mode", "list_template_mode"]
            .iter()
            .any(|key| truth(&source[*key]))
            || source
                .get("restriction_type")
                .is_some_and(|v| !v.is_null() && v.as_f64() != Some(0.0))
            || truth(&source["is_tombstoned"])
        {
            return Err(unavailable());
        }
    }
    let mut effective = previous
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for key in FIELDS {
        if let Some(value) = incoming.get(*key) {
            effective.insert((*key).into(), value.clone());
        }
    }
    // 4OY3 explicitly writes incoming undefined values: absence removes these
    // keys rather than retaining a previous lock or permalink.
    for key in ["is_locked", "permalink"] {
        if !incoming.contains_key(key) {
            effective.remove(key);
        }
    }
    effective.insert(
        "is_tombstoned".into(),
        Value::Bool(truth(&raw["is_tombstoned"])),
    );
    // computeIsUnknown is conditional on XNVs.getFileTimestamp, not merely ok.
    if ["updated", "created", "timestamp"]
        .iter()
        .any(|key| truth(&raw[*key]))
    {
        effective.insert("isUnknown".into(), Value::Bool(false));
        effective.insert("isNonExistent".into(), Value::Bool(false));
    }
    let effective = Value::Object(effective);
    ordinary(&raw, false)?;
    ordinary(&effective, true)?;
    let canvas = effective["mode"] == "quip" || effective["filetype"] == "quip";
    let list = (effective["mode"] == "list" || effective["filetype"] == "list")
        && effective["subtype"] != "poll";
    if !canvas && !list {
        return Err(unavailable());
    }
    Ok(effective)
}

fn ordinary(row: &Value, require_known: bool) -> Result<(), Error> {
    for key in [
        "mode",
        "filetype",
        "subtype",
        "name",
        "title",
        "file_access",
        "permalink",
        "external_type",
    ] {
        if row
            .get(key)
            .is_some_and(|value| !value.is_null() && !value.is_string())
        {
            return Err(unavailable());
        }
    }
    if row
        .get("title_blocks")
        .is_some_and(|value| !value.is_null() && !value.is_array())
    {
        return Err(unavailable());
    }
    if row
        .get("mode")
        .and_then(Value::as_str)
        .is_some_and(|mode| !matches!(mode, "quip" | "list"))
        || truth(&row["file_access"])
        || (require_known && truth(&row["isUnknown"]))
        || [
            "isNonExistent",
            "is_deleted",
            "is_hidden_by_limit",
            "is_external",
            "is_skill",
            "is_huddle_canvas",
            "is_huddle_transcript_canvas",
            "huddle_summary_id",
            "huddle_transcript_file_id",
            "canvas_metadata",
            "meeting_metadata",
            "isMigratedFromLegacyPost",
            "docsId",
        ]
        .iter()
        .any(|key| truth(&row[*key]))
    {
        return Err(unavailable());
    }
    Ok(())
}

fn bounded(value: &Value, depth: usize, nodes: &mut usize, bytes: &mut usize) -> Result<(), Error> {
    if depth > MAX_DEPTH || *nodes == 0 {
        return Err(malformed());
    }
    *nodes -= 1;
    match value {
        Value::String(text) => *bytes = bytes.checked_sub(text.len()).ok_or_else(malformed)?,
        Value::Array(values) => {
            for value in values {
                bounded(value, depth + 1, nodes, bytes)?;
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                *bytes = bytes.checked_sub(key.len()).ok_or_else(malformed)?;
                bounded(value, depth + 1, nodes, bytes)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests;
