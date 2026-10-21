//! aD6p ordinary List-thread context, separate from the no-channel Profile
//! caller. Inputs are original selector observations, not inferred permissions.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListChannel {
    /// zdO/.JM: IM or MPIM, not just MPIM.
    pub is_im_or_mpim: bool,
    /// zdO/.Zx: unknown or nonexistent channel.
    pub unknown_or_nonexistent: bool,
    /// drRZ.ju from the original current channel.
    pub externally_shared: bool,
}

/// Original 9UBA.Sv cache observation. Unknown is not known nonmembership.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListMembership {
    Unknown,
    Known(bool),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ListMemberFacts {
    /// Original QjNl.g(member) && id+5.A(member) selector result.
    pub external_bot: Option<bool>,
    pub membership: Option<ListMembership>,
}

#[derive(Clone, Debug)]
pub struct ListPolicy {
    base: Policy,
    channel: Option<ListChannel>,
    assistant_user_id: Option<String>,
    hide_external_bots: bool,
}

impl ListPolicy {
    /// `channel=None` and `assistant_user_id=None` are observed missing values,
    /// not readiness fallbacks. This is the ordinary List-record thread caller:
    /// no workflow-editor override and no huddle guest allowset. Profile's
    /// assignments/policy remain untouched.
    pub fn decode(
        raw: &Value,
        channel: Option<ListChannel>,
        assistant_user_id: Option<String>,
    ) -> Result<Self, Error> {
        let mut base = Policy::decode(raw)?;
        let on = |key: &str| match raw["assignments"].get(key) {
            None => Ok(false),
            Some(value) => value["group"]
                .as_str()
                .map(|group| group == "on")
                .ok_or_else(unavailable),
        };
        let external = on("ncea_allow_external_channels")?;
        let external_only = on("ncea_allow_only_external_channel_usage")?;
        let hide_external_bots = on("at_menu_hide_external_bots")?;
        base.expert = base.expert
            && !channel.is_some_and(|c| c.is_im_or_mpim)
            && (!channel.is_some_and(|c| c.externally_shared) || external || external_only);
        Ok(Self {
            base,
            channel,
            assistant_user_id,
            hide_external_bots,
        })
    }

    fn assistant(&self, user: &User) -> bool {
        self.channel.is_some_and(|c| c.is_im_or_mpim)
            && self.assistant_user_id.as_deref() == Some(user.id.as_str())
    }

    pub fn requirements(&self, user: &User) -> Result<Requirements, Error> {
        let mut result = self.base.requirements(user)?;
        if self.assistant(user) {
            result = Requirements {
                eligible: false,
                app_profile: false,
                agent_access: false,
                salesforce_environment: false,
            };
        }
        Ok(result)
    }

    pub fn allows(
        &self,
        user: &User,
        facts: Facts<'_>,
        list: ListMemberFacts,
    ) -> Result<bool, Error> {
        if self.assistant(user) || !self.base.allows(user, facts)? {
            return Ok(false);
        }
        let Some(channel) = self
            .channel
            .filter(|c| self.hide_external_bots && !c.unknown_or_nonexistent)
        else {
            return Ok(true);
        };
        if !list.external_bot.ok_or_else(unavailable)? {
            return Ok(true);
        }
        if !channel.externally_shared {
            return Ok(false);
        }
        Ok(!matches!(
            list.membership.ok_or_else(unavailable)?,
            ListMembership::Known(false)
        ))
    }
}

#[cfg(test)]
mod tests;
