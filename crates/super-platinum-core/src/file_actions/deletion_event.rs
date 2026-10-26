//! Bounded xsCq file_deleted normalization, independent of any presentation.
//! Parsed event data is not account/socket authority; its owner must validate
//! ingress before applying it to a current, already-known source collection.
use crate::slack::Error;
use serde_json::Value;
use std::{collections::BTreeSet, sync::Arc};

const MAX_MESSAGES: usize = 128;
const MAX_IDS: usize = 1024;
const MAX_BYTES: usize = 1_048_576;
const MAX_NODES: usize = 10_000;

#[derive(Clone)]
pub struct FileDeletionEvent {
    raw: Arc<Value>,
    ids: Arc<Vec<String>>,
}
impl FileDeletionEvent {
    pub fn decode(raw: &Value) -> Result<Self, Error> {
        Self::parse(raw, std::slice::from_ref(raw))
    }
    pub fn batch(messages: &[Value]) -> Result<Self, Error> {
        if messages.len() > MAX_MESSAGES {
            return Err(malformed());
        }
        Self::parse(&Value::Array(messages.to_vec()), messages)
    }
    fn parse(raw: &Value, messages: &[Value]) -> Result<Self, Error> {
        let (mut nodes, mut bytes) = (MAX_NODES, MAX_BYTES);
        bounded(raw, 0, &mut nodes, &mut bytes)?;
        // Include JSON escaping/structural overhead in the actual retained cap.
        if serde_json::to_vec(raw).map_err(|_| malformed())?.len() > MAX_BYTES {
            return Err(malformed());
        }
        let mut ids = Vec::new();
        let mut seen = BTreeSet::new();
        for message in messages {
            if !message.is_object() || message["type"] != "file_deleted" {
                return Err(malformed());
            }
            // Nullish fallback, not truthy fallback: [] wins over file_id.
            let selected = message.get("file_ids").filter(|v| !v.is_null());
            match selected {
                Some(Value::Array(values)) => {
                    for value in values {
                        add(value, &mut seen, &mut ids)?;
                    }
                }
                Some(value) => add(value, &mut seen, &mut ids)?,
                None => add(&message["file_id"], &mut seen, &mut ids)?,
            }
        }
        Ok(Self {
            raw: Arc::new(raw.clone()),
            ids: Arc::new(ids),
        })
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn ids(&self) -> &[String] {
        &self.ids
    }
}
fn malformed() -> Error {
    Error::Api("file_deletion_event_malformed".into())
}
fn add(value: &Value, seen: &mut BTreeSet<String>, ids: &mut Vec<String>) -> Result<(), Error> {
    // compact after one-level flatten. Nested arrays and truthy non-ID values
    // are outside supported product IDs, not recursively flattened or coerced.
    if value.is_null() || value == false || value == "" || value.as_f64() == Some(0.0) {
        return Ok(());
    }
    let id = value
        .as_str()
        .filter(|id| {
            (2..=255).contains(&id.len())
                && id.starts_with('F')
                && id.bytes().all(|b| b.is_ascii_alphanumeric())
        })
        .ok_or_else(malformed)?;
    if seen.insert(id.to_owned()) {
        if ids.len() == MAX_IDS {
            return Err(malformed());
        }
        ids.push(id.to_owned());
    }
    Ok(())
}
fn bounded(value: &Value, depth: usize, nodes: &mut usize, bytes: &mut usize) -> Result<(), Error> {
    if depth > 32 || *nodes == 0 {
        return Err(malformed());
    }
    *nodes -= 1;
    match value {
        Value::String(s) => *bytes = bytes.checked_sub(s.len()).ok_or_else(malformed)?,
        Value::Array(a) => {
            for v in a {
                bounded(v, depth + 1, nodes, bytes)?;
            }
        }
        Value::Object(o) => {
            for (k, v) in o {
                *bytes = bytes.checked_sub(k.len()).ok_or_else(malformed)?;
                bounded(v, depth + 1, nodes, bytes)?;
            }
        }
        _ => (),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn exact_nullish_precedence_flatten_compact_dedup_and_raw_custody() {
        let events = vec![
            json!({"type":"file_deleted","file_ids":["F1",null,false,0,"","F1","F2"],"file_id":"Fignored"}),
            json!({"type":"file_deleted","file_ids":[],"file_id":"Fignored"}),
            json!({"type":"file_deleted","file_ids":null,"file_id":"F3"}),
            json!({"type":"file_deleted","file":{"id":"Fignored"}}),
            json!({"type":"file_deleted","file_ids":"F4"}),
        ];
        let event = FileDeletionEvent::batch(&events).unwrap();
        assert_eq!(event.ids(), &["F1", "F2", "F3", "F4"]);
        assert_eq!(event.raw(), &Value::Array(events));
    }
    #[test]
    fn malformed_or_excessive_events_never_grant_parsed_ids() {
        for raw in [
            Value::Null,
            json!({"type":"file_change","file_id":"F1"}),
            json!({"type":"file_deleted","file_ids":[["F1"]]}),
            json!({"type":"file_deleted","file_ids":["F1",true]}),
            json!({"type":"file_deleted","file_id":"C1"}),
            json!({"type":"file_deleted","file_id":"F bad"}),
            json!({"type":"file_deleted","file_id":"F1","opaque":"x".repeat(MAX_BYTES+1)}),
        ] {
            assert!(FileDeletionEvent::decode(&raw).is_err());
        }
        assert!(
            FileDeletionEvent::batch(&vec![json!({"type":"file_deleted"}); MAX_MESSAGES + 1])
                .is_err()
        );
    }
}
