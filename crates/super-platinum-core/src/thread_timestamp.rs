//! Isolated ordinary-root timestamp target DATA, never navigation authority.
//!
//! The native caller owns original getView intake, exact fresh reads, canonical
//! joined-target admission, ingress/currentness and the eventual invocation.
//! This NEW object has no predecessor and never enters a shared model/cache.
use serde_json::Value;

const UNAVAILABLE: &str = "Timestamp navigation data is unavailable.";
type Result<T> = std::result::Result<T, &'static str>;

pub struct Target {
    team: String,
    channel: String,
    timestamp: String,
}
impl Target {
    /// Only original whole exact singleton history/info responses qualify.
    /// Neither a DTO, Copy target nor matching cached ids supply that lineage.
    pub fn from_fresh(
        team: &str,
        channel: &str,
        ts: &str,
        history: &Value,
        info: &Value,
    ) -> Result<Self> {
        if !uppercase_id(team, b'T', 9) || !channel_id(channel) || !timestamp(ts) {
            return Err(UNAVAILABLE);
        }
        envelope(history)?;
        envelope(info)?;
        optional_id(history, "channel", channel)?;
        if history.get("has_more").is_some_and(|v| !v.is_boolean())
            || history.get("unchanged_messages").is_some()
            || history
                .get("deleted")
                .is_some_and(|v| !v.as_array().is_some_and(Vec::is_empty))
        {
            return Err(UNAVAILABLE);
        }
        let messages = history
            .get("messages")
            .and_then(Value::as_array)
            .ok_or(UNAVAILABLE)?;
        if messages.len() != 1 {
            return Err(UNAVAILABLE);
        }
        ordinary_root(&messages[0], channel, ts)?;
        ordinary_channel(info.get("channel").ok_or(UNAVAILABLE)?, team, channel)?;
        Ok(Self {
            team: team.into(),
            channel: channel.into(),
            timestamp: ts.into(),
        })
    }
    pub fn team(&self) -> &str {
        &self.team
    }
    pub fn channel(&self) -> &str {
        &self.channel
    }
    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }
}

fn envelope(raw: &Value) -> Result<()> {
    if !raw.is_object()
        || raw.get("ok") != Some(&Value::Bool(true))
        || raw.get("error").is_some_and(|v| !v.is_null())
    {
        return Err(UNAVAILABLE);
    }
    Ok(())
}
fn ordinary_root(raw: &Value, channel: &str, ts: &str) -> Result<()> {
    if !raw.is_object()
        || raw.get("type").and_then(Value::as_str) != Some("message")
        || raw.get("ts").and_then(Value::as_str) != Some(ts)
        || raw.get("subtype").is_some_and(|v| !v.is_null())
        || raw
            .get("thread_ts")
            .is_some_and(|v| !v.is_null() && v.as_str() != Some("") && v.as_str() != Some(ts))
    {
        return Err(UNAVAILABLE);
    }
    optional_id(raw, "channel", channel)?;
    // Named original whole-history input, not a warm Source model. Fresh vkwm
    // excludes outgoing EKM/sentinel state; API retrieval is non-ephemeral.
    // Present contradictory/malformed values still refuse. FHLi/w4o6/3YDY
    // consume ts/thread_ts only: no root-bag/count/user inference is needed.
    for key in [
        "is_ephemeral",
        "isEKMError",
        "isUnknown",
        "isNonExistent",
        "hidden",
        "is_hidden",
        "deleted",
        "is_deleted",
        "is_tombstone",
        "is_tombstoned",
    ] {
        absent_or_false(raw, key)?;
    }
    Ok(())
}
fn ordinary_channel(raw: &Value, team: &str, channel: &str) -> Result<()> {
    let name = raw.get("name").and_then(Value::as_str).ok_or(UNAVAILABLE)?;
    if !raw.is_object()
        || raw.get("id").and_then(Value::as_str) != Some(channel)
        || name.is_empty()
        || name.trim() != name
        || name.len() > 255
        || name.chars().any(char::is_control)
        || raw.get("is_channel") != Some(&Value::Bool(true))
        || raw.get("is_member") != Some(&Value::Bool(true))
        || raw.get("is_private") != Some(&Value::Bool(false))
        || raw.get("is_org_shared") != Some(&Value::Bool(false))
        || raw.get("context_team_id").and_then(Value::as_str) != Some(team)
    {
        return Err(UNAVAILABLE);
    }
    // Closed ZGtP NEW constructor establishes knownness from the actual name,
    // normalizes IM/group/MPIM and preserves absent is_file. BFPz and9TcS then
    // take the ordinary nonfile/nonIM branch. These folds never clear a prior
    // canonical deny and do not grant membership/navigation on their own.
    for key in [
        "is_im",
        "is_group",
        "is_mpim",
        "is_file",
        "is_app",
        "is_app_home",
        "is_archived",
        "is_frozen",
        "isUnknown",
        "isNonExistent",
        "hidden",
        "is_hidden",
        "is_tombstone",
        "is_tombstoned",
        "deleted",
        "is_deleted",
    ] {
        absent_or_false(raw, key)?;
    }
    for key in [
        "internalTeamIds",
        "fromAnotherTeam",
        "contextTeamId",
        "isOrgShared",
    ] {
        if raw.get(key).is_some() {
            return Err(UNAVAILABLE);
        }
    }
    // Fresh optional-list normalization is field-specific: exact frozen[]
    // yields originfalse; singleton[original T] is the other proved branch.
    // contextT/orgfalse/namespace-roleT then converge for either unified flag.
    match raw.get("internal_team_ids") {
        None | Some(Value::Null) => {}
        Some(Value::Array(ids))
            if ids.is_empty() || ids.len() == 1 && ids[0].as_str() == Some(team) => {}
        _ => return Err(UNAVAILABLE),
    }
    optional_id(raw, "team_id", team)
}
fn absent_or_false(raw: &Value, key: &str) -> Result<()> {
    if raw.get(key).is_some_and(|v| v != &Value::Bool(false)) {
        return Err(UNAVAILABLE);
    }
    Ok(())
}
fn optional_id(raw: &Value, key: &str, expected: &str) -> Result<()> {
    if raw.get(key).is_some_and(|v| v.as_str() != Some(expected)) {
        return Err(UNAVAILABLE);
    }
    Ok(())
}
fn uppercase_id(id: &str, prefix: u8, minimum: usize) -> bool {
    (minimum..=255).contains(&id.len())
        && id.as_bytes()[0] == prefix
        && id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
pub(crate) fn channel_id(id: &str) -> bool {
    uppercase_id(id, b'C', 2)
}
pub(crate) fn timestamp(ts: &str) -> bool {
    ts.len() == 17
        && ts.as_bytes()[10] == b'.'
        && ts
            .bytes()
            .enumerate()
            .all(|(i, b)| i == 10 || b.is_ascii_digit())
}

#[cfg(test)]
mod tests;
