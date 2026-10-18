//! Read-only projection of Slack's unified-draft identity map for ranking.
//! This owns no composer contents, edit/send permission, or durable draft receipt.
use crate::config::WorkspaceSession;
use crate::slack::{Error, PreparedRequest, SlackClient};
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod read;
pub use read::{Read, Step};

fn invalid() -> Error {
    Error::Api("draft_presence_unavailable".into())
}
fn text(v: &Value, key: &str) -> Result<String, Error> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 255 && !s.chars().any(char::is_control))
        .map(str::to_owned)
        .ok_or_else(invalid)
}
fn flag(v: &Value, key: &str) -> Result<bool, Error> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        _ => Err(invalid()),
    }
}
fn stamp(v: &Value, key: &str) -> Result<f64, Error> {
    let n = match &v[key] {
        Value::String(s) => s.parse().ok(),
        Value::Number(n) => n.as_f64(),
        _ => None,
    };
    n.filter(|n| n.is_finite() && *n >= 0.0).ok_or_else(invalid)
}
fn envelope(v: &Value) -> Result<(), Error> {
    if v["ok"] == true
        && v.get("error").is_none_or(Value::is_null)
        && v.to_string().len() <= 4 * 1024 * 1024
    {
        Ok(())
    } else {
        Err(invalid())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Record {
    id: String,
    key: String,
    unique: String,
    composer: bool,
    updated: f64,
}
impl Record {
    fn decode(v: &Value) -> Result<Self, Error> {
        let id = text(v, "id")?;
        let unique = text(v, "client_msg_id")?;
        let composer = flag(v, "is_from_composer")?;
        let key = if composer {
            unique.clone()
        } else {
            let first = v["destinations"]
                .as_array()
                .and_then(|v| v.first())
                .ok_or_else(invalid)?;
            let channel = text(first, "channel_id")?;
            match first.get("thread_ts") {
                None | Some(Value::Null) => channel,
                Some(Value::String(s)) if s.is_empty() => channel,
                Some(Value::String(s)) if s.len() <= 255 && !s.chars().any(char::is_control) => {
                    format!("{channel}-{s}")
                }
                _ => return Err(invalid()),
            }
        };
        Ok(Self {
            id,
            key,
            unique,
            composer,
            updated: stamp(v, "last_updated_ts")?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct Presence {
    // Insertion order matches unifiedDrafts. No body, files, or auth data is kept.
    records: Vec<Record>,
    keys: std::collections::BTreeSet<String>,
    revision: u64,
    read_generation: u64,
    versions: std::collections::BTreeMap<String, String>,
    retired: std::collections::BTreeSet<String>,
}
impl Presence {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// A caller-observed stream gap retires pending reads and rank snapshots,
    /// while retaining warm identities until an actual observation reconciles them.
    pub fn invalidate(&mut self) {
        self.read_generation = self.read_generation.wrapping_add(1);
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn has_channel(&self, channel: &str) -> bool {
        self.keys.contains(channel)
    }
    pub fn persisted(&self) -> Value {
        serde_json::to_value(&self.records).unwrap_or(Value::Null)
    }
    pub fn restore(v: Value) -> Self {
        let Ok(records) = serde_json::from_value::<Vec<Record>>(v) else {
            return Self::default();
        };
        let mut keys = std::collections::BTreeSet::new();
        let mut ids = std::collections::BTreeSet::new();
        let mut unique = std::collections::BTreeSet::new();
        if records.len() > 100_000
            || records.iter().any(|r| {
                r.id.is_empty()
                    || r.id.len() > 255
                    || r.id.chars().any(char::is_control)
                    || r.key.is_empty()
                    || r.key.len() > 511
                    || r.key.chars().any(char::is_control)
                    || r.unique.is_empty()
                    || r.unique.len() > 255
                    || r.unique.chars().any(char::is_control)
                    || (r.composer && r.key != r.unique)
                    || !r.updated.is_finite()
                    || r.updated < 0.0
                    || !keys.insert(r.key.clone())
                    || !ids.insert(r.id.clone())
                    || !unique.insert(r.unique.clone())
            })
        {
            return Self::default();
        }
        Self {
            records,
            keys,
            ..Self::default()
        }
    }
    fn changed(&mut self, before: Vec<Record>) -> bool {
        let changed = before != self.records;
        if changed {
            self.revision = self.revision.wrapping_add(1);
            self.keys = self.records.iter().map(|r| r.key.clone()).collect();
        }
        changed
    }
    /// Positive accepted server observation. It never clears a local DraftBook.
    pub fn observe(&mut self, raw: &Value) -> Result<bool, Error> {
        let before = self.records.clone();
        self.reconcile(raw, false)?;
        Ok(self.changed(before))
    }
    fn reconcile(&mut self, raw: &Value, known_update_only: bool) -> Result<(), Error> {
        let id = text(raw, "id")?;
        if flag(raw, "is_deleted")? || flag(raw, "is_sent")? {
            self.records.retain(|r| r.id != id);
            return Ok(());
        }
        let incoming = Record::decode(raw)?;
        if self.retired.contains(&id) {
            return Ok(());
        }
        if let Some(index) = self.records.iter().position(|r| r.id == id) {
            let old = &self.records[index];
            if incoming.updated <= old.updated {
                return Ok(());
            }
            // giv0 retains the current attached key except attached -> composer.
            let mut next = incoming;
            if !next.composer || old.composer {
                next.key = old.key.clone();
            }
            if next.key == old.key {
                self.records[index] = next;
            } else {
                self.records.remove(index);
                if let Some(target) = self.records.iter().position(|r| r.key == next.key) {
                    self.records[target] = next;
                } else {
                    self.records.push(next);
                }
            }
        } else if !known_update_only
            && !self
                .records
                .iter()
                .any(|r| r.key == incoming.key || r.unique == incoming.unique)
        {
            self.records.push(incoming);
        }
        Ok(())
    }
    pub fn retire_server_id(&mut self, id: &str) -> bool {
        self.retired.insert(id.into());
        let before = self.records.len();
        self.records.retain(|r| r.id != id);
        let changed = before != self.records.len();
        if changed {
            self.revision = self.revision.wrapping_add(1);
            self.keys = self.records.iter().map(|r| r.key.clone()).collect();
        }
        changed
    }
    /// Outer accepted socket/account generation is mandatory. Events are read
    /// observations only; no draft_send event becomes a delivery receipt.
    pub fn realtime(&mut self, team: &str, kind: &str, raw: &Value) -> Result<bool, Error> {
        if !matches!(
            kind,
            "draft_create" | "draft_update" | "draft_delete" | "draft_send"
        ) {
            return Ok(false);
        }
        let id = if matches!(kind, "draft_create" | "draft_update") {
            let id = text(&raw["draft"], "id")?;
            if raw
                .get("draft_id")
                .is_some_and(|v| v.as_str() != Some(id.as_str()))
                || raw["draft"]
                    .get("team_id")
                    .is_some_and(|v| v.as_str() != Some(team))
            {
                return Err(invalid());
            }
            id
        } else {
            text(raw, "draft_id")?
        };
        let event_ts = text(raw, "event_ts")?;
        if crate::state::ts_key(&event_ts).0 == 0 {
            return Err(invalid());
        }
        if self
            .versions
            .get(&id)
            .is_some_and(|old| crate::state::ts_key(old) >= crate::state::ts_key(&event_ts))
        {
            return Ok(false);
        }
        let before = self.records.clone();
        match kind {
            "draft_create" | "draft_update" => {
                if text(&raw["draft"], "id")? != id {
                    return Err(invalid());
                }
                self.reconcile(&raw["draft"], kind == "draft_update")?;
            }
            _ => {
                self.retired.insert(id.clone());
                self.records.retain(|r| r.id != id);
            }
        }
        self.versions.insert(id, event_ts);
        let changed = self.changed(before);
        // Accepted socket observations supersede an older list even when the
        // known-key rule leaves its projection unchanged. Event and payload
        // timestamps are independent clocks; they are never compared.
        if !changed {
            self.revision = self.revision.wrapping_add(1);
        }
        Ok(changed)
    }
    /// Already accepted scheduled rows enrich this same metadata store. A
    /// paginated queue page is never evidence that absent keys were deleted.
    pub fn observe_scheduled(
        &mut self,
        draft: &crate::scheduled::ScheduledMessage,
    ) -> Result<bool, Error> {
        self.observe(&serde_json::json!({"id":draft.id,"client_msg_id":draft.client_msg_id,
            "is_from_composer":draft.is_from_composer,"destinations":draft.destinations,
            "last_updated_ts":draft.last_updated_ts,"is_deleted":draft.is_deleted,"is_sent":draft.is_sent}))
    }
}

pub fn active_request(
    client: &SlackClient,
    w: &WorkspaceSession,
    next: Option<&str>,
) -> PreparedRequest {
    let mut fields = vec![
        ("is_active", "true".into()),
        ("limit", "25".into()),
        ("_x_reason", "home-workflow-draft-presence".into()),
    ];
    if let Some(next) = next {
        fields.push(("next_ts", next.into()));
    }
    client.rest_form(w, "drafts.list", fields)
}
pub fn active_ids_request(
    client: &SlackClient,
    w: &WorkspaceSession,
    next: Option<&str>,
) -> PreparedRequest {
    let mut fields = vec![
        ("limit", "1000".into()),
        ("_x_reason", "home-workflow-draft-presence".into()),
    ];
    if let Some(next) = next {
        fields.push(("next_ts", next.into()));
    }
    client.rest_form(w, "drafts.listActive", fields)
}

#[cfg(test)]
mod tests;
