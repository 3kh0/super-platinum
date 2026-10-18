use super::*;
use crate::slack::{PreparedRequest, SlackClient};
use std::collections::BTreeSet;

/// ly/g -> aV5z users.info. This adapter is deliberately the populated unified
/// account-type store branch of Bn/N; legacy roster acquisition is not guessed.
pub struct RoleRead {
    scope: Scope,
    team: String,
    enterprise: String,
}
impl RoleRead {
    pub fn capture(scope: &Scope, config: Config, team: &str) -> Result<Self, Error> {
        if config.org_unified != Some(true)
            || scope.self_enterprise().is_none()
            || !identifier(team, b'T')
            || !scope
                .internal_teams()
                .is_some_and(|ids| ids.iter().any(|id| id == team))
        {
            return Err(invalid());
        }
        Ok(Self {
            scope: scope.clone(),
            team: team.into(),
            enterprise: scope.self_enterprise().ok_or_else(invalid)?.into(),
        })
    }
    pub fn team(&self) -> &str {
        &self.team
    }
    pub fn enterprise_team(&self) -> &str {
        &self.enterprise
    }
    /// ly/g dispatches through the captured enterprise store. The caller must
    /// supply that actual session and retain/recheck both credential snapshots;
    /// never manufacture an enterprise session by relabeling a workspace token.
    pub fn request(
        &self,
        c: &SlackClient,
        w: &WorkspaceSession,
        enterprise: &WorkspaceSession,
    ) -> Result<PreparedRequest, Error> {
        if !self.scope.current(w)
            || self.enterprise != enterprise.team_id
            || enterprise.user_id != self.scope.user()
            || enterprise.enterprise_id.as_deref() != self.scope.self_enterprise()
        {
            return Err(invalid());
        }
        Ok(c.rest_form(
            enterprise,
            "users.info",
            vec![
                ("self", "true".into()),
                ("team", self.team.clone()),
                ("_x_reason", "fetch-workspace-account-types".into()),
            ],
        ))
    }
    /// Missing team_user is ly/g's isUnknown, not a negative role observation.
    pub fn decode(&self, raw: &Value) -> Result<Option<Roles>, Error> {
        success(raw)?;
        let user = raw
            .get("user")
            .and_then(Value::as_object)
            .ok_or_else(invalid)?;
        if user
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id != self.scope.user())
        {
            return Err(invalid());
        }
        let Some(team_user) = user.get("team_user").filter(|v| !v.is_null()) else {
            return Ok(None);
        };
        let team_user = team_user.as_object().ok_or_else(invalid)?;
        let enterprise = self.scope.self_data.get("enterprise_user");
        if enterprise.is_some_and(|v| !v.is_null() && !v.is_object()) {
            return Err(invalid());
        }
        let enterprise_flag = |key| {
            self.scope.same_enterprise(&self.team)
                && enterprise.and_then(|v| v.get(key)).is_some_and(truthy)
        };
        Ok(Some(Roles {
            workspace_admin: team_user.get("is_admin").is_some_and(truthy),
            workspace_owner: team_user.get("is_owner").is_some_and(truthy),
            enterprise_admin: enterprise_flag("is_admin"),
            enterprise_owner: enterprise_flag("is_owner"),
        }))
    }
}

/// t0MN kC -> dPZG usergroups.membership -> BWei bulkSet membership.
pub struct MembershipRead {
    scope: Scope,
}
impl MembershipRead {
    pub fn capture(scope: &Scope) -> Self {
        Self {
            scope: scope.clone(),
        }
    }
    pub fn request(&self, c: &SlackClient, w: &WorkspaceSession) -> Result<PreparedRequest, Error> {
        if !self.scope.current(w) {
            return Err(invalid());
        }
        Ok(c.rest_form(
            w,
            "usergroups.membership",
            vec![(
                "_x_reason",
                "user-group-membership-api/fetchUserGroupMembership".into(),
            )],
        ))
    }
    pub fn decode(&self, raw: &Value) -> Result<BTreeSet<String>, Error> {
        if raw.get("ok") == Some(&Value::Bool(false))
            && raw.get("error").and_then(Value::as_str) == Some("user_is_restricted")
        {
            return Ok(BTreeSet::new());
        }
        success(raw)?;
        let Some(groups) = raw.get("usergroups").filter(|v| !v.is_null()) else {
            return Ok(BTreeSet::new());
        };
        groups
            .as_array()
            .ok_or_else(invalid)?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .filter(|id| identifier(id, b'S'))
                    .map(str::to_owned)
                    .ok_or_else(invalid)
            })
            .collect()
    }
}
