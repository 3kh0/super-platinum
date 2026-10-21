//! CydK local-members manager picker: includeSelf/includeAgents, not bots or
//! guests. These acquired facts are not account/query/currentness authority.
//! Existing collaborator identities belong to Draft, independently of this
//! new-choice filter or whether their display metadata could be hydrated.
use crate::{
    block_actions::{
        app_eligibility::{
            self,
            agents::{Access, Page, Profile},
        },
        workflow_management::activity_members,
    },
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient, models::User},
};
pub use activity_members::Eligibility;
use serde_json::Value;

#[derive(Clone)]
pub struct Member(activity_members::Member);
impl Member {
    pub fn decode(raw: &Value) -> Result<Self, Error> {
        Ok(Self(activity_members::Member::decode(raw)?))
    }
    pub fn raw(&self) -> &Value {
        self.0.raw()
    }
    pub fn user(&self) -> &User {
        self.0.user()
    }
    pub fn profile_request(
        &self,
        client: &SlackClient,
        session: &WorkspaceSession,
    ) -> Result<PreparedRequest, Error> {
        self.0.profile_request(client, session)
    }
    /// XSM4.TZ is the observed messaging-client context, not native-host
    /// detection. None cannot skip the Agentforce access branch.
    pub fn eligibility(
        &self,
        team: &str,
        client_context: Option<bool>,
        profile: Option<&Profile>,
        access: Option<&Access>,
    ) -> Eligibility {
        let raw = self.raw();
        let user = self.user();
        let flag = |key| raw.get(key) == Some(&Value::Bool(true));
        let local = !team.is_empty()
            && (raw["team_id"].as_str() == Some(team)
                || raw
                    .pointer("/enterprise_user/teams")
                    .and_then(Value::as_array)
                    .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(team))));
        if !local
            || flag("isUnknown")
            || flag("isNonExistent")
            || matches!(
                user.id.as_str(),
                "USLACKBOT" | "USLACKSECURITY" | "USEARCH" | "UCHANNELEXPERT" | "USLACK"
            )
            || user.deleted
            || user.is_restricted
            || [
                "is_workflow_bot",
                "is_profile_only_user",
                "is_slackbot",
                "is_slacksecuritybot",
            ]
            .iter()
            .any(|key| flag(key))
        {
            return Eligibility::Denied;
        }
        // includeSelf=true: neither actual self ID nor is_self is excluded.
        let profile = profile.or_else(|| {
            app_eligibility::app_id(user).and_then(|app| access.and_then(|a| a.profile(app)))
        });
        if flag("is_agentforce_bot") {
            let Some(client) = client_context else {
                return Eligibility::NeedsClientContext;
            };
            if client {
                let Some(access) = access else {
                    return Eligibility::NeedsAgentAccess;
                };
                if !access.allows(user, profile) {
                    return Eligibility::Denied;
                }
            }
        }
        if user.is_bot || flag("is_app_user") {
            let Some(profile) = profile else {
                return match app_eligibility::app_id(user) {
                    Some(app) => Eligibility::NeedsProfile { app: app.into() },
                    None => Eligibility::Denied,
                };
            };
            if !profile.agent_for(user) {
                return Eligibility::Denied;
            }
        }
        Eligibility::Allowed
    }
}

fn known(raw: &Value) -> Result<(), Error> {
    if !raw.is_object()
        || ["isUnknown", "isNonExistent"]
            .iter()
            .any(|key| raw.get(*key).is_some_and(|v| v != &Value::Bool(false)))
    {
        return Err(Error::Api("workflow_manager_app_unavailable".into()));
    }
    Ok(())
}
/// FVzo/MnAR refuse unknown app placeholders. Keep this intake guard local;
/// shared Activity/Home consumers and their profile semantics are unchanged.
pub fn decode_profile(app: &str, raw: &Value) -> Result<Profile, Error> {
    known(&raw["app_profile"])?;
    Profile::decode(app, raw)
}
/// An incomplete or unknown list row cannot certify accessible Agentforce
/// facts. The host still collects every page before constructing Access.
pub fn decode_page(raw: &Value) -> Result<Page, Error> {
    let page = Page::decode(raw)?;
    for row in raw["apps"].as_array().expect("Page validated array") {
        known(row)?;
    }
    Ok(page)
}

#[cfg(test)]
mod tests;
