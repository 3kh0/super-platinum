//! Captured HeDt/OYey gates. These are fresh observations, not mutation permits.
//! The desktop owner binds every response/config snapshot to its account,
//! credential, visit and input generation and repeats that check before action.
mod preferences;
mod producers;
mod reads;
use crate::{config::WorkspaceSession, slack::Error};
pub use preferences::{PreferenceFacts, Roles};
pub use producers::{MembershipRead, RoleRead};
pub use reads::{PermissionObservation, PermissionRead, PolicyReads, Scope};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Decision {
    Allowed,
    Denied,
    #[default]
    Unknown,
}
impl Decision {
    pub fn allowed(self) -> bool {
        self == Self::Allowed
    }
    pub fn observed(value: bool) -> Self {
        if value { Self::Allowed } else { Self::Denied }
    }
    fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Denied, _) | (_, Self::Denied) => Self::Denied,
            (Self::Allowed, Self::Allowed) => Self::Allowed,
            _ => Self::Unknown,
        }
    }
    fn or(self, other: Self) -> Self {
        match (self, other) {
            (Self::Allowed, _) | (_, Self::Allowed) => Self::Allowed,
            (Self::Denied, Self::Denied) => Self::Denied,
            _ => Self::Unknown,
        }
    }
}

/// None means acquisition/config currentness is unavailable, not an off group.
/// Both flags are exact `_Z(...) == "on"` source observations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Config {
    pub slackforce_pnp_wfb: Option<bool>,
    pub wfb_hermes_trials: Option<bool>,
    pub org_unified: Option<bool>,
}
impl Config {
    /// Explicit ordinary client-v2 context, not an inference from enterprise
    /// membership. Its registered Gantry metadata defaults unified to true
    /// (client-v2-unified-policy-handoff 60e3). Other contexts use decode.
    pub fn ordinary_client_v2(assignments: &Value) -> Result<Self, Error> {
        Self::decode(assignments, Some(true))
    }
    /// `unified` is the captured T68h registered application metadata value.
    pub fn decode(raw: &Value, unified: Option<bool>) -> Result<Self, Error> {
        success(raw)?;
        let assignments = raw
            .get("assignments")
            .and_then(Value::as_object)
            .ok_or_else(invalid)?;
        let enabled = |name: &str| -> Result<bool, Error> {
            match assignments.get(name) {
                None => Ok(false),
                Some(value) => value
                    .get("group")
                    .and_then(Value::as_str)
                    .map(|group| group == "on")
                    .ok_or_else(invalid),
            }
        };
        Ok(Self {
            slackforce_pnp_wfb: Some(enabled("slackforce_pnp_wfb")?),
            wfb_hermes_trials: Some(enabled("wfb_hermes_trials")?),
            org_unified: unified,
        })
    }
}

#[derive(Clone)]
pub struct FeaturePolicies {
    scope: Scope,
    read: bool,
    write: bool,
}
impl FeaturePolicies {
    pub fn decode(scope: &Scope, raw: &Value) -> Result<Self, Error> {
        success(raw)?;
        let policies = raw
            .get("feature_policies")
            .and_then(Value::as_object)
            .ok_or_else(invalid)?;
        let enabled = |name: &str| -> Result<bool, Error> {
            let Some(value) = policies.get(name) else {
                return Ok(false);
            };
            let object = value.as_object().ok_or_else(invalid)?;
            Ok(object.get("is_enabled").is_some_and(truthy))
        };
        Ok(Self {
            scope: scope.clone(),
            read: enabled("hermes__read")?,
            write: enabled("hermes__write")?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Access {
    can_create: Decision,
    can_view_managed: Decision,
    can_enter_editor: Decision,
}

/// Owned input snapshot for desktop final-dispatch comparison. No Debug or
/// serialization: it contains private account credentials and policy values.
#[derive(Clone)]
pub struct Facts {
    scope: Scope,
    config: Config,
    policies: Option<FeaturePolicies>,
    preferences: Option<PreferenceFacts>,
    permission: Option<PermissionObservation>,
}
impl Facts {
    pub fn capture(
        scope: Scope,
        config: Config,
        policies: Option<FeaturePolicies>,
        preferences: Option<PreferenceFacts>,
        permission: Option<PermissionObservation>,
    ) -> Result<Self, Error> {
        if policies
            .as_ref()
            .is_some_and(|p| !p.scope.same_observation(&scope))
            || preferences
                .as_ref()
                .is_some_and(|p| !p.matches_scope(&scope))
            || permission
                .as_ref()
                .is_some_and(|p| !p.matches_scope(&scope))
        {
            return Err(invalid());
        }
        Ok(Self {
            scope,
            config,
            policies,
            preferences,
            permission,
        })
    }
    pub fn access(&self) -> Access {
        Access::observe(
            &self.scope,
            self.config,
            self.policies.as_ref(),
            self.preferences.as_ref(),
            self.permission.as_ref(),
        )
    }
    pub fn current(&self, w: &WorkspaceSession, config: Config) -> bool {
        self.scope.current(w) && self.config == config
    }
    pub fn same_inputs(&self, other: &Self) -> bool {
        self.scope.same_observation(&other.scope)
            && self.config == other.config
            && self.policies.as_ref().map(|p| (p.read, p.write))
                == other.policies.as_ref().map(|p| (p.read, p.write))
            && match (&self.preferences, &other.preferences) {
                (None, None) => true,
                (Some(a), Some(b)) => a.same_inputs(b),
                _ => false,
            }
            && self
                .permission
                .as_ref()
                .map(PermissionObservation::decision)
                == other
                    .permission
                    .as_ref()
                    .map(PermissionObservation::decision)
    }
}
impl Access {
    pub fn unknown() -> Self {
        Self {
            can_create: Decision::Unknown,
            can_view_managed: Decision::Unknown,
            can_enter_editor: Decision::Unknown,
        }
    }
    /// Policy/currentness is deliberately not cached or serialized here.
    pub fn observe(
        scope: &Scope,
        config: Config,
        policies: Option<&FeaturePolicies>,
        preferences: Option<&PreferenceFacts>,
        permission: Option<&PermissionObservation>,
    ) -> Self {
        let policies = policies.filter(|p| p.scope.same_observation(scope));
        let preferences = preferences.filter(|p| p.matches_scope(scope));
        let permission = permission
            .filter(|p| p.matches_scope(scope))
            .map(PermissionObservation::decision)
            .unwrap_or_default();
        let policy_read = policies
            .map(|p| Decision::observed(p.read))
            .unwrap_or_default();
        let tier = match config.slackforce_pnp_wfb {
            None => Decision::Unknown,
            Some(true) => policies
                .map(|p| Decision::observed(p.write))
                .unwrap_or_default(),
            Some(false) => Decision::observed(scope.paid_tier()),
        };
        let enabled = preferences
            .map(PreferenceFacts::enabled)
            .unwrap_or_default();
        let preference = preferences
            .map(|p| p.can_create(config.org_unified))
            .unwrap_or_default();
        let can_create = tier.and(enabled).and(preference.or(permission));
        let trial = config
            .wfb_hermes_trials
            .map(Decision::observed)
            .unwrap_or_default();
        let can_enter_editor = policy_read.and(can_create.or(trial));
        Self {
            can_create,
            can_enter_editor,
            can_view_managed: can_enter_editor.and(Decision::observed(!scope.restricted())),
        }
    }
    pub fn can_create(&self) -> Decision {
        self.can_create
    }
    pub fn can_view_managed(&self) -> Decision {
        self.can_view_managed
    }
    pub fn can_enter_editor(&self) -> Decision {
        self.can_enter_editor
    }
}

fn invalid() -> Error {
    Error::Api("workflow_capability_invalid".into())
}
fn success(raw: &Value) -> Result<(), Error> {
    if raw.is_object()
        && raw.get("ok") == Some(&Value::Bool(true))
        && raw.get("error").is_none_or(Value::is_null)
    {
        Ok(())
    } else {
        Err(invalid())
    }
}
fn truthy(raw: &Value) -> bool {
    match raw {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_some_and(|v| v != 0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}
fn identifier(value: &str, prefix: u8) -> bool {
    value.len() >= 9
        && value.len() <= 255
        && value.as_bytes()[0] == prefix
        && value
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
fn same_owner(a: &WorkspaceSession, b: &WorkspaceSession) -> bool {
    a.team_id == b.team_id
        && a.enterprise_id == b.enterprise_id
        && a.user_id == b.user_id
        && a.url == b.url
        && a.token == b.token
}
#[cfg(test)]
mod tests;
