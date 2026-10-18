//! Assignee permission shares: retained ShareAssigneeDialog / wfoe / bw5L.
use super::*;
use serde_json::json;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssigneeGrant {
    Read,
    Write,
}
impl AssigneeGrant {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }
}
pub fn list_assignee_open_dm(c: &SlackClient, w: &WorkspaceSession, user: &str) -> PreparedRequest {
    c.rest_form(
        w,
        "conversations.open",
        vec![
            ("users", user.into()),
            ("quiet", "0".into()),
            ("return_im", "1".into()),
            ("_x_reason", "get_channel_id_for_sharing".into()),
        ],
    )
}
pub fn list_assignee_channel(
    c: &SlackClient,
    w: &WorkspaceSession,
    channel: &str,
) -> PreparedRequest {
    c.rest_form(
        w,
        "conversations.info",
        vec![
            ("channel", channel.into()),
            ("_x_reason", "channels-api/fetchConversationInfo".into()),
        ],
    )
}
pub fn list_assignee_prefs(
    c: &SlackClient,
    w: &WorkspaceSession,
    channel: &str,
) -> PreparedRequest {
    c.rest_form(
        w,
        "conversations.listPrefs",
        vec![
            ("channel", channel.into()),
            ("_x_reason", "client_redux_store".into()),
        ],
    )
}
pub fn list_assignee_can_share(
    c: &SlackClient,
    w: &WorkspaceSession,
    list: &str,
    channel: &str,
) -> PreparedRequest {
    c.rest_form(
        w,
        "files.canShare",
        vec![
            ("file_id", list.into()),
            ("channel_id", channel.into()),
            ("_x_reason", "canShare".into()),
        ],
    )
}
/// Empty message ops produce undefined blocks. No message, silent_share, DLP
/// override, broadcast or cascading grant is manufactured for this continuation.
pub fn list_assignee_share(
    c: &SlackClient,
    w: &WorkspaceSession,
    list: &str,
    channel: &str,
    grant: AssigneeGrant,
    context_team: Option<&str>,
) -> PreparedRequest {
    let mut fields = vec![
        ("files", list.into()),
        ("channel", channel.into()),
        ("resharing_aware", "true".into()),
        (
            "permissions",
            json!([{ "grant":grant.as_str(),"file_id":list }]).to_string(),
        ),
        ("skip_dlp_user_warning", "false".into()),
        ("from_share_modal", "true".into()),
    ];
    if let Some(team) = context_team {
        fields.push(("client_context_team_id", team.into()));
    }
    fields.push(("_x_reason", "share_dialog_permission_share_to_user".into()));
    c.rest_form(w, "files.share", fields)
}

pub fn assignee_positive(v: &Value) -> Result<(), Error> {
    if v.get("ok") == Some(&Value::Bool(true)) && v.get("error").is_none_or(Value::is_null) {
        Ok(())
    } else {
        Err(invalid())
    }
}
fn invalid() -> Error {
    Error::Api("Assignee sharing evidence is incomplete or ambiguous.".into())
}
/// A DM is bound to this exact member, never merely an arbitrary channel ID
/// returned from a malformed open/info reply.
pub fn assignee_dm(v: &Value, user: &str) -> Result<Value, Error> {
    assignee_positive(v)?;
    let channel = v
        .get("channel")
        .filter(|v| v.is_object())
        .ok_or_else(invalid)?;
    let id = channel
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    if !identity(id, b"D")
        || channel.get("user").and_then(Value::as_str) != Some(user)
        || channel.get("is_im") != Some(&Value::Bool(true))
        || channel
            .get("is_ext_shared")
            .is_some_and(|v| !v.is_boolean())
    {
        return Err(invalid());
    }
    Ok(channel.clone())
}
fn identity(id: &str, prefix: &[u8]) -> bool {
    id.len() >= 9
        && id.len() <= 128
        && prefix.contains(&id.as_bytes()[0])
        && id
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssigneeShareExperiments {
    pub lists_enabled: bool,
    pub context_team: bool,
}
pub fn assignee_share_experiments(v: &Value) -> Result<AssigneeShareExperiments, Error> {
    assignee_positive(v)?;
    let assignments = v
        .get("assignments")
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    let enabled = |name: &str| -> Result<bool, Error> {
        match assignments.get(name) {
            None => Ok(false),
            Some(v) => v
                .get("group")
                .and_then(Value::as_str)
                .map(|v| v == "on")
                .ok_or_else(invalid),
        }
    };
    let mut user = false;
    let mut team = false;
    for tier in ["playground", "friends_of", "dogfood", "pilot"] {
        user |= enabled(&format!("lists_{tier}"))?;
        team |= enabled(&format!("lists_{tier}_teams"))?;
    }
    Ok(AssigneeShareExperiments {
        lists_enabled: user && team,
        context_team: enabled("workspace_context_for_netskope")?,
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssigneeSharePolicy {
    Allowed,
    CheckUploads,
    Blocked,
}
/// Non-enterprise contexts have no conversationsListPrefs selector in the source;
/// callers pass None only with positive knowledge of that account context.
pub fn assignee_share_policy(
    channel: &Value,
    experiments: AssigneeShareExperiments,
    enterprise: bool,
    prefs: Option<&Value>,
) -> Result<AssigneeSharePolicy, Error> {
    let external = match channel.get("is_ext_shared") {
        None => false,
        Some(v) => v.as_bool().ok_or_else(invalid)?,
    };
    if !external {
        return Ok(AssigneeSharePolicy::Allowed);
    }
    if !experiments.lists_enabled {
        return Ok(AssigneeSharePolicy::Blocked);
    }
    if !enterprise {
        return Ok(AssigneeSharePolicy::Allowed);
    }
    let response = prefs.ok_or_else(invalid)?;
    assignee_positive(response)?;
    let prefs = response
        .get("prefs")
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    let status = match prefs.get("list_share_status") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_str().ok_or_else(invalid)?),
    };
    if status == Some("list_sharing_disabled") {
        return Ok(AssigneeSharePolicy::Blocked);
    }
    let upload = match prefs.get("can_upload_files") {
        None | Some(Value::Null) => true,
        Some(v) => v.as_bool().ok_or_else(invalid)?,
    };
    Ok(if upload {
        AssigneeSharePolicy::Allowed
    } else {
        AssigneeSharePolicy::CheckUploads
    })
}
pub fn assignee_upload_share_allowed(v: &Value) -> Result<bool, Error> {
    assignee_positive(v)?;
    let allowed = v
        .get("can_share")
        .and_then(Value::as_bool)
        .ok_or_else(invalid)?;
    let reason = match v.get("reason") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_str().ok_or_else(invalid)?),
    };
    Ok(allowed || reason != Some("cannot_share_contain_file_upload"))
}
pub fn assignee_context_team(
    channel: &Value,
    experiments: AssigneeShareExperiments,
) -> Result<Option<String>, Error> {
    if !experiments.context_team {
        return Ok(None);
    }
    match channel.get("context_team_id") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_str()
            .filter(|id| identity(id, b"T"))
            .map(|v| Some(v.into()))
            .ok_or_else(invalid),
    }
}

#[cfg(test)]
mod tests;
