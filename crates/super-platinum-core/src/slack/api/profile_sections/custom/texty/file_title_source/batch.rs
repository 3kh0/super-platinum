//! Original 4OY3 roots-plus-embedded batch, then kbkl ordered assignment.
use super::*;
use crate::config::WorkspaceSession;

#[derive(Clone)]
pub struct CandidateObservation {
    identity: Arc<()>,
    id: String,
    row: Arc<Row>,
}
impl CandidateObservation {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn source(&self) -> &Value {
        &self.row.source
    }
    pub fn receipt(&self) -> &Value {
        &self.row.receipt
    }
    /// Original observed ordering for the effective inherited channel_map, if known.
    pub fn channel_map_keys(&self) -> Option<&[String]> {
        self.row.channel_map_order.as_deref().map(Vec::as_slice)
    }
    /// Only supported selector facts. Never a complete processed File model.
    pub fn effective(&self) -> &Value {
        self.row.candidate.as_ref().expect("candidate")
    }
    pub fn current(&self, store: &Store) -> bool {
        Arc::ptr_eq(&self.identity, &store.identity)
            && store
                .rows
                .get(&self.id)
                .is_some_and(|r| Arc::ptr_eq(r, &self.row))
    }
}
#[derive(Clone)]
pub struct Collection {
    identity: Arc<()>,
    all: BTreeMap<String, Arc<Row>>,
    order: Vec<String>,
    rows: Vec<CandidateObservation>,
    unavailable: Vec<String>,
    excluded: Vec<CategoryObservation>,
}
impl Collection {
    /// Source insertion order; this is a bounded observed cache, not inventory.
    pub fn rows(&self) -> &[CandidateObservation] {
        &self.rows
    }
    pub fn unavailable_ids(&self) -> &[String] {
        &self.unavailable
    }
    pub fn excluded(&self) -> &[CategoryObservation] {
        &self.excluded
    }
    pub fn excluded_ids(&self) -> impl Iterator<Item = &str> {
        self.excluded.iter().map(|row| row.id())
    }
    pub fn current(&self, store: &Store) -> bool {
        Arc::ptr_eq(&self.identity, &store.identity)
            && self.order == store.order
            && self.all.len() == store.rows.len()
            && self
                .all
                .iter()
                .all(|(id, row)| store.rows.get(id).is_some_and(|r| Arc::ptr_eq(row, r)))
    }
}
pub struct BatchEntry {
    id: String,
    source: Value,
    predecessor: Option<Arc<Row>>,
    projected: Arc<Row>,
    root: usize,
    embedded: Option<usize>,
}
impl BatchEntry {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn source(&self) -> &Value {
        &self.source
    }
    pub fn preimage(&self) -> Option<&Value> {
        self.predecessor.as_ref().map(|r| &r.source)
    }
    pub fn preimage_effective(&self) -> Option<&Value> {
        self.predecessor.as_ref().and_then(|r| r.candidate.as_ref())
    }
    pub fn effective(&self) -> Option<&Value> {
        self.projected.candidate.as_ref()
    }
    pub fn title_effective(&self) -> Option<&Value> {
        self.projected.effective.as_ref()
    }
    pub fn root_index(&self) -> usize {
        self.root
    }
    pub fn embedded_index(&self) -> Option<usize> {
        self.embedded
    }
}
pub struct BatchReceipt {
    carrier: Arc<Value>,
    entries: Vec<BatchEntry>,
}
impl BatchReceipt {
    pub fn carrier(&self) -> &Value {
        &self.carrier
    }
    /// Every occurrence, even duplicate IDs and unavailable projections.
    pub fn entries(&self) -> &[BatchEntry] {
        &self.entries
    }
}
impl Store {
    pub fn candidate(&self, id: &str) -> Result<CandidateObservation, Error> {
        let row = self
            .rows
            .get(id)
            .filter(|r| r.candidate.is_some())
            .ok_or_else(unavailable)?;
        Ok(CandidateObservation {
            identity: self.identity.clone(),
            id: id.into(),
            row: row.clone(),
        })
    }
    pub fn candidates(&self) -> Collection {
        let mut rows = Vec::new();
        let mut unavailable = Vec::new();
        let mut excluded = Vec::new();
        for id in &self.order {
            match self.candidate(id) {
                Ok(row) => rows.push(row),
                Err(_) => match self.category(id) {
                    Ok(category) if category.exclusion().is_some() => excluded.push(category),
                    _ => unavailable.push(id.clone()),
                },
            }
        }
        Collection {
            identity: self.identity.clone(),
            all: self.rows.clone(),
            order: self.order.clone(),
            rows,
            unavailable,
            excluded,
        }
    }
    /// Accepted search.autocomplete.files carrier and its exact decoded array.
    /// The owner supplies actual current team for 4OY3 channel-map projection;
    /// account/transport/query admission remains the native caller's duty.
    ///
    /// Malformed batches make no changes. Valid unsupported rows are retained
    /// as unavailable. Other valid siblings still publish. Every projection
    /// reads the same original store, including duplicates and embedded rows.
    pub fn ingest_batch(
        &mut self,
        carrier: &Value,
        rows: &[Value],
        owner: &WorkspaceSession,
    ) -> Result<BatchReceipt, Error> {
        self.ingest_batch_inner(carrier, rows, owner, None)
    }
    pub fn ingest_batch_ordered(
        &mut self,
        carrier: &Value,
        rows: &[Value],
        owner: &WorkspaceSession,
        order: &ChannelMapOrder,
    ) -> Result<BatchReceipt, Error> {
        if !order.matches(carrier) {
            return Err(malformed());
        }
        self.ingest_batch_inner(carrier, rows, owner, Some(order))
    }
    fn ingest_batch_inner(
        &mut self,
        carrier: &Value,
        rows: &[Value],
        owner: &WorkspaceSession,
        observed_order: Option<&ChannelMapOrder>,
    ) -> Result<BatchReceipt, Error> {
        if carrier["ok"] != true
            || carrier.get("error").is_some_and(|v| !v.is_null())
            || carrier["files"].as_array().map(Vec::as_slice) != Some(rows)
            || owner.team_id.is_empty()
        {
            return Err(malformed());
        }
        let (mut nodes, mut bytes) = (MAX_NODES, MAX_TOTAL_BYTES);
        bounded(carrier, 0, &mut nodes, &mut bytes)?;
        if rows.len() > MAX_ROWS {
            return Err(malformed());
        }
        let mut expanded: Vec<(&Value, usize, Option<usize>)> =
            rows.iter().enumerate().map(|(i, r)| (r, i, None)).collect();
        for (root, row) in rows.iter().enumerate() {
            if truth(&row["canvas_metadata"]["items"]) {
                let items = row["canvas_metadata"]["items"]
                    .as_array()
                    .ok_or_else(malformed)?;
                for (index, item) in items.iter().enumerate() {
                    if truth(&item["file"]) {
                        expanded.push((&item["file"], root, Some(index)))
                    }
                }
            }
        }
        if expanded.len() > MAX_ROWS {
            return Err(malformed());
        }
        let carrier_bytes = carrier.to_string().len();
        let receipt = Arc::new(carrier.clone());
        let mut entries = Vec::new();
        let mut final_rows = self.rows.clone();
        let mut order = self.order.clone();
        for (raw, root, embedded) in expanded {
            let (mut nodes, mut bytes) = (MAX_NODES, MAX_ROW_BYTES);
            bounded(raw, 0, &mut nodes, &mut bytes)?;
            let incoming = raw.as_object().ok_or_else(malformed)?;
            let id = raw["id"]
                .as_str()
                .filter(|id| valid_id(id))
                .ok_or_else(malformed)?;
            // Intentionally self.rows, never final_rows: source pre-batch view.
            let previous = self.rows.get(id);
            let incoming_order = observed_order.and_then(|order| order.keys(root, embedded));
            let channel_map_order = if incoming.contains_key("channel_map") {
                incoming_order.map(|keys| Arc::new(keys.to_vec()))
            } else {
                previous.and_then(|row| row.channel_map_order.clone())
            };
            let candidate = if previous.is_some_and(|r| r.candidate.is_none()) {
                None
            } else {
                candidate::project(
                    incoming,
                    previous.and_then(|r| r.candidate.as_ref()),
                    Some(&owner.team_id),
                    incoming_order,
                )
                .ok()
            };
            let mut title_input = incoming.clone();
            // Newly implemented metadata projection proves this field cannot
            // alter title facts. Preserve original singleton refusal behavior.
            if candidate.is_some() {
                title_input.remove("canvas_metadata");
            }
            let effective = if let Some(candidate) = &candidate {
                // The explicit batch API has processed metadata and selector
                // facts. Export only established title fields to old consumers.
                Some(Value::Object(
                    FIELDS
                        .iter()
                        .filter_map(|key| candidate.get(*key).map(|v| ((*key).into(), v.clone())))
                        .collect(),
                ))
            } else if previous.is_some_and(|r| r.effective.is_none()) {
                None
            } else {
                project(&title_input, previous.and_then(|r| r.effective.as_ref())).ok()
            };
            let category = category::project(incoming, previous.map(AsRef::as_ref));
            let bytes = category.as_ref().map_or(0, |v| v.to_string().len())
                + carrier_bytes
                + raw.to_string().len()
                + effective.as_ref().map_or(0, |v| v.to_string().len())
                + candidate.as_ref().map_or(0, |v| v.to_string().len());
            let projected = Arc::new(Row {
                receipt: receipt.clone(),
                source: raw.clone(),
                effective,
                candidate,
                category,
                channel_map_order,
                bytes,
            });
            if !final_rows.contains_key(id) {
                order.push(id.into())
            }
            final_rows.insert(id.into(), projected.clone());
            entries.push(BatchEntry {
                id: id.into(),
                source: raw.clone(),
                predecessor: previous.cloned(),
                projected,
                root,
                embedded,
            });
        }
        if final_rows.len() > MAX_ROWS
            || final_rows.values().map(|r| r.bytes).sum::<usize>() > MAX_TOTAL_BYTES
        {
            return Err(malformed());
        }
        self.rows = final_rows;
        self.order = order;
        Ok(BatchReceipt {
            carrier: receipt,
            entries,
        })
    }
}

#[cfg(test)]
mod tests;
