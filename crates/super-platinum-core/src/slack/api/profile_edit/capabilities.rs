use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileEditCapability {
    Allowed,
    Denied(&'static str),
    Unknown(&'static str),
}
impl ProfileEditCapability {
    pub fn allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }
    pub fn reason(&self) -> Option<&'static str> {
        match self {
            Self::Allowed => None,
            Self::Denied(reason) | Self::Unknown(reason) => Some(reason),
        }
    }
}

/// Fresh, request-bound policy and profile values, never a persisted grant.
/// No Debug derive: profile values are private user data.
#[derive(Clone)]
pub struct FreshOwnProfileCapabilities {
    team_id: String,
    user_id: String,
    profile: UserProfile,
    profile_raw: Value,
    full_name: ProfileEditCapability,
    display_name: ProfileEditCapability,
    pronouns: ProfileEditCapability,
}
impl FreshOwnProfileCapabilities {
    pub fn team_id(&self) -> &str {
        &self.team_id
    }
    pub fn user_id(&self) -> &str {
        &self.user_id
    }
    pub fn profile(&self) -> &UserProfile {
        &self.profile
    }
    pub fn profile_raw(&self) -> &Value {
        &self.profile_raw
    }
    pub fn capability(&self, field: Field) -> &ProfileEditCapability {
        match field {
            Field::FullName => &self.full_name,
            Field::DisplayName => &self.display_name,
            Field::Pronouns => &self.pronouns,
        }
    }
    /// Pure decoder also used by disconnected fixtures. Every response must
    /// belong to these exact requests; no identity inferred from profile names.
    pub fn from_responses(
        w: &WorkspaceSession,
        boot: &Value,
        permissions: &Value,
        profile_response: &Value,
        extras_response: &Value,
        enterprise_prefs: Option<&Value>,
    ) -> Result<Self, Error> {
        let mut scope = BootScope::capture(w, boot)?;
        success(extras_response)?;
        scope.team_prefs = extras_response
            .pointer("/team_prefs/toplevel_prefs")
            .cloned();
        success(permissions)?;
        success(profile_response)?;
        let profile_raw = profile_response
            .get("profile")
            .filter(|v| v.is_object())
            .ok_or_else(|| malformed("Slack did not return your profile."))?
            .clone();
        if profile_response
            .get("user_id")
            .is_some_and(|v| v.as_str() != Some(&w.user_id))
        {
            return Err(malformed("The profile response belongs to another member."));
        }
        let profile = serde_json::from_value(profile_raw.clone())
            .map_err(|_| malformed("Slack returned an unreadable profile."))?;
        let full_name = scope.permission(permissions, "UPDATE_NAME");
        let mut display_name = scope.permission(permissions, "UPDATE_DISPLAY_NAME");
        if display_name.allowed() {
            display_name = scope.display_name_policy();
        }
        let pronouns = if scope.enterprise {
            match enterprise_prefs {
                Some(value) => {
                    success(value)?;
                    pronouns_policy(value.get("locked_prefs"))
                }
                None => {
                    ProfileEditCapability::Unknown("Organization profile settings have not loaded.")
                }
            }
        } else {
            pronouns_policy(scope.team_prefs.as_ref())
        };
        Ok(Self {
            team_id: w.team_id.clone(),
            user_id: w.user_id.clone(),
            profile,
            profile_raw,
            full_name,
            display_name,
            pronouns,
        })
    }
}

pub(super) struct BootScope {
    pub internal_teams: Vec<String>,
    pub enterprise: bool,
    top_level_org: bool,
    team_prefs: Option<Value>,
}
impl BootScope {
    pub fn capture(w: &WorkspaceSession, boot: &Value) -> Result<Self, Error> {
        success(boot)?;
        if boot.pointer("/self/id").and_then(Value::as_str) != Some(&w.user_id)
            || boot.pointer("/team/id").and_then(Value::as_str) != Some(&w.team_id)
        {
            return Err(malformed(
                "Profile capabilities belong to another workspace or member.",
            ));
        }
        let top_level_org = w.team_id.starts_with('E');
        let enterprise = top_level_org
            || w.enterprise_id.is_some()
            || boot
                .pointer("/team/enterprise_id")
                .and_then(Value::as_str)
                .is_some_and(|v| !v.is_empty());
        let mut internal_teams = Vec::new();
        // kH+p B10008176 excludes external workspaces from client.init.workspaces.
        // A missing inventory is not equivalent to the source's known empty list.
        if top_level_org {
            let teams = boot
                .get("workspaces")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    malformed("Your organization workspace inventory has not loaded.")
                })?;
            let mut seen = BTreeSet::new();
            for team in teams {
                let id = team
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|id| {
                        id.starts_with('T')
                            && id.len() > 1
                            && id.bytes().all(|b| b.is_ascii_alphanumeric())
                    })
                    .ok_or_else(|| {
                        malformed("Slack returned an invalid profile workspace inventory.")
                    })?;
                if !seen.insert(id) {
                    return Err(malformed("Slack repeated a profile workspace identity."));
                }
                if let Some(flag) = team.get("allow_external_users") {
                    if !flag.is_boolean() && !flag.is_null() {
                        return Err(malformed(
                            "Slack returned an unreadable workspace membership policy.",
                        ));
                    }
                }
                if team.get("allow_external_users").and_then(Value::as_bool) != Some(true) {
                    internal_teams.push(id.into());
                }
            }
        }
        Ok(Self {
            internal_teams,
            enterprise,
            top_level_org,
            team_prefs: None,
        })
    }
    pub fn aggregate_workspaces(&self) -> bool {
        self.top_level_org && !self.internal_teams.is_empty()
    }
    pub(super) fn permission(&self, response: &Value, permission: &str) -> ProfileEditCapability {
        let allowed = if self.aggregate_workspaces() {
            let Some(map) = response.get("allowed_per_team").and_then(Value::as_object) else {
                return ProfileEditCapability::Unknown(
                    "Workspace profile permissions have not loaded.",
                );
            };
            let mut found = false;
            for id in &self.internal_teams {
                if let Some(list) = map.get(id) {
                    let Some(ids) = permission_ids(list) else {
                        return ProfileEditCapability::Unknown(
                            "Slack returned unreadable profile permissions.",
                        );
                    };
                    found |= ids.contains(&permission);
                }
            }
            found
        } else {
            let Some(ids) = response.get("allowed").and_then(permission_ids) else {
                return ProfileEditCapability::Unknown("Profile permissions have not loaded.");
            };
            ids.contains(&permission)
        };
        if allowed {
            ProfileEditCapability::Allowed
        } else {
            ProfileEditCapability::Denied("Your organization does not allow editing this name.")
        }
    }
    fn display_name_policy(&self) -> ProfileEditCapability {
        let Some(prefs) = self.team_prefs.as_ref().and_then(Value::as_object) else {
            return ProfileEditCapability::Unknown("Workspace sign-in settings have not loaded.");
        };
        // 7MjS: KS52 isGantryManage is FALSE in the messaging client. Its
        // auth_mode comes from teamPrefs[team.id], including Grid; the admin
        // dashboard-only locked_prefs.auth_mode branch must not be substituted.
        let auth = match prefs.get("auth_mode") {
            None | Some(Value::Null) => "",
            Some(Value::String(value)) => value,
            _ => {
                return ProfileEditCapability::Unknown(
                    "Workspace sign-in settings are unreadable.",
                );
            }
        };
        let choose = match prefs.get("sso_choose_username") {
            None | Some(Value::Null) => None,
            Some(Value::Bool(value)) => Some(*value),
            _ => return ProfileEditCapability::Unknown("Workspace username policy is unreadable."),
        };
        if !auth.is_empty() && auth != "normal" && choose == Some(false) {
            ProfileEditCapability::Denied("Your display name is managed by your sign-in provider.")
        } else {
            ProfileEditCapability::Allowed
        }
    }
}
fn permission_ids(value: &Value) -> Option<Vec<&str>> {
    value.as_array()?.iter().map(Value::as_str).collect()
}
fn pronouns_policy(prefs: Option<&Value>) -> ProfileEditCapability {
    let Some(prefs) = prefs.and_then(Value::as_object) else {
        return ProfileEditCapability::Unknown("Pronoun settings have not loaded.");
    };
    match prefs.get("display_pronouns") {
        Some(Value::Bool(true)) => ProfileEditCapability::Allowed,
        Some(Value::Bool(false) | Value::Null) | None => {
            ProfileEditCapability::Denied("Pronouns are disabled in this organization.")
        }
        _ => ProfileEditCapability::Unknown("Slack returned unreadable pronoun settings."),
    }
}
