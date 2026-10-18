//! Exact, explicit resolve/reopen intent. A restored record is inert evidence.
//! BRw9/Bo5c use file-channel identity, write access and an existing comment root.
use super::*;
use crate::slack::models::File;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct ArchiveIntent {
    file: String,
    before: Value,
    root_ts: String,
    thread: String,
    desired: bool,
}
pub struct ArchiveAccepted {
    intent: ArchiveIntent,
    root: Message,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveRecord {
    version: u8,
    workspace: String,
    user: String,
    file: String,
    before: Value,
    desired: bool,
}
fn raw(message: &Message) -> Result<Value, Invalid> {
    let raw = serde_json::to_value(message).map_err(|_| Invalid::Message)?;
    if serde_json::to_vec(&raw)
        .map_err(|_| Invalid::Message)?
        .len()
        > 64 * 1024
    {
        return Err(Invalid::Limit);
    }
    Ok(raw)
}
fn archived(message: &Message) -> Result<bool, Invalid> {
    match message
        .extra
        .get("document_comment")
        .and_then(|v| v.get("is_archived"))
    {
        None => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        _ => Err(Invalid::Message),
    }
}
fn identity(file: &str, value: &Value) -> Result<Message, Invalid> {
    let result = message(value, &channel(file)?)?;
    if !root(&result) || result.reply_count.unwrap_or(0) == 0 {
        return Err(Invalid::Message);
    }
    let thread = result.extra["document_comment"]["thread_id"]
        .as_str()
        .unwrap();
    if thread.chars().any(char::is_control) {
        return Err(Invalid::Identity);
    }
    archived(&result)?;
    Ok(result)
}
fn permission(file: &File, expected: &str) -> Result<(), Invalid> {
    if file.id.as_deref() != Some(expected)
        || !matches!(file.filetype.as_deref(), Some("quip" | "canvas"))
        || !matches!(
            file.extra.get("access").and_then(Value::as_str),
            Some("write" | "owner")
        )
        || file
            .extra
            .get("is_locked")
            .is_some_and(|value| value != &Value::Bool(false))
        || file
            .extra
            .get("is_deleted")
            .is_some_and(|value| value != &Value::Bool(false))
    {
        return Err(Invalid::Identity);
    }
    Ok(())
}
impl ArchiveIntent {
    /// Cheap presentation eligibility only. Dispatch still requires capture and
    /// exact fresh-root revalidation; this does not mint a request or receipt.
    pub fn action(file: &File, value: &Message) -> Option<bool> {
        let id = file.id.as_deref()?;
        permission(file, id).ok()?;
        let expected = channel(id).ok()?;
        if !root(value)
            || value.reply_count.unwrap_or(0) == 0
            || !value.ts.as_deref().is_some_and(timestamp)
            || value
                .channel
                .as_ref()
                .is_some_and(|channel| channel != &expected)
        {
            return None;
        }
        let thread = value
            .extra
            .get("document_comment")?
            .get("thread_id")?
            .as_str()?;
        if thread.len() > 256 || thread.chars().any(char::is_control) {
            return None;
        }
        archived(value).ok().map(|current| !current)
    }
    pub fn capture(file: &File, root: &Message) -> Result<Self, Invalid> {
        let id = file.id.as_deref().ok_or(Invalid::Identity)?;
        permission(file, id)?;
        let before = raw(root)?;
        let root = identity(id, &before)?;
        Ok(Self {
            file: id.into(),
            before,
            root_ts: root.ts.clone().unwrap(),
            thread: root.extra["document_comment"]["thread_id"]
                .as_str()
                .unwrap()
                .into(),
            desired: !archived(&root)?,
        })
    }
    pub fn file(&self) -> &str {
        &self.file
    }
    pub fn root_ts(&self) -> &str {
        &self.root_ts
    }
    pub fn desired(&self) -> bool {
        self.desired
    }
    pub fn method(&self) -> &'static str {
        if self.desired {
            "quip.thread.archive"
        } else {
            "quip.thread.unarchive"
        }
    }
    pub fn revalidate(&self, file: &File, root: &Message) -> Result<(), Invalid> {
        permission(file, &self.file)?;
        if raw(root)? != self.before {
            return Err(Invalid::Message);
        }
        identity(&self.file, &self.before)?;
        Ok(())
    }
    /// The wire response, not visual equality or a future read, creates this proof.
    pub fn accept(self, response: &Value) -> Result<ArchiveAccepted, Invalid> {
        if response.get("ok").and_then(Value::as_bool) != Some(true)
            || response.get("error").is_some_and(|value| !value.is_null())
        {
            return Err(Invalid::Envelope);
        }
        let root = identity(&self.file, response.get("message").ok_or(Invalid::Message)?)?;
        if root.ts.as_deref() != Some(&self.root_ts)
            || root.extra["document_comment"]["thread_id"].as_str() != Some(&self.thread)
            || archived(&root)? != self.desired
        {
            return Err(Invalid::Message);
        }
        Ok(ArchiveAccepted { intent: self, root })
    }
    pub fn record(&self, workspace: &str, user: &str) -> Result<ArchiveRecord, Invalid> {
        let record = ArchiveRecord {
            version: 1,
            workspace: workspace.into(),
            user: user.into(),
            file: self.file.clone(),
            before: self.before.clone(),
            desired: self.desired,
        };
        record.validate()?;
        Ok(record)
    }
}
impl ArchiveAccepted {
    pub fn root(&self) -> &Message {
        &self.root
    }
    pub fn readback(&self, file: &File, root: &Message) -> Result<(), Invalid> {
        permission(file, self.intent.file())?;
        let root = identity(self.intent.file(), &raw(root)?)?;
        if root.ts != self.root.ts
            || root.extra["document_comment"]["thread_id"]
                != self.root.extra["document_comment"]["thread_id"]
            || archived(&root)? != self.intent.desired
        {
            return Err(Invalid::Message);
        }
        Ok(())
    }
}
impl ArchiveRecord {
    pub fn workspace(&self) -> &str {
        &self.workspace
    }
    pub fn user(&self) -> &str {
        &self.user
    }
    pub fn file(&self) -> &str {
        &self.file
    }
    pub fn root_ts(&self) -> &str {
        self.before["ts"].as_str().unwrap_or("")
    }
    pub fn desired(&self) -> bool {
        self.desired
    }
    /// Human local review may inspect a changed outcome, but only this exact
    /// original thread. It creates no acknowledgement or replay authority.
    pub fn review_current(&self, file: &File, root: &Message) -> Result<Value, Invalid> {
        self.validate()?;
        permission(file, &self.file)?;
        let current = raw(root)?;
        let parsed = identity(&self.file, &current)?;
        if parsed.ts.as_deref() != Some(self.root_ts())
            || current["document_comment"]["thread_id"]
                != self.before["document_comment"]["thread_id"]
        {
            return Err(Invalid::Identity);
        }
        Ok(current)
    }
    pub fn validate(&self) -> Result<(), Invalid> {
        if self.version != 1
            || [&self.workspace, &self.user]
                .iter()
                .any(|id| id.is_empty() || id.len() > 256 || id.chars().any(char::is_control))
        {
            return Err(Invalid::Identity);
        }
        let root = identity(&self.file, &self.before)?;
        raw(&root)?;
        if archived(&root)? == self.desired {
            return Err(Invalid::Message);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
