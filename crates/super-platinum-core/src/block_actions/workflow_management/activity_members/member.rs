use super::*;
use crate::{
    block_actions::app_eligibility::{
        self,
        agents::{Access, Profile},
    },
    slack::models::User,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Eligibility {
    Allowed,
    Denied,
    NeedsProfile { app: String },
    NeedsAgentAccess,
    NeedsClientContext,
}
#[derive(Clone)]
pub struct Member {
    raw: Value,
    user: User,
}
impl Member {
    pub fn decode(raw: &Value) -> Result<Self, Error> {
        if !raw.is_object() || raw["id"].as_str().is_none_or(|s| !id(s)) {
            return Err(invalid());
        }
        for key in [
            "is_self",
            "is_app_user",
            "is_bot",
            "is_agentforce_bot",
            "is_slackbot",
            "is_slacksecuritybot",
            "is_workflow_bot",
            "is_profile_only_user",
            "deleted",
            "is_restricted",
        ] {
            if raw.get(key).is_some_and(|v| !v.is_boolean()) {
                return Err(invalid());
            }
        }
        let user = serde_json::from_value(raw.clone()).map_err(|_| invalid())?;
        Ok(Self {
            raw: raw.clone(),
            user,
        })
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn user(&self) -> &User {
        &self.user
    }
    /// AjOk inherits includeSelf=false/includeAgents=true. This is the actual
    /// 8Bbq predicate, not the Home workflow includeBots=true policy.
    pub fn eligibility(
        &self,
        current_user: &str,
        client_context: Option<bool>,
        profile: Option<&Profile>,
        access: Option<&Access>,
    ) -> Eligibility {
        let u = &self.user;
        let flag = |key| self.raw.get(key) == Some(&Value::Bool(true));
        if u.id == current_user
            || flag("is_self")
            || matches!(
                u.id.as_str(),
                "USLACKBOT" | "USEARCH" | "UCHANNELEXPERT" | "USLACK" | "USLACKSECURITY"
            )
            || u.deleted
            || [
                "is_slackbot",
                "is_slacksecuritybot",
                "is_workflow_bot",
                "is_profile_only_user",
            ]
            .iter()
            .any(|k| flag(k))
            || (u.is_restricted && self.raw["restricted_user_type"] == "external_guest")
        {
            return Eligibility::Denied;
        }
        let profile = profile.or_else(|| {
            app_eligibility::app_id(u).and_then(|app| access.and_then(|a| a.profile(app)))
        });
        if flag("is_agentforce_bot") {
            let Some(client) = client_context else {
                return Eligibility::NeedsClientContext;
            };
            if client {
                let Some(access) = access else {
                    return Eligibility::NeedsAgentAccess;
                };
                if !access.allows(u, profile) {
                    return Eligibility::Denied;
                }
            }
        }
        if u.is_bot || flag("is_app_user") {
            let Some(profile) = profile else {
                return match app_eligibility::app_id(u) {
                    Some(app) => Eligibility::NeedsProfile { app: app.into() },
                    None => Eligibility::Denied,
                };
            };
            if !profile.agent_for(u) {
                return Eligibility::Denied;
            }
        }
        Eligibility::Allowed
    }
    pub fn profile_request(
        &self,
        c: &SlackClient,
        w: &WorkspaceSession,
    ) -> Result<PreparedRequest, Error> {
        app_eligibility::profile_request(
            c,
            w,
            app_eligibility::app_id(&self.user).ok_or_else(invalid)?,
        )
    }
}
