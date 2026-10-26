//! Bounded 20Mt singleton producers and 4OY3 reference-title history.
//! A decoded response is evidence of bytes/request ID, not account authority.
use super::*;

#[derive(Clone)]
pub struct FileReferenceResponse {
    id: String,
    order: ChannelMapOrder,
    source: Value,
    positive: bool,
}

impl FileReferenceResponse {
    /// The transport validates the request factory. Offline adapters use this
    /// same decoder; callers must still bind owner/read freshness before ingest.
    pub fn decode(requested_id: &str, bytes: &[u8]) -> Result<Self, Error> {
        if !valid_id(requested_id) {
            return Err(malformed());
        }
        let order = ChannelMapOrder::decode(bytes)?;
        let raw = order.carrier();
        let mut nodes = MAX_NODES;
        let mut budget = MAX_ROW_BYTES;
        bounded(raw, 0, &mut nodes, &mut budget)?;
        let (source, positive) = if raw["ok"] == true {
            if raw.get("error").is_some_and(|v| !v.is_null())
                || !raw["file"].is_object()
                || raw["file"]["id"] != requested_id
            {
                return Err(malformed());
            }
            let mut mapped = raw["file"].as_object().unwrap().clone();
            mapped.insert("is_deleted".into(), Value::Bool(false));
            (Value::Object(mapped), true)
        } else if raw["ok"] == false {
            // Exact error.data mapping in 20Mt. Do not infer these statuses from
            // transport/HTTP errors, arbitrary absent rows or a previous cache.
            let mut mapped = Map::new();
            mapped.insert("id".into(), requested_id.into());
            match raw["error"].as_str() {
                Some("not_visible") => {
                    for (from, to) in [("filetype", "mode"), ("title", "title")] {
                        if let Some(value) = raw.get(from) {
                            mapped.insert(to.into(), value.clone());
                        }
                    }
                    mapped.insert("file_access".into(), "not_visible".into());
                }
                Some("template_not_visible") => {
                    mapped.insert("filetype".into(), "quip".into());
                    mapped.insert("mode".into(), "template_not_visible".into());
                }
                Some("file_deleted") => {
                    mapped.insert("mode".into(), "tombstone".into());
                    mapped.insert("is_deleted".into(), true.into());
                }
                _ => return Err(malformed()),
            }
            (Value::Object(mapped), false)
        } else {
            return Err(malformed());
        };
        Ok(Self {
            id: requested_id.into(),
            order,
            source,
            positive,
        })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn raw(&self) -> &Value {
        self.order.carrier()
    }
    /// Exact mapped producer record; the complete response remains in raw().
    pub fn source(&self) -> &Value {
        &self.source
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReferenceValue<'a> {
    Normal(&'a Value),
    /// Canvas-only presentation: locked, not disabled. No title dependencies.
    PrivateCanvas,
    /// Exact deleted-response history: disabled, not locked. Canvas token kind
    /// supplies the presentation; no title dependencies are consulted.
    DeletedCanvas,
}

#[derive(Clone)]
pub struct ReferenceObservation {
    identity: Arc<()>,
    id: String,
    row: Arc<Row>,
}
impl ReferenceObservation {
    /// Only the direct, known-source event transition mints this provenance.
    /// Account/socket ingress authority remains the native owner's duty.
    pub fn deletion_event(&self) -> Option<&DeletionEvidence> {
        self.row.deletion.as_deref()
    }
    pub fn is_event_deletion(&self) -> bool {
        self.deletion_event().is_some()
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn receipt(&self) -> &Value {
        &self.row.receipt
    }
    pub fn source(&self) -> &Value {
        &self.row.source
    }
    pub fn effective(&self) -> &Value {
        self.row.reference.as_ref().expect("reference observation")
    }
    pub fn value(&self) -> ReferenceValue<'_> {
        if self.effective()["mode"] == "tombstone" {
            ReferenceValue::DeletedCanvas
        } else if private(self.effective()) {
            ReferenceValue::PrivateCanvas
        } else {
            ReferenceValue::Normal(self.effective())
        }
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
    pub fn reference(&self, id: &str) -> Result<ReferenceObservation, Error> {
        let row = self
            .rows
            .get(id)
            .filter(|r| r.reference.is_some())
            .ok_or_else(unavailable)?;
        Ok(ReferenceObservation {
            identity: self.identity.clone(),
            id: id.into(),
            row: row.clone(),
        })
    }
    /// Return only the observation of this newly committed receipt. Malformed
    /// receipts cannot fall back to an older category/label. Valid unsupported
    /// records retire old observations and retain their ordinary poison policy.
    pub fn ingest_reference(
        &mut self,
        team: &str,
        response: &FileReferenceResponse,
    ) -> Result<ReferenceObservation, Error> {
        if team.is_empty() {
            return Err(malformed());
        }
        let raw_source = if response.positive {
            &response.raw()["file"]
        } else {
            response.source()
        };
        self.ingest_mapped(
            response.raw(),
            raw_source,
            response
                .source()
                .as_object()
                .expect("decoded mapped record"),
            Some(team),
            response.order.info_keys(),
            !response.positive && response.raw()["error"] == "file_deleted",
        )?;
        self.reference(response.id())
    }
}

fn private(row: &Value) -> bool {
    ["mode", "file_access"].iter().any(|key| {
        matches!(
            row[*key].as_str(),
            Some("not_visible" | "template_not_visible")
        )
    })
}

// Status precedence is intentionally narrower than all SRWA statuses. A raw
// deleted/tombstone/unknown/unported restriction never becomes Private canvas.
fn private_supported(row: &Value) -> bool {
    private(row)
        && ["mode", "file_access"]
            .iter()
            .all(|key| row.get(*key).is_none_or(|v| v.is_null() || v.is_string()))
        && row["mode"] != "tombstone"
        && ![
            "is_deleted",
            "isNonExistent",
            "isUnknown",
            "is_tombstoned",
            "is_hidden_by_limit",
            "canvas_template_mode",
            "list_template_mode",
            "is_external",
            "is_skill",
            "is_huddle_canvas",
            "is_huddle_transcript_canvas",
        ]
        .iter()
        .any(|key| truth(&row[*key]))
        && row
            .get("restriction_type")
            .is_none_or(|v| v.is_null() || v.as_f64() == Some(0.0))
}

pub(super) fn project(
    incoming: &Map<String, Value>,
    previous: Option<&Row>,
    deleted_response: bool,
) -> Option<Value> {
    let raw = Value::Object(incoming.clone());
    // Only the validated files.info error producer can originate this branch.
    // Raw flags, positive is_deleted and unrelated APIs do not mint deletion.
    // 4OY3 returns the incoming tombstone without any predecessor fields.
    if deleted_response {
        return Some(raw);
    }
    if raw["mode"] == "tombstone" {
        return None;
    }
    // 4OY3 passes an incoming not-visible record through, without prior fields.
    // This branch precedes the predecessor lookup, including unsupported prior
    // history. Only this exact incoming record establishes fresh evidence.
    if private(&raw) {
        return private_supported(&raw).then_some(raw);
    }
    if previous.is_some_and(|row| row.reference.is_none()) {
        return None;
    }
    let previous = previous.and_then(|row| row.reference.as_ref());
    // Status precedes normal title/kind validation. In particular a positive
    // {id,updated} can retain blocked access without any defined kind or title.
    let projected = super::merge_title_fields(incoming, previous);
    if projected["mode"] == "tombstone" {
        // A partial positive clears is_deleted but does not invent a mode.
        // Retain only a previously proved tombstone with supported new facts.
        return deleted_supported(&projected).then_some(projected);
    }
    if private(&projected) {
        return private_supported(&projected).then_some(projected);
    }
    // Normal admission and all unported predecessor guards are unchanged.
    super::project(incoming, previous).ok()
}

fn deleted_supported(row: &Value) -> bool {
    let mut checked = row.clone();
    let fields = checked.as_object_mut().expect("projected object");
    fields.remove("mode");
    fields.remove("is_deleted");
    super::ordinary(&checked, true).is_ok()
        && row
            .get("is_deleted")
            .is_none_or(|v| v.is_null() || v.is_boolean())
        && ![
            "is_tombstoned",
            "canvas_template_mode",
            "list_template_mode",
        ]
        .iter()
        .any(|key| truth(&row[*key]))
        && row
            .get("restriction_type")
            .is_none_or(|v| v.is_null() || v.as_f64() == Some(0.0))
}

#[cfg(test)]
mod deleted_tests;
#[cfg(test)]
mod tests;
