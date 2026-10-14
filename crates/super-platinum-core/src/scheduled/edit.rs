//! Attachment-only edits of an existing server draft. No transport or file deletion.
//!
//! The caller owns account/transport admission and must keep this model alive after
//! dispatch, including when its dialog closes. A lost/partial acknowledgement does
//! not authorize another write. Preflight is a stale-data check, not a server CAS.
use super::ScheduledMessage;
use crate::slack::api::ScheduleMessage;
use serde_json::Value;
use std::{collections::HashSet, sync::Arc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditPhase {
    Editing,
    Prepared,
    InFlight,
    Conflict,
    Uncertain,
    Confirmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditError {
    InvalidBaseline,
    InvalidPhase,
    UnknownFile,
    Unchanged,
    EmptyOrUnprovenContent,
    ForeignAttempt,
    PreflightMismatch,
}

/// Immutable identity and complete submission. No public constructor or mutable fields.
#[derive(Debug, Clone)]
pub struct FrozenAttachmentEdit(Arc<Submission>);

#[derive(Debug)]
struct Submission {
    draft_id: String,
    payload: ScheduleMessage,
}

impl FrozenAttachmentEdit {
    pub fn draft_id(&self) -> &str {
        &self.0.draft_id
    }

    /// A clone for the request builder; mutations to it cannot change this receipt.
    pub fn request(&self) -> ScheduleMessage {
        self.0.payload.clone()
    }
}

#[derive(Debug)]
pub struct AttachmentEdit {
    original: ScheduledMessage,
    retained: Vec<String>,
    phase: EditPhase,
    attempt: Option<FrozenAttachmentEdit>,
}

impl AttachmentEdit {
    pub fn new(original: ScheduledMessage) -> Result<Self, EditError> {
        let mut ids = HashSet::new();
        if !original.active()
            || original.id.is_empty()
            || original.client_msg_id.is_empty()
            || original.post_at < 0
            || original.destinations.is_empty()
            || original.destinations.iter().any(|destination| {
                !destination["channel_id"]
                    .as_str()
                    .is_some_and(|id| !id.is_empty())
            })
            || original
                .file_ids
                .iter()
                .any(|id| id.is_empty() || !ids.insert(id.clone()))
        {
            return Err(EditError::InvalidBaseline);
        }
        Ok(Self {
            retained: original.file_ids.clone(),
            original,
            phase: EditPhase::Editing,
            attempt: None,
        })
    }

    pub fn phase(&self) -> EditPhase {
        self.phase
    }

    pub fn file_ids(&self) -> &[String] {
        &self.retained
    }

    /// Staging only. Dropping an undispatched model is Cancel and changes nothing.
    pub fn remove(&mut self, id: &str) -> Result<(), EditError> {
        self.require_phase(EditPhase::Editing)?;
        let position = self
            .retained
            .iter()
            .position(|candidate| candidate == id)
            .ok_or(EditError::UnknownFile)?;
        self.retained.remove(position);
        Ok(())
    }

    /// Undo a removal using the original order, never insertion order.
    pub fn restore(&mut self, id: &str) -> Result<(), EditError> {
        self.require_phase(EditPhase::Editing)?;
        if !self
            .original
            .file_ids
            .iter()
            .any(|candidate| candidate == id)
        {
            return Err(EditError::UnknownFile);
        }
        self.retained = self
            .original
            .file_ids
            .iter()
            .filter(|candidate| candidate.as_str() == id || self.retained.contains(candidate))
            .cloned()
            .collect();
        Ok(())
    }

    pub fn freeze(&mut self) -> Result<FrozenAttachmentEdit, EditError> {
        self.require_phase(EditPhase::Editing)?;
        if self.retained == self.original.file_ids {
            return Err(EditError::Unchanged);
        }
        if self.retained.is_empty() && !self.original.blocks.iter().any(known_content) {
            return Err(EditError::EmptyOrUnprovenContent);
        }
        let mut payload = ScheduleMessage::from_draft(&self.original);
        payload.file_ids.clone_from(&self.retained);
        let attempt = FrozenAttachmentEdit(Arc::new(Submission {
            draft_id: self.original.id.clone(),
            payload,
        }));
        self.attempt = Some(attempt.clone());
        self.phase = EditPhase::Prepared;
        Ok(attempt)
    }

    /// Only call before any mutation dispatch. Failed read-only preflight can be retried.
    pub fn abandon_preflight(&mut self, attempt: &FrozenAttachmentEdit) -> Result<(), EditError> {
        self.require_attempt(attempt, EditPhase::Prepared)?;
        self.attempt = None;
        self.phase = EditPhase::Editing;
        Ok(())
    }

    /// Require a complete positive raw response envelope, including full payload
    /// even at equal timestamps. Bare draft objects cannot establish read success.
    /// The caller checks captured account/transport ownership again before dispatch.
    pub fn admit_dispatch(
        &mut self,
        attempt: &FrozenAttachmentEdit,
        response: &Value,
    ) -> Result<(), EditError> {
        self.require_attempt(attempt, EditPhase::Prepared)?;
        let matches = positive_draft(response).is_some_and(|current| {
            matches_payload(
                current,
                &self.original.id,
                &ScheduleMessage::from_draft(&self.original),
            ) && current.get("last_updated_ts").and_then(Value::as_str)
                == Some(self.original.last_updated_ts.as_str())
        });
        if !matches {
            self.phase = EditPhase::Conflict;
            return Err(EditError::PreflightMismatch);
        }
        self.phase = EditPhase::InFlight;
        Ok(())
    }

    /// Complete raw mutation response. Partial/error/mismatching responses stay uncertain.
    /// Even an error response is not automatically classified as safe to replay here.
    pub fn acknowledge(
        &mut self,
        attempt: &FrozenAttachmentEdit,
        response: &Value,
    ) -> Result<bool, EditError> {
        self.require_attempt(attempt, EditPhase::InFlight)?;
        let confirmed = positive_draft(response)
            .is_some_and(|draft| matches_payload(draft, attempt.draft_id(), &attempt.0.payload));
        self.phase = if confirmed {
            EditPhase::Confirmed
        } else {
            EditPhase::Uncertain
        };
        Ok(confirmed)
    }

    pub fn mark_uncertain(&mut self, attempt: &FrozenAttachmentEdit) -> Result<(), EditError> {
        self.require_attempt(attempt, EditPhase::InFlight)?;
        self.phase = EditPhase::Uncertain;
        Ok(())
    }

    /// Positive read-only response envelope only. Missing records, old content,
    /// deletion and list absence never clear the no-replay latch.
    pub fn review(
        &mut self,
        attempt: &FrozenAttachmentEdit,
        response: Option<&Value>,
    ) -> Result<bool, EditError> {
        self.require_attempt(attempt, EditPhase::Uncertain)?;
        let confirmed = response
            .and_then(positive_draft)
            .is_some_and(|draft| matches_payload(draft, attempt.draft_id(), &attempt.0.payload));
        if confirmed {
            self.phase = EditPhase::Confirmed;
        }
        Ok(confirmed)
    }

    fn require_phase(&self, expected: EditPhase) -> Result<(), EditError> {
        (self.phase == expected)
            .then_some(())
            .ok_or(EditError::InvalidPhase)
    }

    fn require_attempt(
        &self,
        attempt: &FrozenAttachmentEdit,
        phase: EditPhase,
    ) -> Result<(), EditError> {
        if !self
            .attempt
            .as_ref()
            .is_some_and(|owned| Arc::ptr_eq(&owned.0, &attempt.0))
        {
            return Err(EditError::ForeignAttempt);
        }
        self.require_phase(phase)
    }
}

fn positive_draft(response: &Value) -> Option<&Value> {
    if response.get("ok") != Some(&Value::Bool(true))
        || response.get("error").is_some_and(|error| !error.is_null())
    {
        return None;
    }
    response.get("draft").filter(|draft| draft.is_object())
}

fn matches_payload(draft: &Value, id: &str, payload: &ScheduleMessage) -> bool {
    let date = draft.get("date_scheduled").and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_str().and_then(|value| value.parse::<i64>().ok()))
    });
    draft.get("id").and_then(Value::as_str) == Some(id)
        && draft.get("client_msg_id").and_then(Value::as_str)
            == Some(payload.client_msg_id.as_str())
        && draft.get("blocks").and_then(Value::as_array) == Some(&payload.blocks)
        && draft.get("destinations").and_then(Value::as_array) == Some(&payload.destinations)
        && draft.get("file_ids") == Some(&serde_json::json!(payload.file_ids))
        && date == Some(payload.post_at)
        && draft.get("is_deleted") == Some(&Value::Bool(false))
        && draft.get("is_sent") == Some(&Value::Bool(false))
}

/// Conservative content admission for removing the last attachment. Unknown rich
/// blocks are preserved byte-for-JSON-value, but cannot alone prove sendable content.
fn known_content(value: &Value) -> bool {
    match value.get("type").and_then(Value::as_str) {
        Some("text" | "plain_text" | "mrkdwn") => value["text"]
            .as_str()
            .is_some_and(|text| !text.trim().is_empty()),
        Some("emoji") => nonempty(value, "name"),
        Some("user") => nonempty(value, "user_id"),
        Some("channel") => nonempty(value, "channel_id"),
        Some("usergroup") => nonempty(value, "usergroup_id"),
        Some("link") => nonempty(value, "url"),
        Some(
            "rich_text"
            | "rich_text_section"
            | "rich_text_list"
            | "rich_text_quote"
            | "rich_text_preformatted",
        ) => value["elements"]
            .as_array()
            .is_some_and(|elements| elements.iter().any(known_content)),
        Some("section" | "header") => value.get("text").is_some_and(known_content),
        _ => false,
    }
}

fn nonempty(value: &Value, key: &str) -> bool {
    value[key]
        .as_str()
        .is_some_and(|text| !text.trim().is_empty())
}

#[cfg(test)]
mod tests;
