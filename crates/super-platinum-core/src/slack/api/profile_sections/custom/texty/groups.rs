//! Profile aD6p -> m0EK -> D8Wa/2gUC group search. Pure observations only.
//! Effective JJAw records are distinct from raw incoming pages; neither grants
//! insertion authority without the desktop's current owner/native caret guards.
use crate::{
    block_actions::directory_endpoint::DirectoryEndpoint,
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient, models::User},
};
use serde_json::{Value, json};
use std::time::Instant;
mod matching;
pub use super::channels::{Outcomes, Page};
pub use super::members::Pages;
pub use matching::{GroupScored, Names, matches};
pub const LIMIT: usize = 25;
#[cfg(test)]
mod tests;
fn unavailable() -> Error {
    Error::Api("profile_group_completion_unavailable".into())
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null | Value::Bool(false) => false,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        _ => true,
    }
}
fn id(v: &str) -> bool {
    v.len() >= 9
        && v.len() <= 255
        && v.starts_with('S')
        && v.bytes().all(|b| b.is_ascii_alphanumeric())
}
fn string(v: &Value) -> Result<&str, Error> {
    if !truth(v) {
        Ok("")
    } else {
        v.as_str().ok_or_else(unavailable)
    }
}
/// One explicit original row and the independently produced effective record.
/// This does not run JJAw or infer a missing predecessor from a legacy cache.
/// The intake owner supplies its accepted projection, including same-version
/// reselection of an old effective row. The exact values remain separately held.
#[derive(Clone)]
pub struct Snapshot {
    source: Value,
    effective: Value,
}
impl Snapshot {
    pub fn capture(source: Value, effective: Value) -> Result<Self, Error> {
        let key = source["id"]
            .as_str()
            .filter(|v| id(v))
            .ok_or_else(unavailable)?;
        if effective["id"].as_str() != Some(key) || !source.is_object() || !effective.is_object() {
            return Err(unavailable());
        }
        for key in ["name", "_name_lc", "handle"] {
            string(&effective[key])?;
        }
        Ok(Self { source, effective })
    }
    pub fn source(&self) -> &Value {
        &self.source
    }
    pub fn effective(&self) -> &Value {
        &self.effective
    }
    pub fn id(&self) -> &str {
        self.effective["id"].as_str().expect("captured group ID")
    }
}
/// aD6p/0dhJ category admission and fixed includeDeleted=false. The native
/// caller must actually observe self and registered unified-user mode first.
#[derive(Clone, Copy)]
pub struct Policy {
    enabled: bool,
    include_from_another_team: bool,
}
impl Policy {
    pub fn capture(self_user: &User, unified_observed: bool) -> Result<Self, Error> {
        if !super::member_policy::member_id(&self_user.id) {
            return Err(unavailable());
        }
        Ok(Self {
            enabled: !self_user.is_restricted,
            include_from_another_team: unified_observed,
        })
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn include_from_another_team(&self) -> bool {
        self.include_from_another_team
    }
    /// Locality is a captured tO.dk selector, not a guessed team_id comparison.
    /// Unified mode does not invoke that selector at all.
    pub fn allows(&self, row: &Snapshot, from_another_team: Option<bool>) -> Result<bool, Error> {
        let row = row.effective();
        if !self.enabled || truth(&row["isUnknown"]) || truth(&row["isNonExistent"]) {
            return Ok(false);
        }
        if !self.include_from_another_team && from_another_team.ok_or_else(unavailable)? {
            return Ok(false);
        }
        Ok(!truth(&row["date_delete"]))
    }
}
#[derive(Clone)]
pub struct Query {
    owner: WorkspaceSession,
    query: String,
    org_wide: bool,
}
impl Query {
    pub fn capture(
        owner: &WorkspaceSession,
        query: &str,
        include_from_another_team: bool,
    ) -> Result<Self, Error> {
        if query.len() > 1_048_576
            || owner.team_id.is_empty()
            || owner.user_id.is_empty()
            || owner.token.is_empty()
        {
            return Err(unavailable());
        }
        Ok(Self {
            owner: owner.clone(),
            query: query.into(),
            org_wide: include_from_another_team,
        })
    }
    pub fn current(&self, owner: &WorkspaceSession) -> bool {
        self.owner.team_id == owner.team_id
            && self.owner.enterprise_id == owner.enterprise_id
            && self.owner.user_id == owner.user_id
            && self.owner.token == owner.token
            && self.owner.url == owner.url
            && self.owner.name == owner.name
    }
    pub fn raw(&self) -> &str {
        &self.query
    }
    pub fn request(
        &self,
        client: &SlackClient,
        owner: &WorkspaceSession,
        endpoint: &DirectoryEndpoint,
        now: Instant,
        marker: Option<&str>,
    ) -> Result<PreparedRequest, Error> {
        if !self.current(owner)
            || marker
                .is_some_and(|m| m.is_empty() || m.len() > 4096 || m.chars().any(char::is_control))
        {
            return Err(unavailable());
        }
        let mut body = json!({"query":self.query,"count":LIMIT,"org_wide":self.org_wide});
        if let Some(marker) = marker {
            body["marker"] = json!(marker);
        }
        endpoint.request(client, owner, now, "usergroups/search", body)
    }
}
