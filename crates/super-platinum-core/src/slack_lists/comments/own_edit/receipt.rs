use super::*;

#[derive(Clone)]
pub struct OwnEditAccepted {
    record: OwnEditRecord,
    message: Message,
    raw: Value,
}
impl OwnEditIntent {
    pub fn accept(self, raw: Value) -> Result<OwnEditAccepted, ListError> {
        self.record.validate_ack(&raw)?;
        let r = &self.record;
        // Unlike a whole conversations.replies row, chat.update may omit the
        // nested identity; its required outer channel/ts supplies request-bound
        // identity. Explicit conflicting nested fields always refuse.
        let mut projected = r.seed.original_raw().clone();
        for key in ["text", "blocks", "edited"] {
            if let Some(value) = raw["message"].get(key) {
                projected[key] = value.clone();
            }
        }
        let message = parse_message(&projected, r.target().channel_id())?;
        Ok(OwnEditAccepted {
            record: self.record,
            message,
            raw,
        })
    }
}
impl OwnEditAccepted {
    pub fn record(&self) -> &OwnEditRecord {
        &self.record
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn message(&self) -> &Message {
        &self.message
    }
    /// Apply selected confirmed body fields to an unchanged original body.
    /// Reactions and all unrelated current metadata are retained, never copied
    /// back from the original HTTP receipt or a sparse ACK.
    pub fn apply(&self, current: &Message) -> Option<Message> {
        if !self.record.seed.current(current) {
            return None;
        }
        let mut next = current.clone();
        next.blocks = self.message.blocks.clone();
        if self.raw["message"].get("text").is_some() {
            next.text = self.message.text.clone();
        }
        if self.raw["message"].get("edited").is_some() {
            next.edited = self.message.edited.clone();
        }
        Some(next)
    }
}
impl OwnEditRecord {
    /// Shared inert receipt validation for account-local recovery. This cannot
    /// create a sendable intent or grant current publication authority.
    pub(crate) fn validate_ack(&self, raw: &Value) -> Result<(), ListError> {
        self.validate()?;
        if raw["ok"] != true
            || raw.get("error").is_some_and(|v| !v.is_null())
            || raw["channel"].as_str() != Some(self.target().channel_id())
            || raw["ts"].as_str() != Some(self.ts())
            || !confirmed(self, &raw["message"])
        {
            return Err(ListError(
                "The edit acknowledgement did not match the requested comment.",
            ));
        }
        Ok(())
    }
    /// Positive later HTTP content evidence, not an ACK and not absence proof.
    /// No UUID exists on chat.update; equal content cannot identify the writer.
    pub fn observed(&self, page: &Page) -> Result<Option<Message>, ListError> {
        self.validate()?;
        let target = page.target();
        if target.list_id() != self.target().list_id()
            || target.record_id() != self.target().record_id()
            || target.channel_id() != self.target().channel_id()
            || target.root_ts() != self.target().root_ts()
            || ["date_created", "created_by"]
                .iter()
                .any(|key| target.record_raw().get(*key) != self.target().record_raw().get(*key))
        {
            return Err(ListError("The edit readback belongs to another item."));
        }
        if page.raw()["deleted"]
            .as_array()
            .is_some_and(|v| v.iter().any(|v| v == self.ts()))
        {
            return Ok(None);
        }
        let Some(raw) = page.raw()["messages"]
            .as_array()
            .and_then(|v| v.iter().find(|v| v["ts"].as_str() == Some(self.ts())))
        else {
            return Ok(None);
        };
        // A later HTTP row must supply its own author/thread, unlike the ACK.
        if raw["user"].as_str() != Some(self.user())
            || raw["thread_ts"].as_str() != target.root_ts()
            || !confirmed(self, raw)
        {
            return Ok(None);
        }
        let message = parse_message(raw, target.channel_id())?;
        if seed::ordinary(&message, target, self.user()).is_err() {
            return Ok(None);
        }
        Ok(Some(message))
    }
}
fn confirmed(record: &OwnEditRecord, raw: &Value) -> bool {
    if !raw.is_object() {
        return false;
    }
    for (key, expected) in [
        ("ts", record.ts()),
        ("channel", record.target().channel_id()),
        ("user", record.user()),
        ("thread_ts", record.target().root_ts().unwrap()),
    ] {
        if raw.get(key).is_some_and(|v| v.as_str() != Some(expected)) {
            return false;
        }
    }
    if raw.get("text").is_some_and(|v| !v.is_string())
        || raw
            .get("edited")
            .is_some_and(|v| !v.is_null() && !v.is_object())
        || [
            "isUnknown",
            "isEKMError",
            "is_ephemeral",
            "no_display",
            "is_deleted",
            "isNonExistent",
            "is_tombstoned",
            "_hidden_stub",
        ]
        .iter()
        .any(|k| seed::truthy(raw.get(*k)))
        || raw.get("type").is_some_and(|v| v != "message")
        || ["files", "attachments", "pendingFiles"].iter().any(|key| {
            raw.get(*key)
                .is_some_and(|v| v.as_array().is_none_or(|v| !v.is_empty()))
        })
        || seed::truthy(raw.get("markdown_text"))
        || seed::truthy(raw.get("list_edit"))
        || raw.get("subtype").is_some_and(|v| {
            !v.is_null() && v.as_str() != record.seed.original_raw()["subtype"].as_str()
        })
    {
        return false;
    }
    let Some(blocks) = raw["blocks"].as_array() else {
        return false;
    };
    let preserved = record.seed.preserved();
    let Some(rich_len) = blocks.len().checked_sub(preserved.len()) else {
        return false;
    };
    let expected_rich = record.blocks.len() - preserved.len();
    blocks[rich_len..] == *preserved
        && content::confirmed(&record.blocks[..expected_rich], &blocks[..rich_len])
}
impl Page {
    /// Confirmed edit changes the typed body only; the immutable HTTP receipt,
    /// target and pagination are unchanged. Host overlays guard newer deletes.
    pub fn apply_own_edit(&mut self, accepted: &OwnEditAccepted) -> bool {
        if self.target != *accepted.record().target()
            || self.raw["deleted"]
                .as_array()
                .is_some_and(|v| v.iter().any(|v| v == accepted.record().ts()))
        {
            return false;
        }
        let Some(message) = self
            .messages
            .iter_mut()
            .find(|m| m.ts.as_deref() == Some(accepted.record().ts()))
        else {
            return false;
        };
        let Some(next) = accepted.apply(message) else {
            return false;
        };
        *message = next;
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnEditPhase {
    Ready,
    Dispatched,
    NotSent,
    Rejected,
    Uncertain,
    Accepted,
}
pub struct OwnEditAttempt {
    intent: Option<OwnEditIntent>,
    record: OwnEditRecord,
    phase: OwnEditPhase,
}
impl OwnEditAttempt {
    pub fn new(intent: OwnEditIntent) -> Self {
        Self {
            record: intent.record.clone(),
            intent: Some(intent),
            phase: OwnEditPhase::Ready,
        }
    }
    /// Spend once before entering async work. Restored records cannot be sent.
    pub fn take(&mut self) -> Option<OwnEditIntent> {
        if self.phase != OwnEditPhase::Ready {
            return None;
        }
        self.phase = OwnEditPhase::Dispatched;
        self.intent.take()
    }
    pub fn settle(&mut self, phase: OwnEditPhase) -> bool {
        if self.phase != OwnEditPhase::Dispatched
            || !matches!(
                phase,
                OwnEditPhase::NotSent | OwnEditPhase::Rejected | OwnEditPhase::Uncertain
            )
        {
            return false;
        }
        self.phase = phase;
        true
    }
    pub fn accepted(&mut self, accepted: &OwnEditAccepted) -> bool {
        if self.phase != OwnEditPhase::Dispatched || self.record != accepted.record {
            return false;
        }
        self.phase = OwnEditPhase::Accepted;
        true
    }
    pub fn phase(&self) -> OwnEditPhase {
        self.phase
    }
    pub fn record(&self) -> &OwnEditRecord {
        &self.record
    }
}
