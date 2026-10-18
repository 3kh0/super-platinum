//! Am1o/fRHe -> 53BY -> 2gUC channel search. Ordinary-channel presentation is
//! a caller restriction, never an invented server filter. Discovery is EVxR.
use crate::{
    block_actions::directory_endpoint::DirectoryEndpoint,
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient},
};
use serde_json::{Value, json};
use std::time::Instant;
mod pages;
pub use pages::{Outcomes, Page, Pages};
#[cfg(test)]
mod tests;

pub const LIMIT: usize = 25;
fn unavailable() -> Error {
    Error::Api("profile_channel_completion_unavailable".into())
}
fn marker(value: &str) -> bool {
    !value.is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}
fn truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_some_and(|v| v != 0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}
/// Actual Am1o observations, not inferred from credentials or typed channels.
/// Absence means that the source observation supplied no optional hint. The
/// caller must establish that observation; Default is not readiness evidence.
#[derive(Clone, Default)]
pub struct RequestFacts {
    pub top_channels: Option<Value>,
    pub default_workspace: Option<String>,
}
/// Request ownership only. The desktop separately retains account/transport,
/// source, query/caret and renderer custody through dispatch and selection.
#[derive(Clone)]
pub struct Query {
    owner: WorkspaceSession,
    query: String,
    facts: RequestFacts,
}
impl Query {
    pub fn capture(
        owner: &WorkspaceSession,
        query: &str,
        facts: RequestFacts,
    ) -> Result<Self, Error> {
        // Defensive local bounds, not a Slack query grammar or server quota.
        if query.len() > 1_048_576
            || owner.team_id.is_empty()
            || owner.user_id.is_empty()
            || owner.token.is_empty()
            || facts
                .top_channels
                .as_ref()
                .is_some_and(|v| v.to_string().len() > 1_048_576)
            || facts
                .default_workspace
                .as_ref()
                .is_some_and(|v| v.len() > 4096)
        {
            return Err(unavailable());
        }
        Ok(Self {
            owner: owner.clone(),
            query: query.into(),
            facts,
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
        next: Option<&str>,
    ) -> Result<PreparedRequest, Error> {
        if !self.current(owner) || next.is_some_and(|v| !marker(v)) {
            return Err(unavailable());
        }
        let mut body = json!({"query":self.query,"count":LIMIT,"fuzz":1,
            "filter":"xws","include_record_channels":true,"check_membership":true});
        if let Some(top) = self.facts.top_channels.as_ref().filter(|v| truth(v)) {
            body["top_channels"] = top.clone();
        }
        if let Some(workspace) = self
            .facts
            .default_workspace
            .as_ref()
            .filter(|s| !s.is_empty())
        {
            body["default_workspace"] = json!(workspace);
        }
        if let Some(next) = next {
            body["marker"] = json!(next);
        }
        endpoint.request(client, owner, now, "channels/search", body)
    }
}
