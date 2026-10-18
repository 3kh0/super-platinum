//! hXqI member options over fRHe defaults: self, ordinary bots and guests stay
//! available. The Calendar includeBots=false policy is deliberately separate.
use super::*;
use crate::{block_actions::app_eligibility::agents::Access, slack::models::User};

#[derive(Clone)]
pub struct DirectoryUser {
    raw: Value,
    user: User,
}
impl DirectoryUser {
    pub fn decode(raw: &Value) -> Result<Self, Error> {
        if !raw.is_object()
            || !raw["id"]
                .as_str()
                .is_some_and(|v| id(v) && matches!(v.as_bytes().first(), Some(b'U' | b'W')))
        {
            return Err(invalid());
        }
        for key in [
            "is_app_user",
            "is_agentforce_bot",
            "is_slackbot",
            "is_workflow_bot",
            "is_profile_only_user",
            "is_slacksecuritybot",
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
    pub fn eligible(&self, agents: Option<&Access>) -> bool {
        let u = &self.user;
        if u.deleted
            || matches!(u.id.as_str(), "USEARCH" | "UCHANNELEXPERT" | "USLACK")
            || ["is_workflow_bot", "is_profile_only_user"]
                .iter()
                .any(|k| u.extra.get(*k) == Some(&Value::Bool(true)))
            || (u.is_restricted
                && u.extra.get("restricted_user_type").and_then(Value::as_str)
                    == Some("external_guest"))
        {
            return false;
        }
        if u.extra.get("is_agentforce_bot") == Some(&Value::Bool(true)) {
            let Some(access) = agents else { return false };
            let profile = crate::block_actions::app_eligibility::app_id(u)
                .and_then(|app| access.profile(app));
            return access.allows(u, profile);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workflow_default_includes_self_bots_slackbot_security_and_ordinary_guests() {
        for raw in [
            json!({"id":"U1"}),
            json!({"id":"USLACKBOT","is_bot":true}),
            json!({"id":"U2","is_bot":true,"is_slackbot":true}),
            json!({"id":"U3","is_bot":true,"is_slacksecuritybot":true}),
            json!({"id":"U4","is_restricted":true}),
            json!({"id":"U5","is_app_user":true,"profile":{"api_app_id":"A1"}}),
        ] {
            let choice = DirectoryUser::decode(&raw).unwrap();
            assert!(choice.eligible(None));
            assert_eq!(choice.raw(), &raw);
        }
    }
    #[test]
    fn workflow_excluded_members_and_unobserved_agent_access_cannot_be_selected() {
        for raw in [
            json!({"id":"USEARCH"}),
            json!({"id":"UCHANNELEXPERT"}),
            json!({"id":"USLACK"}),
            json!({"id":"U1","deleted":true}),
            json!({"id":"U1","is_profile_only_user":true}),
            json!({"id":"U1","is_workflow_bot":true}),
            json!({"id":"U1","is_agentforce_bot":true}),
            json!({"id":"U1","is_restricted":true,"restricted_user_type":"external_guest"}),
        ] {
            assert!(!DirectoryUser::decode(&raw).unwrap().eligible(None));
        }
        assert!(DirectoryUser::decode(&json!({"id":"U1","is_agentforce_bot":"false"})).is_err());
        assert!(DirectoryUser::decode(&json!({"id":"C1"})).is_err());
    }
}
