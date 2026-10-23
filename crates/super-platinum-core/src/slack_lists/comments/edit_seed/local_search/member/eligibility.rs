use super::{Error, observed};

/// Actual selector results, not a wire `facts` grant. None is unavailable and
/// is required only when the original fixed-options predicate reaches it.
#[derive(Clone, Copy, Default)]
pub struct MemberFacts {
    pub interaction_allowed: Option<bool>,
    pub slackbot: Option<bool>,
    pub system_notification: Option<bool>,
    pub search_bot: Option<bool>,
    pub channel_expert_bot: Option<bool>,
    pub agent_environment: Option<bool>,
    pub agent: Option<bool>,
    pub admitted_agent: Option<bool>,
    pub workflow: Option<bool>,
    pub deleted: Option<bool>,
    pub external_huddle_guest: Option<bool>,
    pub salesforce_app: Option<bool>,
    pub profile_only: Option<bool>,
}
impl MemberFacts {
    pub fn allows(&self, salesforce_blocked: bool) -> Result<bool, Error> {
        if !observed(self.interaction_allowed, "member interaction")? {
            return Ok(false);
        }
        let mut allowed = true;
        for (flag, name) in [
            (self.slackbot, "Slackbot"),
            (self.system_notification, "system notification"),
            (self.search_bot, "search bot"),
            (self.channel_expert_bot, "channel expert bot"),
        ] {
            if allowed {
                allowed = !observed(flag, name)?;
            }
        }
        // Original getMemberFilterFunction visits the agent environment even
        // after the preceding predicate became false.
        if observed(self.agent_environment, "agent environment")? && observed(self.agent, "agent")?
        {
            allowed = observed(self.admitted_agent, "agent catalog admission")? && allowed;
        }
        for (flag, name) in [
            (self.workflow, "workflow"),
            (self.deleted, "deleted"),
            (self.external_huddle_guest, "external huddle guest"),
        ] {
            if allowed {
                allowed = !observed(flag, name)?;
            }
        }
        if allowed && salesforce_blocked {
            allowed = !observed(self.salesforce_app, "Salesforce app")?;
        }
        if allowed {
            allowed = !observed(self.profile_only, "profile-only member")?;
        }
        Ok(allowed)
    }
}
#[derive(Clone, Copy, Default)]
pub struct GroupFacts {
    pub from_another_team: Option<bool>,
    pub active: Option<bool>,
}
impl GroupFacts {
    pub fn allows(&self, unified_user: bool) -> Result<bool, Error> {
        if !unified_user && observed(self.from_another_team, "group locality")? {
            return Ok(false);
        }
        observed(self.active, "active group")
    }
}
