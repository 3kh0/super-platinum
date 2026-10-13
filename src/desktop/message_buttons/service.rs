//! Captured Emf2.getBotByMsg and rIIG.getTeamIdFromBotOrUser precedence.
//! This resolver is pure: a missing cache entry never starts work in render.
use super_platinum_core::{
    block_actions::validation::MessageActionError,
    slack::models::{Message, User},
    state::Workspace,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Service {
    pub(crate) service_id: String,
    pub(crate) app_id: String,
    pub(crate) team_id: String,
}

/// Inline profiles win as complete records, even when fields are missing.
/// Cached bot data is used only when the message has no inline profile.
/// Ordinary message dispatch still requires an explicit message.bot_id: member
/// bot-ID fallback belongs to Slack's broader bot selector, not this wire path.
pub(crate) fn resolve(
    workspace: &Workspace,
    message: &Message,
) -> Result<Service, MessageActionError> {
    let missing = MessageActionError::MissingServiceIdentity;
    let bot_id = message
        .bot_id
        .as_deref()
        .filter(|id| identifier(id))
        .ok_or(missing)?;
    if bot_id == "BSLACKBOT" {
        return Err(MessageActionError::UnsupportedButton);
    }
    let profile = match message.bot_profile.as_ref() {
        Some(profile) => profile,
        None => workspace
            .bots
            .get(bot_id, app_hint(message)?)
            .ok_or(missing)?,
    };
    // The message app_id is a lookup hint only. It does not override or veto
    // an inline profile's app identity (Emf2 returns that profile immediately).
    let app_id = profile
        .extra
        .get("app_id")
        .and_then(|v| v.as_str())
        .filter(|id| identifier(id))
        .ok_or(missing)?;
    let profile_team = match profile.extra.get("team_id") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => Some(value.as_str().ok_or(missing)?),
    };
    let team_id = if let Some(team) = profile_team.filter(|team| team_id(team, b'T')) {
        team
    } else {
        // getBotByMsg either returns inline profile or cached bot; consequently
        // rIIG's bot/profile team choices refer to that selected record here.
        let member = message.user.as_ref().and_then(|id| workspace.users.get(id));
        member_team(member, &workspace.team_id)?
    };
    if !identifier(team_id) {
        return Err(missing);
    }
    Ok(Service {
        service_id: bot_id.into(),
        app_id: app_id.into(),
        team_id: team_id.into(),
    })
}

/// A caller may enqueue this lookup outside rendering, then guard its async
/// completion by account/team/transport and the cache's own request ticket.
pub(crate) fn lookup_target(
    workspace: &Workspace,
    message: &Message,
) -> Option<(String, Option<String>)> {
    if message.bot_profile.is_some() {
        return None;
    }
    let bot_id = message.bot_id.as_deref().filter(|id| identifier(id))?;
    // Captured Emf2 excludes these built-ins from getBotById fetches. This is a
    // fetch exclusion, not a claim that every built-in shares Slackbot actions.
    if matches!(
        bot_id,
        "B01" | "BSLACKBOT" | "B03" | "BSLACKSECURITY" | "BCHANNELEXPERT"
    ) {
        return None;
    }
    let hint = app_hint(message).ok()?;
    if workspace.bots.get(bot_id, hint).is_some() {
        return None;
    }
    Some((bot_id.into(), hint.map(str::to_owned)))
}

fn app_hint(message: &Message) -> Result<Option<&str>, MessageActionError> {
    match message.extra.get("app_id") {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .filter(|id| identifier(id))
            .map(Some)
            .ok_or(MessageActionError::MissingServiceIdentity),
    }
}

fn member_team<'a>(
    member: Option<&'a User>,
    current: &'a str,
) -> Result<&'a str, MessageActionError> {
    let Some(member) = member else {
        return Ok(current);
    };
    let team = member
        .extra
        .get("team_id")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty());
    let enterprise = member.extra.get("enterprise_user");
    // pE90/isSlackbot and iXFk/isSlackAppUser return the current workspace even
    // if those special members carry another team or enterprise membership.
    if (team.is_none() && enterprise.is_none())
        || member.id == "USLACKBOT"
        || member.id == "USLACK"
        || member.extra.get("is_slackbot").and_then(|v| v.as_bool()) == Some(true)
    {
        return Ok(current);
    }
    if let Some(team) = team.filter(|team| team_id(team, b'T')) {
        return Ok(team);
    }
    if member.deleted && team.is_some_and(|team| team_id(team, b'E')) {
        return Ok(current);
    }
    let teams = enterprise
        .and_then(|value| value.get("teams"))
        .and_then(|value| value.as_array());
    if let Some(teams) = teams.filter(|teams| !teams.is_empty()) {
        if teams.iter().any(|team| team.as_str() == Some(current)) {
            return Ok(current);
        }
        return teams[0]
            .as_str()
            .filter(|id| identifier(id))
            .ok_or(MessageActionError::MissingServiceIdentity);
    }
    Ok(current)
}

// Captured uVRc validators, not a guessed prefix-only test. Current workspace
// and enterprise membership fallback values come from their canonical models.
fn team_id(value: &str, prefix: u8) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 9
        && bytes[0] == prefix
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
fn identifier(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[cfg(test)]
mod tests;
