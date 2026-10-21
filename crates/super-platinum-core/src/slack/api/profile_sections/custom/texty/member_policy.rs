//! Profile aD6p options applied by 4wTh/fRHe, not Activity/Person policy.
//! Retained source: tmp/profile-rich-ui-prep/member-policy-references.json
//! (720 original-factory vectors), member-eligibility-references.json and
//! member-environment-references.json. Native client mode is XSM4.TZ=true.
use crate::{
    block_actions::app_eligibility,
    slack::{Error, models::User},
};
use serde_json::Value;
pub mod environment;
mod list;
pub use list::{ListChannel, ListMemberFacts, ListMembership, ListPolicy};
#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub struct Policy {
    slackbot: bool,
    search: bool,
    expert: bool,
    acl: bool,
    exclude_salesforce: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Requirements {
    pub eligible: bool,
    pub app_profile: bool,
    pub agent_access: bool,
    pub salesforce_environment: bool,
}
/// Each Some value is an observation, not a default for unavailable data.
#[derive(Clone, Copy, Default)]
pub struct Facts<'a> {
    pub restricted_app: Option<bool>,
    pub agent_allowed: Option<bool>,
    pub salesforce_app: Option<&'a str>,
}
fn unavailable() -> Error {
    Error::Api("profile_member_policy_unavailable".into())
}
pub fn positive(raw: &Value) -> bool {
    raw.is_object()
        && raw.get("ok") == Some(&Value::Bool(true))
        && raw.get("error").is_none_or(Value::is_null)
}
fn flag(user: &User, key: &str) -> Result<bool, Error> {
    match user.extra.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(v)) => Ok(*v),
        _ => Err(unavailable()),
    }
}
pub fn member_id(id: &str) -> bool {
    matches!(id, "USEARCH" | "UCHANNELEXPERT" | "USLACK")
        || crate::block_actions::form::valid_message_user(id)
}
/// Self, ordinary bots, guests, active profile-only people and deleted humans
/// are permitted. Deleted bots/profile-only rows and workflow bots are not.
pub fn member_base(user: &User) -> Result<bool, Error> {
    if !member_id(&user.id) {
        return Err(unavailable());
    }
    let workflow = flag(user, "is_workflow_bot")?;
    let profile_only = flag(user, "is_profile_only_user")?;
    let app = flag(user, "is_app_user")?;
    let guest_type = match user.extra.get("restricted_user_type") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.as_str()),
        _ => return Err(unavailable()),
    };
    Ok(!workflow
        && !(user.deleted && (user.is_bot || app || user.id == "USLACK"))
        && !(user.is_restricted && guest_type == Some("external_guest"))
        && !(profile_only && user.deleted))
}
impl Policy {
    pub fn decode(raw: &Value) -> Result<Self, Error> {
        if !positive(raw) {
            return Err(unavailable());
        }
        let rows = raw
            .get("assignments")
            .and_then(Value::as_object)
            .ok_or_else(unavailable)?;
        let on = |key: &str| -> Result<bool, Error> {
            match rows.get(key) {
                None => Ok(false),
                Some(v) => v
                    .get("group")
                    .and_then(Value::as_str)
                    .map(|v| v == "on")
                    .ok_or_else(unavailable),
            }
        };
        // Both operands must be valid even when the first decides the result.
        let slackbot = on("slackbot_in_channels")?;
        let awareness = on("slackbot_in_channels_team_awareness")?;
        let acl_search = on("loom_app_acl_search_filter")?;
        let acl_all = on("app_acls_for_all_apps")?;
        Ok(Self {
            slackbot: slackbot || awareness,
            search: on("enable_search_system_user")?,
            expert: on("enable_native_channel_expert")?,
            acl: acl_search && acl_all,
            exclude_salesforce: on("slackforce_seamless_block_app_membership")?,
        })
    }
    pub fn requirements(&self, user: &User) -> Result<Requirements, Error> {
        let base = member_base(user)?;
        let slackbot = flag(user, "is_slackbot")? || user.id == "USLACKBOT";
        let agent = flag(user, "is_agentforce_bot")?;
        let eligible = base
            && user.id != "USLACK"
            && (!slackbot || self.slackbot)
            && (user.id != "USEARCH" || self.search)
            && (user.id != "UCHANNELEXPERT" || self.expert);
        Ok(Requirements {
            eligible,
            app_profile: eligible && (self.acl || agent) && app_eligibility::app_id(user).is_some(),
            agent_access: eligible && agent,
            salesforce_environment: eligible && self.exclude_salesforce,
        })
    }
    pub fn allows(&self, user: &User, facts: Facts<'_>) -> Result<bool, Error> {
        let needs = self.requirements(user)?;
        if !needs.eligible {
            return Ok(false);
        }
        if needs.agent_access && !facts.agent_allowed.ok_or_else(unavailable)? {
            return Ok(false);
        }
        if self.acl
            && app_eligibility::app_id(user).is_some()
            && facts.restricted_app.ok_or_else(unavailable)?
        {
            return Ok(false);
        }
        if needs.salesforce_environment {
            let app = facts.salesforce_app.ok_or_else(unavailable)?;
            if app_eligibility::app_id(user) == Some(app) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
