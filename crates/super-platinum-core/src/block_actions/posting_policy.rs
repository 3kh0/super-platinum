//! Current Slack xsoG preference cascade and gq7X/Vm7I plan adjustment.
//! This is a pure selection policy, not a server permission or dispatch capability.
//! Account/visit freshness, destination validity and app-DM policy remain callers' work.
use serde_json::Value;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unknown {
    Identity,
    Shape,
    Member,
    Plan,
    FeatureAccess,
    ChannelAdmin,
    EffectiveFlags,
}
pub type Result<T> = std::result::Result<T, Unknown>;

/// A result from the actual JM7a/Bn/N current-user role selectors for this channel.
/// In Grid, `member.is_admin` alone does not establish this fact. The producer
/// must resolve the channel's relevant workspaces and their current role stores.
#[derive(Clone, Copy)]
pub struct ChannelAdmin<'a> {
    pub channel_id: &'a str,
    pub user_id: &'a str,
    pub allowed: bool,
}

/// Raw current-account observations. Missing data is not an inferred denial or
/// grant. The caller must keep these observations under one current-owner fence.
pub struct Context<'a> {
    pub team_id: &'a str,
    /// v7Vi.Kc: enterpriseId, boot enterprise_id, or teamId, in that order.
    pub top_level_team_id: &'a str,
    pub user_id: &'a str,
    /// W5jp.H7's actual selected team object, preserving raw plan/enterprise_id.
    pub team: &'a Value,
    pub member: &'a Value,
    /// t5X0.r7 includes an enterprise-ID team in unified mode.
    pub unified: bool,
    /// Actual featureAccess.manage_channel_posting__access.is_enabled.
    pub manage_posting_access: Option<bool>,
    pub channel_admin: Option<ChannelAdmin<'a>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flags {
    pub read_only: bool,
    pub thread_only: bool,
}
impl Flags {
    pub fn permits_root_post(self) -> bool {
        !self.read_only && !self.thread_only
    }
}

/// Independently retained effective flags, analogous to Slack's two stores.
/// A new unknown record is deliberately not initialized from selector defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observed {
    channel_id: String,
    read_only: Option<bool>,
    thread_only: Option<bool>,
}
impl Observed {
    pub fn unknown(channel_id: String) -> Self {
        Self {
            channel_id,
            read_only: None,
            thread_only: None,
        }
    }
    /// Complete read_only_channels/thread_only_channels inventory observations
    /// may supply explicit false for absent IDs; a partial channel cannot.
    pub fn from_inventory(channel_id: String, read_only: bool, thread_only: bool) -> Self {
        Self {
            channel_id,
            read_only: Some(read_only),
            thread_only: Some(thread_only),
        }
    }
    pub fn values(&self) -> (Option<bool>, Option<bool>) {
        (self.read_only, self.thread_only)
    }
    /// Retire individual facts whose acquisition dependency changed. This can
    /// only remove knowledge; it never supplies a missing false or a grant.
    pub fn forget_values(&mut self, read_only: bool, thread_only: bool) {
        if read_only {
            self.read_only = None;
        }
        if thread_only {
            self.thread_only = None;
        }
    }
    pub fn observe(&mut self, channel: &Value, ctx: &Context<'_>) -> Result<()> {
        ctx.check(channel)?;
        if channel["id"].as_str() != Some(self.channel_id.as_str()) {
            return Err(Unknown::Identity);
        }
        let read = channel.get("is_read_only");
        let thread = channel.get("is_thread_only");
        // 340b gives effective booleans precedence. Present malformed companion
        // retires its previous fact; omission preserves a known store value.
        if read.is_some_and(Value::is_boolean) || thread.is_some_and(Value::is_boolean) {
            if let Some(v) = read {
                self.read_only = v.as_bool();
            }
            if let Some(v) = thread {
                self.thread_only = v.as_bool();
            }
            return Ok(());
        }
        let props = match properties(channel) {
            Ok(props) => props,
            Err(error) => {
                self.read_only = None;
                self.thread_only = None;
                return Err(error);
            }
        };
        let post = props.and_then(|p| p.get("posting_restricted_to"));
        let reply = props.and_then(|p| p.get("threads_restricted_to"));
        if post.is_some_and(|v| !v.is_null()) || reply.is_some_and(|v| !v.is_null()) {
            match preference_flags(channel, post, reply, ctx) {
                Ok(f) => {
                    self.read_only = Some(f.read_only);
                    self.thread_only = Some(f.thread_only);
                }
                Err(e) => {
                    self.read_only = None;
                    self.thread_only = None;
                    return Err(e);
                }
            }
        } else if read.is_some() || thread.is_some() {
            self.read_only = None;
            self.thread_only = None;
            return Err(Unknown::Shape);
        }
        Ok(())
    }
    /// gq7X.P only, used by pS2E.canThread. Unlike root-post effective(),
    /// this must not force file/huddle read-only to false.
    pub fn thread_read_only(&self, channel: &Value, ctx: &Context<'_>) -> Result<bool> {
        ctx.check(channel)?;
        if channel["id"].as_str() != Some(self.channel_id.as_str()) {
            return Err(Unknown::Identity);
        }
        adjust(self.read_only.ok_or(Unknown::EffectiveFlags)?, channel, ctx)
    }

    /// Applies the captured plan/general/external-host exceptions after the
    /// preference or effective-flag observation. File and huddle root posting
    /// remains refused (FS2H always rejects huddle channels independently).
    pub fn effective(&self, channel: &Value, ctx: &Context<'_>) -> Result<Flags> {
        ctx.check(channel)?;
        if channel["id"].as_str() != Some(self.channel_id.as_str()) {
            return Err(Unknown::Identity);
        }
        let file = flag(channel, "is_file")?;
        let huddle = properties(channel)?.and_then(|p| p.get("huddles"));
        let huddle = match huddle {
            None => false,
            Some(v) if v.is_object() => flag(v, "is_huddle_channel")?,
            Some(_) => return Err(Unknown::Shape),
        };
        if file || huddle {
            return Ok(Flags {
                read_only: false,
                thread_only: true,
            });
        }
        Ok(Flags {
            read_only: adjust(self.read_only.ok_or(Unknown::EffectiveFlags)?, channel, ctx)?,
            thread_only: adjust(
                self.thread_only.ok_or(Unknown::EffectiveFlags)?,
                channel,
                ctx,
            )?,
        })
    }
}

fn adjust(value: bool, channel: &Value, ctx: &Context<'_>) -> Result<bool> {
    if !value {
        return Ok(false);
    }
    if ctx.free_or_standard()? && !flag(channel, "is_general")? {
        // This branch precedes IM/MPIM normalization in the captured selectors.
        return Ok(external_host(channel, ctx)?
            || ctx.manage_posting_access.ok_or(Unknown::FeatureAccess)?);
    }
    if flag(channel, "is_im")? || flag(channel, "is_mpim")? {
        return Ok(false);
    }
    Ok(true)
}

pub fn preference_flags(
    channel: &Value,
    post: Option<&Value>,
    reply: Option<&Value>,
    ctx: &Context<'_>,
) -> Result<Flags> {
    ctx.check(channel)?;
    let can_post = restriction(channel, post, ctx)?;
    let can_reply = restriction(channel, reply, ctx)?;
    Ok(Flags {
        read_only: !can_post && !can_reply,
        thread_only: !can_post && can_reply,
    })
}

fn restriction(channel: &Value, pref: Option<&Value>, ctx: &Context<'_>) -> Result<bool> {
    let Some(pref) = pref.filter(|p| !p.is_null()) else {
        return Ok(true);
    };
    let obj = pref.as_object().ok_or(Unknown::Shape)?;
    if obj.is_empty() {
        return Ok(false);
    }
    if flag(ctx.member, "isNonExistent")? {
        return Ok(false);
    }
    if flag(ctx.member, "isUnknown")? {
        return Err(Unknown::Member);
    }
    if let Some(users) = obj.get("user") {
        let users = users.as_array().ok_or(Unknown::Shape)?;
        if users.iter().any(|u| !u.is_string()) {
            return Err(Unknown::Shape);
        }
        if users.iter().any(|u| u.as_str() == Some(ctx.user_id)) {
            return Ok(true);
        }
    }
    let roles = obj
        .get("role")
        .or_else(|| obj.get("type"))
        .ok_or(Unknown::Shape)?;
    let roles = roles.as_array().ok_or(Unknown::Shape)?;
    let role = roles
        .first()
        .and_then(Value::as_str)
        .ok_or(Unknown::Shape)?;
    match role {
        "all" | "ra" => Ok(true),
        "regular" => Ok(!external_host(channel, ctx)? && !flag(ctx.member, "is_restricted")?),
        "admin" => {
            if external_host(channel, ctx)? {
                return Ok(false);
            }
            let admin = ctx.channel_admin.ok_or(Unknown::ChannelAdmin)?;
            if admin.user_id != ctx.user_id || Some(admin.channel_id) != channel["id"].as_str() {
                return Err(Unknown::Identity);
            }
            Ok(admin.allowed)
        }
        "org_admin" => {
            if external_host(channel, ctx)? {
                return Ok(false);
            }
            match ctx.member.get("enterprise_user") {
                None => Ok(false),
                Some(v) if v.is_object() => flag(v, "is_admin"),
                Some(_) => Err(Unknown::Shape),
            }
        }
        // Slack's JS default is permissive for unrecognized roles. We do not
        // turn a future or malformed permission schema into a local grant.
        _ => Err(Unknown::Shape),
    }
}

fn external_host(channel: &Value, ctx: &Context<'_>) -> Result<bool> {
    if !flag(channel, "is_ext_shared")? {
        return Ok(false);
    }
    match channel.get("conversation_host_id") {
        None | Some(Value::Null) => Ok(false),
        Some(Value::String(host)) => {
            Ok(!host.is_empty() && host != ctx.team_id && host != ctx.top_level_team_id)
        }
        Some(_) => Err(Unknown::Shape),
    }
}
impl Context<'_> {
    fn check(&self, channel: &Value) -> Result<()> {
        if self.team_id.is_empty()
            || self.top_level_team_id.is_empty()
            || self.user_id.is_empty()
            || self.team["id"].as_str() != Some(self.team_id)
            || self.member["id"].as_str() != Some(self.user_id)
            || channel["id"].as_str().is_none_or(str::is_empty)
        {
            return Err(Unknown::Identity);
        }
        if !channel.is_object() || !self.team.is_object() || !self.member.is_object() {
            return Err(Unknown::Shape);
        }
        Ok(())
    }
    fn free_or_standard(&self) -> Result<bool> {
        let plan = self.team.get("plan");
        if matches!(plan, Some(Value::String(s)) if s == "std") {
            return Ok(true);
        }
        if matches!(plan, Some(Value::String(s)) if !s.is_empty()) {
            return Ok(false);
        }
        if !matches!(plan, None | Some(Value::Null) | Some(Value::String(_))) {
            return Err(Unknown::Plan);
        }
        // Raw current team observation: an omitted plan is the captured free
        // shape, except an enterprise workspace or unified enterprise object.
        let enterprise = match self.team.get("enterprise_id") {
            None | Some(Value::Null) => false,
            Some(Value::String(s)) => !s.is_empty(),
            Some(_) => return Err(Unknown::Plan),
        };
        Ok(!enterprise && !(self.unified && self.team_id.starts_with('E')))
    }
}
fn properties(channel: &Value) -> Result<Option<&serde_json::Map<String, Value>>> {
    match channel.get("properties") {
        None => Ok(None),
        Some(v) => v.as_object().map(Some).ok_or(Unknown::Shape),
    }
}
fn flag(raw: &Value, key: &str) -> Result<bool> {
    match raw.get(key) {
        None => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(Unknown::Shape),
    }
}
