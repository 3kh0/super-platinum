//! Fresh ordinary-root permalink data for a named native Copy-only adapter.
//!
//! The caller supplies its original whole responses and same-ID original Team
//! record. This module establishes neither that custody nor a current Source
//! model, clipboard capability, renderer lifetime, or permission to copy.
use serde_json::Value;

const UNAVAILABLE: &str = "Thread link data is unavailable.";
type Result<T> = std::result::Result<T, &'static str>;

/// An isolated literal projection, never a canonical message/channel model.
#[derive(Debug)]
pub struct FreshRootLink {
    team: String,
    channel: String,
    timestamp: String,
    url: String,
}

impl FreshRootLink {
    /// Requires the original exact singleton history and channel-info reads.
    /// `original_team_record` must come from the caller's separately owned raw
    /// Gantry/Boot Team intake; a Workspace/Session URL cannot acquire custody
    /// by satisfying this parser. No predecessor or shared cache is consulted.
    pub fn capture(
        team: &str,
        channel: &str,
        ts: &str,
        history: &Value,
        info: &Value,
        original_team_record: &Value,
    ) -> Result<Self> {
        if !team_id(team) || !channel_id(channel) || !timestamp(ts) {
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
        if !original_team_record.is_object()
            || original_team_record.get("id").and_then(Value::as_str) != Some(team)
        {
            return Err(UNAVAILABLE);
        }
        for key in ["isUnknown", "isNonExistent", "deleted", "is_deleted"] {
            absent_or_false(original_team_record, key)?;
        }
        let base = workspace_base(
            original_team_record
                .get("url")
                .and_then(Value::as_str)
                .ok_or(UNAVAILABLE)?,
        )?;
        Ok(Self {
            team: team.into(),
            channel: channel.into(),
            timestamp: ts.into(),
            url: format!("{base}/archives/{channel}/p{}{}", &ts[..10], &ts[11..]),
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
    pub fn url(&self) -> &str {
        &self.url
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
    // Named fresh whole-history data: vkwm's incoming whitelist excludes the
    // failed-send EKM/sentinel state; API retrieval supplies the non-ephemeral
    // occurrence. Present denying/malformed flags still refuse. This is not a
    // claim that an existing Source cached model had any property overwritten.
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
        "is_tombstone",
        "deleted",
        "is_deleted",
    ] {
        absent_or_false(raw, key)?;
    }
    // Do not accept a previously normalized model as an original raw intake.
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
    // The retained fresh constructor maps absent/null/[] to exact VrU3.Ml=[];
    // its length-zero origin branch is independent of global flags. Singleton
    // [original T] is the separately qualified same-team alternative. Neither
    // rule is an absent-property fold over an existing channel namespace.
    match raw.get("internal_team_ids") {
        None | Some(Value::Null) => {}
        Some(Value::Array(ids))
            if ids.is_empty() || (ids.len() == 1 && ids[0].as_str() == Some(team)) => {}
        _ => return Err(UNAVAILABLE),
    }
    optional_id(raw, "team_id", team)?;
    Ok(())
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
fn team_id(id: &str) -> bool {
    uppercase_id(id, b'T', 9)
}
fn channel_id(id: &str) -> bool {
    uppercase_id(id, b'C', 2)
}
fn timestamp(ts: &str) -> bool {
    ts.len() == 17
        && ts.as_bytes()[10] == b'.'
        && ts
            .bytes()
            .enumerate()
            .all(|(i, b)| i == 10 || b.is_ascii_digit())
}

// An intentionally smaller native profile of the actual Team.url truthy
// branch (https://new URL(team.url).host). No WHATWG/Rust URL equivalence or
// fallback hostname is assumed: these accepted bytes need no normalization.
fn workspace_base(input: &str) -> Result<&str> {
    let base = input.strip_suffix('/').unwrap_or(input);
    let host = base.strip_prefix("https://").ok_or(UNAVAILABLE)?;
    let label = host
        .strip_suffix(".enterprise.slack.com")
        .or_else(|| host.strip_suffix(".slack.com"))
        .ok_or(UNAVAILABLE)?;
    if !(1..=63).contains(&label.len())
        || matches!(label, "app" | "my")
        || !label.as_bytes()[0].is_ascii_alphanumeric()
        || !label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
        || !label
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(UNAVAILABLE);
    }
    Ok(base)
}

#[cfg(test)]
mod tests;
