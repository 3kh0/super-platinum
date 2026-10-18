use super::*;
use crate::slack::{PreparedRequest, SlackClient};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};

/// Fresh client.init identities and complete workspace metadata. No Debug:
/// exact credentials are retained solely for request association/currentness.
#[derive(Clone)]
pub struct Scope {
    owner: WorkspaceSession,
    team: Value,
    pub(super) self_data: Value,
    restricted: bool,
    enterprise: Option<String>,
    internal: Option<Vec<String>>,
    workspaces: BTreeMap<String, Value>,
}
impl Scope {
    pub fn decode(workspace: &WorkspaceSession, boot: &Value) -> Result<Self, Error> {
        success(boot)?;
        if !(identifier(&workspace.team_id, b'T') || identifier(&workspace.team_id, b'E'))
            || !(identifier(&workspace.user_id, b'U') || identifier(&workspace.user_id, b'W'))
            || boot.pointer("/team/id").and_then(Value::as_str) != Some(&workspace.team_id)
            || boot.pointer("/self/id").and_then(Value::as_str) != Some(&workspace.user_id)
            || !boot["self"].is_object()
            || !boot["team"].is_object()
            || workspace
                .enterprise_id
                .as_ref()
                .is_some_and(|id| !identifier(id, b'E'))
            || (workspace.team_id.starts_with('E')
                && workspace
                    .enterprise_id
                    .as_ref()
                    .is_some_and(|id| id != &workspace.team_id))
        {
            return Err(invalid());
        }
        let mut workspaces = BTreeMap::new();
        let internal = if let Some(raw) = boot.get("workspaces") {
            let mut ids = Vec::new();
            let mut seen = BTreeSet::new();
            for value in raw.as_array().ok_or_else(invalid)? {
                let id = value
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|id| identifier(id, b'T'))
                    .ok_or_else(invalid)?;
                if !seen.insert(id) {
                    return Err(invalid());
                }
                if value.get("allow_external_users") != Some(&Value::Bool(true)) {
                    ids.push(id.into());
                }
                workspaces.insert(id.into(), value.clone());
            }
            Some(ids)
        } else {
            None
        };
        let observed_enterprise = boot["team"]
            .get("enterprise_id")
            .filter(|v| !v.is_null())
            .map(|v| {
                v.as_str()
                    .filter(|id| identifier(id, b'E'))
                    .ok_or_else(invalid)
            })
            .transpose()?;
        if workspace
            .enterprise_id
            .as_deref()
            .zip(observed_enterprise)
            .is_some_and(|(a, b)| a != b)
        {
            return Err(invalid());
        }
        // v7Vi.jV observes the configured selfTeamIds.enterpriseId. Team
        // metadata describes affiliation; it must never establish self scope.
        let enterprise = workspace.enterprise_id.clone();
        workspaces.insert(workspace.team_id.clone(), boot["team"].clone());
        Ok(Self {
            owner: workspace.clone(),
            team: boot["team"].clone(),
            self_data: boot["self"].clone(),
            restricted: boot["self"].get("is_restricted").is_some_and(truthy),
            enterprise,
            internal,
            workspaces,
        })
    }
    pub fn current(&self, w: &WorkspaceSession) -> bool {
        same_owner(&self.owner, w)
    }
    pub fn same_observation(&self, other: &Self) -> bool {
        same_owner(&self.owner, &other.owner)
            && self.team == other.team
            && self.self_data == other.self_data
            && self.restricted == other.restricted
            && self.internal == other.internal
            && self.workspaces == other.workspaces
            && self.enterprise == other.enterprise
    }
    pub fn team(&self) -> &str {
        &self.owner.team_id
    }
    pub fn user(&self) -> &str {
        &self.owner.user_id
    }
    pub fn restricted(&self) -> bool {
        self.restricted
    }
    pub fn internal_teams(&self) -> Option<&[String]> {
        self.internal.as_deref()
    }
    pub(super) fn self_enterprise(&self) -> Option<&str> {
        self.enterprise.as_deref()
    }
    pub(super) fn paid_tier(&self) -> bool {
        identifier(self.team(), b'E') || self.team.get("plan").is_some_and(truthy)
    }
    pub(super) fn enterprise_workspace(&self, unified: bool) -> bool {
        (unified && identifier(self.team(), b'E'))
            || self.team.get("enterprise_id").is_some_and(truthy)
    }
    pub(super) fn same_enterprise(&self, team: &str) -> bool {
        self.enterprise.as_ref().is_some_and(|id| {
            self.workspaces
                .get(team)
                .and_then(|v| v.get("enterprise_id"))
                .and_then(Value::as_str)
                == Some(id)
        })
    }
}

/// Closed reads only. Native reasons identify this caller; wire methods/fields
/// come from client.init/client.extras, cKZq/IjwW and enterprise prefs producers.
pub struct PolicyReads;
impl PolicyReads {
    pub fn experiments(c: &SlackClient, w: &WorkspaceSession) -> PreparedRequest {
        crate::slack::api::list_experiments(c, w)
    }
    pub fn boot(c: &SlackClient, w: &WorkspaceSession) -> PreparedRequest {
        c.rest_form(
            w,
            "client.init",
            vec![
                ("include_relevant_onboarding", "false".into()),
                ("_x_reason", "native-workflow-capabilities".into()),
            ],
        )
    }
    pub fn features(c: &SlackClient, w: &WorkspaceSession) -> PreparedRequest {
        c.rest_form(
            w,
            "features.access.policies.list",
            vec![("_x_reason", "native-workflow-capabilities".into())],
        )
    }
    pub fn team_preferences(c: &SlackClient, w: &WorkspaceSession) -> PreparedRequest {
        c.rest_form(
            w,
            "client.extras",
            vec![
                ("extras", json!(["team_prefs"]).to_string()),
                ("_x_reason", "native-workflow-capabilities".into()),
            ],
        )
    }
    pub fn enterprise_preferences(c: &SlackClient, w: &WorkspaceSession) -> PreparedRequest {
        c.rest_form(
            w,
            "enterprise.prefs.get",
            vec![("_x_reason", "native-workflow-capabilities".into())],
        )
    }
}

/// mLk1.yI (not tA): current workspace, or any known internal workspace for an
/// enterprise context; an observed empty enterprise inventory uses org scope.
pub struct PermissionRead {
    scope: Scope,
    teams: Option<Vec<String>>,
}
#[derive(Clone)]
pub struct PermissionObservation {
    scope: Scope,
    decision: Decision,
}
impl PermissionObservation {
    pub fn decision(&self) -> Decision {
        self.decision
    }
    pub(super) fn matches_scope(&self, scope: &Scope) -> bool {
        self.scope.same_observation(scope)
    }
}
impl PermissionRead {
    pub fn capture(scope: &Scope) -> Result<Self, Error> {
        let teams = if scope.team().starts_with('E') {
            if scope.enterprise.is_none() {
                return Err(invalid());
            }
            let ids = scope.internal_teams().ok_or_else(invalid)?;
            if ids.is_empty() {
                None
            } else {
                Some(ids.to_vec())
            }
        } else {
            None
        };
        Ok(Self {
            scope: scope.clone(),
            teams,
        })
    }
    pub fn discovery(
        &self,
        c: &SlackClient,
        w: &WorkspaceSession,
    ) -> Result<PreparedRequest, Error> {
        if !self.scope.current(w) {
            return Err(invalid());
        }
        Ok(c.rest_form(
            w,
            "api.getFlannelHttpUrl",
            vec![
                ("include_external_workspaces", "true".into()),
                ("_x_reason", "client-needs-valid-flannel-url".into()),
            ],
        ))
    }
    pub fn arguments(&self) -> Value {
        let mut args = json!({"user_id":self.scope.user(),"permissions":["CREATE_WORKFLOWS"]});
        if let Some(ids) = &self.teams {
            args["team_ids"] = json!(ids);
        } else {
            args["as_admin"] = json!(true);
        }
        args
    }
    /// Caller supplies the time at which this exact discovery response arrived.
    /// Recheck its captured owner and the 2.5-second early-expiry margin.
    pub fn request(
        &self,
        c: &SlackClient,
        w: &WorkspaceSession,
        discovery: &Value,
        observed: Instant,
        now: Instant,
    ) -> Result<PreparedRequest, Error> {
        if !self.scope.current(w) || now < observed {
            return Err(invalid());
        }
        success(discovery)?;
        let ttl = discovery["ttl_seconds"].as_u64().ok_or_else(invalid)?;
        let life = Duration::from_secs(ttl).saturating_sub(Duration::from_millis(2500));
        let expires = observed.checked_add(life).ok_or_else(invalid)?;
        if now >= expires {
            return Err(invalid());
        }
        let endpoint = discovery["url"].as_str().ok_or_else(invalid)?;
        let team = format!("https://edgeapi.slack.com/cache/{}", w.team_id);
        let enterprise = self
            .scope
            .enterprise
            .as_ref()
            .map(|id| format!("https://edgeapi.slack.com/cache/{id}"));
        if endpoint != team && enterprise.as_deref() != Some(endpoint) {
            return Err(invalid());
        }
        let mut logical = w.clone();
        logical.enterprise_id = None;
        let mut args = self.arguments();
        if w.team_id.starts_with('E') {
            args["enterprise_token"] = json!(w.token);
        }
        let mut request = c
            .edge_json(&logical, "permissions/info", args)
            .map_err(|_| invalid())?;
        let suffix = request
            .url
            .split_once('?')
            .map(|(_, v)| format!("?{v}"))
            .unwrap_or_default();
        request.url = format!("{endpoint}/permissions/info{suffix}");
        Ok(request)
    }
    pub fn decode(&self, raw: &Value) -> Result<PermissionObservation, Error> {
        success(raw)?;
        let contains = |raw: &Value| -> Result<bool, Error> {
            let values = raw.as_array().ok_or_else(invalid)?;
            if values.iter().any(|v| v.as_str().is_none()) {
                return Err(invalid());
            }
            Ok(values
                .iter()
                .any(|v| v.as_str() == Some("CREATE_WORKFLOWS")))
        };
        let allowed = if let Some(teams) = &self.teams {
            let map = raw
                .get("allowed_per_team")
                .and_then(Value::as_object)
                .ok_or_else(invalid)?;
            let mut result = false;
            for team in teams {
                if let Some(raw) = map.get(team) {
                    result |= contains(raw)?;
                }
            }
            result
        } else {
            contains(raw.get("allowed").ok_or_else(invalid)?)?
        };
        Ok(PermissionObservation {
            scope: self.scope.clone(),
            decision: Decision::observed(allowed),
        })
    }
}
