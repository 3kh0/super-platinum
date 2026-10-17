use super::{Fault, Result};
use serde::{Deserialize, Serialize};

pub const MAX_TITLE_BYTES: usize = 16 * 1024;
pub const MAX_MARKDOWN_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_CONTENT_BYTES: usize = MAX_TITLE_BYTES + MAX_MARKDOWN_BYTES;
pub const MAX_CONFLICTS: usize = 16;
pub const MAX_CONFLICT_OPERATIONS: usize = 128;
pub const MAX_CONFLICT_BYTES: usize = MAX_CONFLICTS * MAX_CONTENT_BYTES;
pub const MAX_SCOPES: usize = 32;
pub const MAX_ATTEMPTS: usize = 128;
pub(crate) const MAX_ROW_BYTES: usize = MAX_CONTENT_BYTES * 6 + 64 * 1024;

pub(crate) fn uuid(value: &str) -> Result<()> {
    if uuid::Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value) {
        Ok(())
    } else {
        Err(Fault::Invalid)
    }
}
pub(crate) fn next(value: u64) -> Result<u64> {
    value
        .checked_add(1)
        .filter(|v| *v <= i64::MAX as u64)
        .ok_or(Fault::Invalid)
}
pub(crate) fn revision(value: u64) -> Result<()> {
    if value > 0 && value <= i64::MAX as u64 {
        Ok(())
    } else {
        Err(Fault::Invalid)
    }
}
pub(crate) fn clock(value: u64) -> Result<()> {
    if value <= i64::MAX as u64 {
        Ok(())
    } else {
        Err(Fault::Invalid)
    }
}
fn id(value: &str, prefix: &[u8]) -> bool {
    value.len() > 1
        && value.len() <= 256
        && prefix.contains(&value.as_bytes()[0])
        && value.bytes().all(|b| b.is_ascii_alphanumeric())
}

// Intentionally no Debug implementation for content-bearing records.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Owner {
    pub(crate) account: String,
    pub(crate) team: String,
    pub(crate) user: String,
}
impl Owner {
    pub fn new(account: &str, team: &str, user: &str) -> Result<Self> {
        let owner = Self {
            account: account.into(),
            team: team.into(),
            user: user.into(),
        };
        owner.validate()?;
        Ok(owner)
    }
    pub fn account(&self) -> &str {
        &self.account
    }
    pub fn team(&self) -> &str {
        &self.team
    }
    pub fn user(&self) -> &str {
        &self.user
    }
    pub(crate) fn key(&self) -> String {
        format!("{}/{}", self.team, self.user)
    }
    pub(crate) fn validate(&self) -> Result<()> {
        uuid(&self.account)?;
        if id(&self.team, b"TE") && id(&self.user, b"UW") {
            Ok(())
        } else {
            Err(Fault::Invalid)
        }
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Content {
    title: String,
    markdown: String,
}
impl Content {
    pub fn new(title: String, markdown: String) -> Result<Self> {
        let value = Self { title, markdown };
        value.validate()?;
        Ok(value)
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn markdown(&self) -> &str {
        &self.markdown
    }
    pub(crate) fn empty() -> Self {
        Self {
            title: String::new(),
            markdown: String::new(),
        }
    }
    pub(crate) fn bytes(&self) -> usize {
        self.title.len() + self.markdown.len()
    }
    pub(crate) fn validate(&self) -> Result<()> {
        if self.title.len() <= MAX_TITLE_BYTES && self.markdown.len() <= MAX_MARKDOWN_BYTES {
            Ok(())
        } else {
            Err(Fault::Capacity)
        }
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub(crate) id: String,
    pub(crate) owner: Owner,
    pub(crate) revision: u64,
    pub(crate) writer: String,
    pub(crate) content: Content,
    pub(crate) attempt: Option<String>,
    pub(crate) archived: bool,
    pub(crate) decision_revision: u64,
}
impl Draft {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn owner(&self) -> &Owner {
        &self.owner
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn writer(&self) -> &str {
        &self.writer
    }
    pub fn content(&self) -> &Content {
        &self.content
    }
    pub fn attempt_id(&self) -> Option<&str> {
        self.attempt.as_deref()
    }
    pub fn archived(&self) -> bool {
        self.archived
    }
    pub fn decision_revision(&self) -> u64 {
        self.decision_revision
    }
    pub(crate) fn fresh(owner: Owner, writer: &str, content: Content) -> Result<Self> {
        uuid(writer)?;
        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            owner,
            revision: 1,
            writer: writer.into(),
            content,
            attempt: None,
            archived: false,
            decision_revision: 1,
        })
    }
    pub(crate) fn validate(&self) -> Result<()> {
        uuid(&self.id)?;
        self.owner.validate()?;
        revision(self.revision)?;
        uuid(&self.writer)?;
        self.content.validate()?;
        revision(self.decision_revision)?;
        if let Some(id) = &self.attempt {
            uuid(id)?;
        }
        if self.archived && self.decision_revision < 2 {
            return Err(Fault::Invalid);
        }
        Ok(())
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Delivery {
    Reserved,
    Started,
    Uncertain,
    CanceledBeforeAdmission,
    RateLimited { retry_at: u64 },
    Created { file_id: String },
}
impl Delivery {
    pub(crate) fn validate(&self) -> Result<()> {
        match self {
            Self::RateLimited { retry_at } => clock(*retry_at),
            Self::Created { file_id } if !id(file_id, b"F") => Err(Fault::Invalid),
            _ => Ok(()),
        }
    }
    pub(crate) fn editable(&self) -> bool {
        matches!(
            self,
            Self::CanceledBeforeAdmission | Self::RateLimited { .. }
        )
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    pub(crate) id: String,
    pub(crate) owner: Owner,
    pub(crate) draft_id: String,
    pub(crate) draft_revision: u64,
    pub(crate) frozen: Content,
    pub(crate) delivery: Delivery,
    pub(crate) result_revision: u64,
}
impl Attempt {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn owner(&self) -> &Owner {
        &self.owner
    }
    pub fn draft_id(&self) -> &str {
        &self.draft_id
    }
    pub fn draft_revision(&self) -> u64 {
        self.draft_revision
    }
    pub fn content(&self) -> &Content {
        &self.frozen
    }
    pub fn delivery(&self) -> &Delivery {
        &self.delivery
    }
    pub fn result_revision(&self) -> u64 {
        self.result_revision
    }
    pub(crate) fn validate(&self) -> Result<()> {
        uuid(&self.id)?;
        self.owner.validate()?;
        uuid(&self.draft_id)?;
        revision(self.draft_revision)?;
        revision(self.result_revision)?;
        self.frozen.validate()?;
        self.delivery.validate()?;
        if self.frozen.title.trim().is_empty()
            || (matches!(self.delivery, Delivery::Reserved) != (self.result_revision == 1))
        {
            return Err(Fault::Invalid);
        }
        match self.delivery {
            Delivery::Started | Delivery::CanceledBeforeAdmission if self.result_revision != 2 => {
                return Err(Fault::Invalid);
            }
            Delivery::Uncertain | Delivery::RateLimited { .. } | Delivery::Created { .. }
                if self.result_revision < 3 =>
            {
                return Err(Fault::Invalid);
            }
            _ => {}
        }
        Ok(())
    }
}
/// Only Store::admit constructs this; neither Clone nor Deserialize is implemented.
#[must_use]
pub struct DispatchPermit(pub(crate) Attempt);
impl DispatchPermit {
    /// Consume immediately before existing Transport handoff, with no intervening await.
    /// The returned address carries no reusable dispatch authority.
    pub fn consume(self) -> Attempt {
        self.0
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct Reservation {
    pub(crate) draft: Draft,
    pub(crate) attempt: Attempt,
}
impl Reservation {
    pub fn draft(&self) -> &Draft {
        &self.draft
    }
    pub fn attempt(&self) -> &Attempt {
        &self.attempt
    }
}
pub enum Outcome {
    Uncertain,
    RateLimited { retry_at: u64 },
    Created { file_id: String },
}
impl Outcome {
    /// No API error-name allowlist. Missing Retry-After supplies no invented interval.
    pub fn from_response(
        response: &std::result::Result<serde_json::Value, crate::slack::Error>,
        now: u64,
    ) -> Result<Self> {
        clock(now)?;
        Ok(match response {
            Ok(value) => match crate::slack::api::created_canvas_id(value) {
                Ok(file_id)
                    if id(&file_id, b"F")
                        && value.get("ok").and_then(serde_json::Value::as_bool) != Some(false)
                        && value.get("error").is_none_or(serde_json::Value::is_null) =>
                {
                    Self::Created { file_id }
                }
                _ => Self::Uncertain,
            },
            Err(crate::slack::Error::RateLimited { retry_after_secs }) => Self::RateLimited {
                retry_at: now
                    .checked_add(retry_after_secs.unwrap_or(0))
                    .filter(|v| *v <= i64::MAX as u64)
                    .ok_or(Fault::Invalid)?,
            },
            Err(_) => Self::Uncertain,
        })
    }
    pub(crate) fn delivery(self) -> Result<Delivery> {
        let d = match self {
            Self::Uncertain => Delivery::Uncertain,
            Self::RateLimited { retry_at } => Delivery::RateLimited { retry_at },
            Self::Created { file_id } => Delivery::Created { file_id },
        };
        d.validate()?;
        Ok(d)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conflict {
    pub(crate) id: String,
    pub(crate) owner: Owner,
    pub(crate) draft_id: String,
    pub(crate) base_revision: u64,
    pub(crate) writer: String,
    pub(crate) content: Content,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Disposition {
    pub(crate) id: String,
    pub(crate) owner: Owner,
    pub(crate) draft_id: String,
    pub(crate) base_revision: u64,
    pub(crate) writer: String,
}
impl Disposition {
    pub(crate) fn from_conflict(value: &Conflict) -> Self {
        Self {
            id: value.id.clone(),
            owner: value.owner.clone(),
            draft_id: value.draft_id.clone(),
            base_revision: value.base_revision,
            writer: value.writer.clone(),
        }
    }
    pub(crate) fn validate(&self) -> Result<()> {
        uuid(&self.id)?;
        self.owner.validate()?;
        uuid(&self.draft_id)?;
        revision(self.base_revision)?;
        uuid(&self.writer)
    }
}
impl Conflict {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn owner(&self) -> &Owner {
        &self.owner
    }
    pub fn draft_id(&self) -> &str {
        &self.draft_id
    }
    pub fn base_revision(&self) -> u64 {
        self.base_revision
    }
    pub fn writer(&self) -> &str {
        &self.writer
    }
    pub fn content(&self) -> &Content {
        &self.content
    }
    pub(crate) fn validate(&self) -> Result<()> {
        uuid(&self.id)?;
        self.owner.validate()?;
        uuid(&self.draft_id)?;
        revision(self.base_revision)?;
        uuid(&self.writer)?;
        self.content.validate()
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArchiveReason {
    AbandonedOutcomeUnknown,
    CreatedReviewed,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveReceipt {
    pub(crate) operation: String,
    pub(crate) owner: Owner,
    pub(crate) old_draft: String,
    pub(crate) expected_draft_revision: u64,
    pub(crate) expected_decision_revision: u64,
    pub(crate) old_attempt: String,
    pub(crate) expected_result_revision: u64,
    pub(crate) expected_delivery: Delivery,
    pub(crate) reason: ArchiveReason,
    pub(crate) new_draft: String,
    pub(crate) new_writer: String,
}
impl ArchiveReceipt {
    pub fn operation(&self) -> &str {
        &self.operation
    }
    pub fn old_draft_id(&self) -> &str {
        &self.old_draft
    }
    pub fn new_draft_id(&self) -> &str {
        &self.new_draft
    }
    pub fn reason(&self) -> ArchiveReason {
        self.reason
    }
    pub(crate) fn validate(&self) -> Result<()> {
        uuid(&self.operation)?;
        self.owner.validate()?;
        uuid(&self.old_draft)?;
        uuid(&self.old_attempt)?;
        uuid(&self.new_draft)?;
        uuid(&self.new_writer)?;
        revision(self.expected_draft_revision)?;
        revision(self.expected_decision_revision)?;
        revision(self.expected_result_revision)?;
        self.expected_delivery.validate()?;
        Attempt {
            id: self.old_attempt.clone(),
            owner: self.owner.clone(),
            draft_id: self.old_draft.clone(),
            draft_revision: self.expected_draft_revision,
            frozen: Content::new("metadata validation".into(), String::new())?,
            delivery: self.expected_delivery.clone(),
            result_revision: self.expected_result_revision,
        }
        .validate()?;
        let decision_valid = match self.reason {
            ArchiveReason::AbandonedOutcomeUnknown => matches!(
                self.expected_delivery,
                Delivery::Started | Delivery::Uncertain
            ),
            ArchiveReason::CreatedReviewed => {
                matches!(self.expected_delivery, Delivery::Created { .. })
            }
        };
        if !decision_valid {
            return Err(Fault::Invalid);
        }
        if self.old_draft == self.new_draft {
            return Err(Fault::Invalid);
        }
        Ok(())
    }
}
