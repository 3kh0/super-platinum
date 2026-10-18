//! Exact plain or rich replies to an existing, writable Canvas comment root.
//! A durable record is inert; only deliberate capture creates a send intent.
use super::*;
use crate::slack::models::File;
use serde::{Deserialize, Serialize};

pub const MAX_TEXT_UTF16: usize = 12_000;
#[derive(Clone)]
pub struct ReplyIntent {
    record: ReplyRecord,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplyRecord {
    version: u8,
    workspace: String,
    user: String,
    file: String,
    root: Value,
    text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    blocks: Option<Vec<Value>>,
    client_msg_id: String,
}
pub struct ReplyAccepted {
    record: ReplyRecord,
    message: Message,
}
pub fn valid_text(text: &str) -> bool {
    text.encode_utf16().count() <= MAX_TEXT_UTF16
        && !text
            .chars()
            .any(|c| c == '\0' || (c.is_control() && c != '\n' && c != '\t' && c != '\r'))
}
fn valid_owner(v: &str) -> bool {
    !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control)
}
fn canonical(file: &File, value: &Message) -> Result<String, Invalid> {
    let id = file.id.as_deref().ok_or(Invalid::Identity)?;
    let ch = channel(id)?;
    if !matches!(file.filetype.as_deref(), Some("quip" | "canvas"))
        || !matches!(
            file.extra.get("access").and_then(Value::as_str),
            Some("comment" | "write" | "owner")
        )
        || ["is_locked", "is_deleted"]
            .iter()
            .any(|k| file.extra.get(*k).is_some_and(|v| v != &Value::Bool(false)))
        || value
            .extra
            .get("is_locked")
            .is_some_and(|v| v != &Value::Bool(false))
        || !root(value)
        || !value.ts.as_deref().is_some_and(timestamp)
        || value.channel.as_ref().is_some_and(|c| c != &ch)
        || value
            .extra
            .get("document_comment")
            .and_then(|d| d.get("is_archived"))
            .is_some_and(|v| v != &Value::Bool(false))
    {
        return Err(Invalid::Identity);
    }
    let thread = value.extra["document_comment"]["thread_id"]
        .as_str()
        .ok_or(Invalid::Identity)?;
    if !valid_owner(thread) {
        return Err(Invalid::Identity);
    }
    Ok(id.into())
}
impl ReplyIntent {
    pub fn available(file: &File, root: &Message) -> bool {
        canonical(file, root).is_ok()
    }
    pub fn capture(
        workspace: &str,
        user: &str,
        file: &File,
        root: &Message,
        text: &str,
        client_msg_id: &str,
    ) -> Result<Self, Invalid> {
        let record = ReplyRecord {
            version: 1,
            workspace: workspace.into(),
            user: user.into(),
            file: canonical(file, root)?,
            root: serde_json::to_value(root).map_err(|_| Invalid::Message)?,
            text: text.into(),
            blocks: None,
            client_msg_id: client_msg_id.into(),
        };
        record.validate()?;
        Ok(Self { record })
    }
    pub fn capture_rich(
        workspace: &str,
        user: &str,
        file: &File,
        root: &Message,
        blocks: Vec<Value>,
        client_msg_id: &str,
    ) -> Result<Self, Invalid> {
        let text = super::content::fallback(&blocks)?;
        let mut intent = Self::capture(workspace, user, file, root, &text, client_msg_id)?;
        intent.record.version = 2;
        intent.record.blocks = Some(blocks);
        intent.record.validate()?;
        Ok(intent)
    }
    pub fn record(&self) -> &ReplyRecord {
        &self.record
    }
    pub fn revalidate(&self, file: &File, root: &Message) -> Result<(), Invalid> {
        if canonical(file, root)? != self.record.file
            || serde_json::to_value(root).map_err(|_| Invalid::Message)? != self.record.root
        {
            return Err(Invalid::Message);
        }
        Ok(())
    }
    pub fn accept(self, raw: &Value) -> Result<ReplyAccepted, Invalid> {
        if raw.get("ok").and_then(Value::as_bool) != Some(true)
            || raw.get("error").is_some_and(|v| !v.is_null())
            || raw.get("channel").and_then(Value::as_str)
                != Some(channel(&self.record.file)?.as_str())
        {
            return Err(Invalid::Envelope);
        }
        let message = message(
            raw.get("message").ok_or(Invalid::Message)?,
            &channel(&self.record.file)?,
        )?;
        if !message.ts.as_deref().is_some_and(timestamp)
            || message.ts.as_deref() != raw.get("ts").and_then(Value::as_str)
            || message.thread_ts.as_deref() != Some(self.record.root_ts())
            || message.client_msg_id.as_deref() != Some(self.record.client_msg_id())
            || match &self.record.blocks {
                None => message.text.as_deref() != Some(self.record.text()),
                Some(blocks) => !super::content::confirmed(blocks, &message.blocks),
            }
        {
            return Err(Invalid::Message);
        }
        Ok(ReplyAccepted {
            record: self.record,
            message,
        })
    }
}
impl ReplyRecord {
    pub fn validate(&self) -> Result<(), Invalid> {
        let content_valid = match (self.version, &self.blocks) {
            (1, None) => true,
            (2, Some(blocks)) => {
                super::content::fallback(blocks).is_ok_and(|text| text == self.text)
            }
            _ => false,
        };
        if !content_valid
            || !valid_owner(&self.workspace)
            || !valid_owner(&self.user)
            || !valid_text(&self.text)
            || self.text.trim().is_empty()
            || uuid::Uuid::parse_str(&self.client_msg_id).is_err()
        {
            return Err(Invalid::Identity);
        }
        let msg = message(&self.root, &channel(&self.file)?)?;
        if !root(&msg)
            || !msg.ts.as_deref().is_some_and(timestamp)
            || serde_json::to_vec(&self.root)
                .map_err(|_| Invalid::Message)?
                .len()
                > 64 * 1024
        {
            return Err(Invalid::Message);
        }
        Ok(())
    }
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
        self.root["ts"].as_str().unwrap()
    }
    pub fn same_root(&self, observed: &Message) -> bool {
        root(observed)
            && observed.ts.as_deref() == Some(self.root_ts())
            && observed
                .channel
                .as_ref()
                .is_none_or(|c| channel(&self.file).is_ok_and(|expected| *c == expected))
            && observed
                .extra
                .get("document_comment")
                .and_then(|d| d.get("thread_id"))
                == self.root.pointer("/document_comment/thread_id")
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn blocks(&self) -> Option<&[Value]> {
        self.blocks.as_deref()
    }
    pub fn client_msg_id(&self) -> &str {
        &self.client_msg_id
    }
}
impl ReplyAccepted {
    pub fn record(&self) -> &ReplyRecord {
        &self.record
    }
    pub fn message(&self) -> &Message {
        &self.message
    }
}
#[cfg(test)]
mod tests;
