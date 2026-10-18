use super::*;
use serde_json::Map;
use std::collections::{BTreeMap, BTreeSet};

/// Exact Bn/N selector observations for one workspace. A missing map entry is
/// unavailable, not a role inferred from the member's restriction flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Roles {
    pub workspace_owner: bool,
    pub workspace_admin: bool,
    pub enterprise_owner: bool,
    pub enterprise_admin: bool,
}

/// Owns complete, request-bound policy objects. Missing preference keys inside
/// a successful complete object retain source defaults; unloaded objects do not.
#[derive(Clone)]
pub struct PreferenceFacts {
    scope: Scope,
    teams: BTreeMap<String, Map<String, Value>>,
    org: Option<Map<String, Value>>,
    locked: Option<Map<String, Value>>,
    unlocked: Option<Map<String, Value>>,
    roles: BTreeMap<String, Roles>,
    subteams: Option<BTreeSet<String>>,
}
impl PreferenceFacts {
    pub(super) fn matches_scope(&self, scope: &Scope) -> bool {
        self.scope.same_observation(scope)
    }
    pub(super) fn same_inputs(&self, other: &Self) -> bool {
        self.scope.same_observation(&other.scope)
            && self.teams == other.teams
            && self.org == other.org
            && self.locked == other.locked
            && self.unlocked == other.unlocked
            && self.roles == other.roles
            && self.subteams == other.subteams
    }
    pub fn decode(
        scope: &Scope,
        extras: &Value,
        enterprise: Option<&Value>,
        roles: BTreeMap<String, Roles>,
        subteams: Option<BTreeSet<String>>,
    ) -> Result<Self, Error> {
        success(extras)?;
        let raw = extras
            .get("team_prefs")
            .and_then(Value::as_object)
            .ok_or_else(invalid)?;
        let top = raw
            .get("toplevel_prefs")
            .and_then(Value::as_object)
            .ok_or_else(invalid)?;
        let mut teams = BTreeMap::from([(scope.team().to_owned(), top.clone())]);
        // RgmF Object.entries(workspace_prefs) applies each complete map by id.
        if let Some(workspaces) = raw.get("workspace_prefs") {
            let workspaces = workspaces.as_object().ok_or_else(invalid)?;
            for (id, workspace) in workspaces {
                if !identifier(id, b'T') {
                    return Err(invalid());
                }
                let prefs = workspace.as_object().ok_or_else(invalid)?;
                teams.insert(id.into(), prefs.clone());
            }
        }
        let (org, locked) = if let Some(raw) = enterprise {
            success(raw)?;
            (
                Some(
                    raw.get("org_prefs")
                        .and_then(Value::as_object)
                        .ok_or_else(invalid)?
                        .clone(),
                ),
                Some(
                    raw.get("locked_prefs")
                        .and_then(Value::as_object)
                        .ok_or_else(invalid)?
                        .clone(),
                ),
            )
        } else {
            (None, None)
        };
        Ok(Self {
            scope: scope.clone(),
            teams,
            org,
            locked,
            unlocked: enterprise
                .and_then(|v| v.get("unlocked_prefs"))
                .map(|v| v.as_object().cloned().ok_or_else(invalid))
                .transpose()?,
            roles,
            subteams,
        })
    }
    /// CtFl.IN: locked then unlocked (undefined only), never org_prefs.
    /// Err means acquisition unavailable; Ok(None) is known absent. JSON null
    /// is observed and suppresses the unlocked fallback, as in 7+j2.
    pub fn connector_visibility(&self, config: Config) -> Result<Option<&Value>, ()> {
        let unified = config.org_unified.ok_or(())?;
        let key = "wfb_default_connector_visibility";
        if self.scope.enterprise_workspace(unified) || self.scope.team().starts_with('E') {
            let locked = self.locked.as_ref().ok_or(())?;
            if let Some(value) = locked.get(key) {
                return Ok(Some(value));
            }
            Ok(self.unlocked.as_ref().ok_or(())?.get(key))
        } else {
            Ok(self.teams.get(self.scope.team()).ok_or(())?.get(key))
        }
    }
    fn effective(&self, team: &str, key: &str) -> Result<Option<&Value>, ()> {
        if self.scope.same_enterprise(team) {
            let org = self.org.as_ref().ok_or(())?;
            let locked = self.locked.as_ref().ok_or(())?;
            if let Some(value) = org.get(key).filter(|v| !v.is_null()) {
                return Ok(Some(value));
            }
            if let Some(value) = locked.get(key).filter(|v| !v.is_null()) {
                return Ok(Some(value));
            }
        }
        Ok(self.teams.get(team).ok_or(())?.get(key))
    }
    pub fn enabled(&self) -> Decision {
        let Some(ids) = self.scope.internal_teams() else {
            return Decision::Unknown;
        };
        ids.iter().fold(Decision::Denied, |result, id| {
            result.or(match self.effective(id, "workflow_builder_enabled") {
                Ok(value) => Decision::observed(value != Some(&Value::Bool(false))),
                Err(()) => Decision::Unknown,
            })
        })
    }
    pub fn can_create(&self, unified: Option<bool>) -> Decision {
        let Some(unified) = unified else {
            return Decision::Unknown;
        };
        let restricted = self.scope.restricted();
        let key = "who_can_create_workflows";
        // t/+A enterprise default: no locked policy and a nonrestricted self.
        if self.scope.enterprise_workspace(unified) && !restricted {
            let Some(locked) = self.locked.as_ref() else {
                return Decision::Unknown;
            };
            if locked.get(key).is_none_or(Value::is_null) {
                return Decision::Allowed;
            }
        }
        let Some(ids) = self.scope.internal_teams() else {
            return Decision::Unknown;
        };
        let selected = |raw: Option<&Value>| -> bool {
            raw.and_then(Value::as_object).is_some_and(|v| {
                v.get("type")
                    .and_then(Value::as_array)
                    .is_some_and(|v| v.iter().any(|x| x.as_str() == Some("selected")))
                    && v.get("workspace")
                        .and_then(Value::as_array)
                        .is_some_and(|v| {
                            v.iter()
                                .any(|x| x.as_str().is_some_and(|x| ids.iter().any(|id| id == x)))
                        })
            })
        };
        if !restricted
            && (selected(self.locked.as_ref().and_then(|v| v.get(key)))
                || ids
                    .iter()
                    .any(|id| self.effective(id, key).is_ok_and(selected)))
        {
            return Decision::Allowed;
        }
        // CmgE's enterprise admin-page shortcut is deliberately not enabled:
        // this API is the messaging client, not the Gantry admin dashboard.
        if !unified {
            return self
                .teams
                .get(self.scope.team())
                .map(|p| self.evaluate(p.get(key), self.scope.team()))
                .unwrap_or_default();
        }
        ids.iter().fold(Decision::Denied, |result, id| {
            result.or(match self.effective(id, key) {
                Ok(value) => self.evaluate(value, id),
                Err(()) => Decision::Unknown,
            })
        })
    }
    fn evaluate(&self, value: Option<&Value>, team: &str) -> Decision {
        let regular = Decision::observed(!self.scope.restricted());
        let Some(value) = value else { return regular };
        let empty = match value {
            Value::Null => true,
            Value::String(v) => v.is_empty(),
            Value::Array(v) => v.is_empty(),
            Value::Object(v) => v.is_empty(),
            Value::Bool(_) | Value::Number(_) => true,
        };
        if empty {
            return regular;
        }
        if let Some(role) = value.as_str() {
            return self.role(role, team).unwrap_or(regular);
        }
        let Some(object) = value.as_object() else {
            return Decision::Unknown;
        };
        if object
            .get("user")
            .and_then(Value::as_array)
            .is_some_and(|v| v.iter().any(|v| v.as_str() == Some(self.scope.user())))
        {
            return Decision::Allowed;
        }
        let mut unknown = false;
        if let Some(groups) = object.get("subteam") {
            let Some(groups) = groups.as_array() else {
                return Decision::Unknown;
            };
            if let Some(memberships) = &self.subteams {
                if groups
                    .iter()
                    .any(|v| v.as_str().is_some_and(|v| memberships.contains(v)))
                {
                    return Decision::Allowed;
                }
            } else if !groups.is_empty() {
                unknown = true;
            }
        }
        let roles = match object.get("type") {
            None => Decision::Denied,
            Some(Value::Array(types)) if types.is_empty() => regular,
            Some(Value::Array(types)) => types.iter().fold(Decision::Denied, |result, v| {
                result.or(v
                    .as_str()
                    .and_then(|role| self.role(role, team))
                    .unwrap_or(Decision::Denied))
            }),
            _ => Decision::Unknown,
        };
        if unknown {
            roles.or(Decision::Unknown)
        } else {
            roles
        }
    }
    fn role(&self, name: &str, team: &str) -> Option<Decision> {
        match name {
            "ra" => Some(Decision::Allowed),
            "regular" => Some(Decision::observed(!self.scope.restricted())),
            "admin" | "owner" | "org_admin" | "org_owner" => Some(
                self.roles
                    .get(team)
                    .map(|r| {
                        Decision::observed(match name {
                            "admin" => r.workspace_admin || r.enterprise_admin,
                            "owner" => r.workspace_owner || r.enterprise_admin,
                            "org_admin" => r.enterprise_admin,
                            _ => r.enterprise_owner,
                        })
                    })
                    .unwrap_or_default(),
            ),
            _ => None,
        }
    }
}
