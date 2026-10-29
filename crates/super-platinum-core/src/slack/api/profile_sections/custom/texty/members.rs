//! aD6p/fRHe Profile member options -> WNgx -> 2gUC users/search.
//! This is request/response custody only, not member eligibility or ranking.
use crate::{
    block_actions::directory_endpoint::DirectoryEndpoint,
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient},
};
use serde_json::{Value, json};
use std::time::Instant;

// The installed channel decoder is the neutral 2gUC decoder: it retains raw
// object/array order, auxiliary rows and outcomes without filtering entity IDs.
pub use super::channels::{Outcomes, Page};
pub const LIMIT: usize = super::channels::LIMIT;
mod final_sdk;

#[cfg(test)]
mod tests;

fn unavailable() -> Error {
    Error::Api("profile_member_completion_unavailable".into())
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

/// Actual processed priority/default-workspace observations. Default supplies no
/// readiness evidence; callers must establish the observations independently.
#[derive(Clone, Default)]
pub struct RequestFacts {
    pub top_users: Option<Value>,
    pub default_workspace: Option<String>,
}

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
        // Local defensive bounds, not a Slack query grammar.
        if query.len() > 1_048_576
            || owner.team_id.is_empty()
            || owner.user_id.is_empty()
            || owner.token.is_empty()
            || facts
                .top_users
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
        marker: Option<&str>,
    ) -> Result<PreparedRequest, Error> {
        if !self.current(owner)
            || marker
                .is_some_and(|m| m.is_empty() || m.len() > 4096 || m.chars().any(char::is_control))
        {
            return Err(unavailable());
        }
        // aD6p includes deleted members; inherited includeBots/includeAgents,
        // includeExternal/includeGuests are true. WNgx therefore emits no filter.
        // excludeDeletedBots, excludeInteractionRestrictedApps, includeSelf and
        // includeProfileOnlyUsers are local caller options, not extra wire keys.
        let mut body = json!({"query":self.query,"count":LIMIT,"fuzz":1,
            "enable_workspace_ranking":true});
        if let Some(top) = self.facts.top_users.as_ref().filter(|v| truth(v)) {
            body["top_users"] = top.clone();
        }
        if let Some(workspace) = self
            .facts
            .default_workspace
            .as_ref()
            .filter(|v| !v.is_empty())
        {
            body["default_workspace"] = json!(workspace);
        }
        if let Some(marker) = marker {
            body["marker"] = json!(marker);
        }
        endpoint.request(client, owner, now, "users/search", body)
    }
}

/// Neutral 2gUC accumulation with a decode-and-append boundary: malformed bytes
/// poison this query just like cyclic pagination. Retrying needs a fresh Pages.
#[derive(Clone, Default)]
pub struct Pages {
    inner: super::channels::Pages,
    failed: bool,
}
impl Pages {
    pub fn accept(&mut self, raw: Value, keys: Option<&[String]>) -> Result<(), Error> {
        let page = match Page::decode_ordered(raw, keys) {
            Ok(page) => page,
            Err(error) => {
                self.failed = true;
                return Err(error);
            }
        };
        self.append(page)
    }
    pub fn append(&mut self, page: Page) -> Result<(), Error> {
        if self.failed {
            return Err(unavailable());
        }
        let result = self.inner.append(page);
        self.failed = result.is_err();
        result
    }
    pub fn complete(&self) -> bool {
        !self.failed && self.inner.complete()
    }
    pub fn marker(&self) -> Option<&str> {
        if self.failed {
            None
        } else {
            self.inner.marker()
        }
    }
    /// Raw accepted pages remain available for diagnostics, never as a complete
    /// result after failure. Use rows/outcomes for completed-query consumers.
    pub fn pages(&self) -> &[Page] {
        self.inner.pages()
    }
    pub fn rows(&self) -> Result<&[Value], Error> {
        if self.failed {
            return Err(unavailable());
        }
        self.inner.rows()
    }
    pub fn outcomes(&self) -> Result<&Outcomes, Error> {
        if self.failed {
            return Err(unavailable());
        }
        self.inner.outcomes()
    }
}
