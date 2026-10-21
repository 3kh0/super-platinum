//! Ordinary Canvas root attribution (NKsY / Yc0S / VRXO).
//!
//! This is a borrowed display selection, not read, account, permission, or
//! profile-navigation authority. The caller retains its original reader and
//! current canonical member/name-policy observations. No message fields change.
use crate::{block_actions::name_policy, slack::models::Message};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    Thread,
    Stream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Editor,
    Member,
    Policy(name_policy::Error),
}

/// Distinct source render branches. A known empty name is intentionally empty;
/// neither an ID nor a username is a replacement for that observed value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Name {
    Unknown,
    Nonexistent,
    Known(String),
}

#[derive(Clone, Copy, Debug)]
pub struct Editor<'a> {
    message: &'a Message,
    id: &'a str,
}

fn truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// Select the original most_recent_editor only for a visible thread root.
/// Falsey absence suppresses attribution; a truthy non-string editor is an
/// unsupported shape, not permission to fall back to authors or Message.user.
/// Root validation here is structural; the caller owns the acquired read scope.
pub fn editor(
    message: &Message,
    context: Context,
    hide_attribution: bool,
) -> Result<Option<Editor<'_>>, Error> {
    if context != Context::Thread || hide_attribution || !super::root(message) {
        return Ok(None);
    }
    let comment = &message.extra["document_comment"];
    if comment.get("is_visible") == Some(&Value::Bool(false)) {
        return Ok(None);
    }
    let Some(value) = comment
        .get("most_recent_editor")
        .filter(|value| truth(value))
    else {
        return Ok(None);
    };
    let id = value.as_str().ok_or(Error::Editor)?;
    // Bound malformed identities without normalizing the selected source name.
    if id.len() > 128 || !id.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(Error::Editor);
    }
    Ok(Some(Editor { message, id }))
}

impl<'a> Editor<'a> {
    pub fn id(&self) -> &'a str {
        self.id
    }

    pub fn message(&self) -> &'a Message {
        self.message
    }

    /// The member must be the caller's canonical observation for this exact
    /// editor. A missing lookup is not silently converted into an unknown row.
    /// Unknown has render priority; nonexistent is a placeholder only with an
    /// empty resolved name. Hidden text is an explicit localization observation.
    pub fn name(
        &self,
        member: &Value,
        policy: &name_policy::Snapshot,
        hidden_text: &str,
    ) -> Result<Name, Error> {
        if !member.is_object() || member.get("id").and_then(Value::as_str) != Some(self.id) {
            return Err(Error::Member);
        }
        if member.get("isUnknown").is_some_and(truth) {
            return Ok(Name::Unknown);
        }
        let name = policy
            .member_name(member, hidden_text)
            .map_err(Error::Policy)?;
        if name.is_empty() && member.get("isNonExistent").is_some_and(truth) {
            Ok(Name::Nonexistent)
        } else {
            Ok(Name::Known(name))
        }
    }
}

#[cfg(test)]
mod tests;
