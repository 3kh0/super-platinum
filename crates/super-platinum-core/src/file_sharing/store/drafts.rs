//! Explicitly saved, unsubmitted notes. Separate from delivery receipts.
//! No operation here grants permission, executes Slack work, or autosaves typing.
use super::*;
use serde::{Deserialize, Serialize};

mod associations;
mod storage;
pub(super) use associations::{associate, blocking, retire_association, settled, take_association};
pub(super) use storage::{check_current, migrate};
#[cfg(test)]
mod tests;

pub const MAX_DRAFTS: usize = 32;
pub const MAX_SOURCE_BYTES: usize = 256 * 1024;
const MAX_DRAFT_BODY: usize = MAX_SOURCE_BYTES + FILE_SHARE_NOTE_BYTES + 65_536;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftScope {
    pub team: String,
    pub user: String,
    pub file: String,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Destination {
    pub id: String,
    pub name: String,
    pub private: bool,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Content {
    /// Exact bounded canonical File snapshot; never a Context or API envelope.
    pub source: Value,
    pub title: String,
    pub note: Vec<Value>,
    pub destination: Option<Destination>,
}
impl Content {
    pub fn empty(&self) -> bool {
        self.note.is_empty() && self.destination.is_none()
    }
}
/// No Debug: source metadata, note and display labels are private user data.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub scope: DraftScope,
    pub token: String,
    pub revision: u64,
    pub content: Content,
}
pub enum Claim {
    Claimed(Draft),
    /// Includes exact expected-absence conflict and removed/replaced old token.
    Conflict(Option<Draft>),
    Blocked(Receipt),
    Capacity,
}
pub enum Mutation<T> {
    Applied(T),
    Conflict(Option<Draft>),
    Blocked(Receipt),
}
fn valid_scope(scope: &DraftScope) -> bool {
    identity(&scope.team) && identity(&scope.user) && api_shape(&scope.file, b"F", 2)
}
fn api_shape(value: &str, prefix: &[u8], min: usize) -> bool {
    (min..=64).contains(&value.len())
        && prefix.contains(&value.as_bytes()[0])
        && value
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
fn bounded_source(value: &Value) -> bool {
    let mut work = vec![(value, 0)];
    let mut nodes = 0usize;
    while let Some((v, depth)) = work.pop() {
        nodes += 1;
        if depth > 32 || nodes > MAX_SOURCE_BYTES {
            return false;
        }
        match v {
            Value::Array(values) => {
                if values.len() > MAX_SOURCE_BYTES.saturating_sub(nodes + work.len()) {
                    return false;
                }
                work.extend(values.iter().map(|v| (v, depth + 1)));
            }
            Value::Object(values) => {
                if values.len() > MAX_SOURCE_BYTES.saturating_sub(nodes + work.len()) {
                    return false;
                }
                work.extend(values.values().map(|v| (v, depth + 1)));
            }
            _ => (),
        }
    }
    struct Limit(usize);
    impl std::io::Write for Limit {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.0 {
                return Err(std::io::Error::other("source limit"));
            }
            self.0 -= bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(&mut Limit(MAX_SOURCE_BYTES), value).is_ok()
}
fn validate_content(scope: &DraftScope, content: &Content) -> Result<(), AppError> {
    if !valid_scope(scope)
        || !content.source.is_object()
        || !bounded_source(&content.source)
        || content.source.get("id").and_then(Value::as_str) != Some(&scope.file)
        || content.title.is_empty()
        || content.title.len() > 16_384
    {
        return Err(invalid());
    }
    let file: crate::slack::models::File =
        serde_json::from_value(content.source.clone()).map_err(|_| invalid())?;
    if file.id.as_deref() != Some(&scope.file) {
        return Err(invalid());
    }
    if !content.note.is_empty() {
        FileShareNote::new(content.note.clone()).map_err(|_| invalid())?;
    }
    if content
        .destination
        .as_ref()
        .is_some_and(|d| !api_shape(&d.id, b"CGD", 9) || d.name.is_empty() || d.name.len() > 16_384)
    {
        return Err(invalid());
    }
    Ok(())
}
fn encode(draft: &Draft) -> Result<String, AppError> {
    validate_content(&draft.scope, &draft.content)?;
    if uuid::Uuid::parse_str(&draft.token).is_err() || draft.revision == 0 {
        return Err(invalid());
    }
    number(draft.revision)?;
    let body = serde_json::to_string(draft).map_err(|_| invalid())?;
    if body.len() > MAX_DRAFT_BODY {
        return Err(invalid());
    }
    Ok(body)
}
fn decode(body: String, scope: &DraftScope, token: &str, revision: i64) -> Result<Draft, AppError> {
    if body.len() > MAX_DRAFT_BODY || revision <= 0 {
        return Err(invalid());
    }
    let draft: Draft = serde_json::from_str(&body).map_err(|_| invalid())?;
    encode(&draft)?;
    if draft.scope != *scope || draft.token != token || draft.revision != revision as u64 {
        return Err(invalid());
    }
    Ok(draft)
}
