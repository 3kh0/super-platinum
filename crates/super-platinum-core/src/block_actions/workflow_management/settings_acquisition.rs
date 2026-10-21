//! Closed CydK settings reads. Raw pages are observations, not picker eligibility
//! or publication authority. The host retains query/account/source ownership.
use super::{Error, PreparedRequest, SlackClient, Value, WorkspaceSession, json};
use crate::block_actions::directory_endpoint::DirectoryEndpoint;
use std::collections::BTreeSet;
use std::time::Instant;

mod page;
pub use page::Page;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchKind {
    Members,
    Channels,
    InternalTeams,
    ExternalTeams,
    AvailableExternal,
}
#[derive(Clone)]
pub struct Read {
    owner: WorkspaceSession,
    kind: SearchKind,
    body: Value,
    teams_by_ids: bool,
    reason: Option<String>,
}
pub(super) fn invalid() -> Error {
    Error::Api("invalid_workflow_settings_read".into())
}
fn marker(value: &str) -> bool {
    !value.is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}
fn owner_valid(owner: &WorkspaceSession) -> bool {
    !owner.token.is_empty() && !owner.user_id.is_empty() && !owner.team_id.is_empty()
}
impl Read {
    pub fn new(
        owner: &WorkspaceSession,
        kind: SearchKind,
        query: &str,
        next: Option<&str>,
    ) -> Result<Self, Error> {
        if !owner_valid(owner)
            || query.len() > 4096
            || query.contains('\0')
            || next.is_some_and(|value| !marker(value))
            || kind == SearchKind::AvailableExternal && !query.is_empty()
        {
            return Err(invalid());
        }
        let mut body = match kind {
            SearchKind::Members => json!({"query":query,"count":24,"fuzz":1,
                "enable_workspace_ranking":true,"filter":"NOT deactivated AND NOT guests",
                "include_profile_only_users":true}),
            SearchKind::Channels => json!({"query":query,"count":24,"fuzz":1,
                "include_record_channels":true,"check_membership":true}),
            SearchKind::InternalTeams => {
                // JSON.stringify property order is retained in this string too.
                let encoded = if query.is_empty() {
                    r#"{"type":"is","value":"team"}"#.to_owned()
                } else {
                    format!(
                        r#"{{"type":"and","clauses":[{{"type":"is","value":"team"}},{{"type":"fuzzy","value":{}}}]}}"#,
                        serde_json::to_string(query).map_err(|_| invalid())?
                    )
                };
                json!({"query":encoded,"count":24})
            }
            SearchKind::ExternalTeams => json!({"query":query}),
            SearchKind::AvailableExternal => json!({"count":100,"filter":"available"}),
        };
        if let Some(next) = next {
            body[if kind == SearchKind::InternalTeams {
                "cursor_mark"
            } else {
                "marker"
            }] = json!(next);
        }
        Ok(Self {
            owner: owner.clone(),
            kind,
            body,
            teams_by_ids: false,
            reason: Some("fetchAndUpsertTeamsWithQuery".into()),
        })
    }
    /// One source cZN0 chunk. The original caller supplies its real reason;
    /// there is no source default reason for the by-ID thunk. None preserves
    /// the actual caller's omission, rather than inventing an empty reason.
    pub fn teams_by_ids(
        owner: &WorkspaceSession,
        ids: &[String],
        reason: Option<&str>,
    ) -> Result<Self, Error> {
        let unique: BTreeSet<_> = ids.iter().collect();
        if !owner_valid(owner)
            || ids.is_empty()
            || ids.len() > 250
            || unique.len() != ids.len()
            || !ids.iter().all(|id| team_id(id))
            || reason.is_some_and(|reason| !marker(reason))
        {
            return Err(invalid());
        }
        Ok(Self {
            owner: owner.clone(),
            kind: SearchKind::InternalTeams,
            body: json!({"teams":ids,"include_unlisted_if_admin":false}),
            teams_by_ids: true,
            reason: reason.map(str::to_owned),
        })
    }
    pub fn kind(&self) -> SearchKind {
        self.kind
    }
    pub fn body(&self) -> &Value {
        &self.body
    }
    pub fn method(&self) -> &'static str {
        if self.teams_by_ids {
            return "team.info";
        }
        match self.kind {
            SearchKind::Members => "users/search",
            SearchKind::Channels => "channels/search",
            SearchKind::InternalTeams => "search.enterprise",
            SearchKind::ExternalTeams => "externalTeams/search",
            SearchKind::AvailableExternal => "externalTeams/list",
        }
    }
    pub fn current(&self, owner: &WorkspaceSession) -> bool {
        self.owner.team_id == owner.team_id
            && self.owner.enterprise_id == owner.enterprise_id
            && self.owner.user_id == owner.user_id
            && self.owner.token == owner.token
            && self.owner.url == owner.url
    }
    pub fn request(
        &self,
        client: &SlackClient,
        owner: &WorkspaceSession,
        endpoint: Option<&DirectoryEndpoint>,
        now: Instant,
    ) -> Result<PreparedRequest, Error> {
        if !self.current(owner) {
            return Err(invalid());
        }
        if self.kind == SearchKind::InternalTeams {
            let mut fields: Vec<(&str, String)> = self
                .body
                .as_object()
                .ok_or_else(invalid)?
                .iter()
                .map(|(k, v)| (k.as_str(), super::form_value(v)))
                .collect();
            if let Some(reason) = &self.reason {
                fields.push(("_x_reason", reason.clone()));
            }
            Ok(client.rest_form(owner, self.method(), fields))
        } else {
            endpoint
                .ok_or_else(invalid)?
                .workflow_settings(client, owner, now, self)
        }
    }
    pub fn decode(&self, raw: Value) -> Result<Page, Error> {
        self.decode_ordered(raw, None)
    }
    pub fn decode_ordered(&self, raw: Value, keys: Option<&[String]>) -> Result<Page, Error> {
        Page::decode(self, raw, keys)
    }
    pub(super) fn row_field(&self) -> &'static str {
        if self.teams_by_ids {
            "teams"
        } else if self.kind == SearchKind::InternalTeams {
            "items"
        } else {
            "results"
        }
    }
}
pub(super) fn team_id(id: &str) -> bool {
    id.len() >= 9
        && matches!(id.as_bytes()[0], b'T' | b'E')
        && id.as_bytes()[1..]
            .iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}
