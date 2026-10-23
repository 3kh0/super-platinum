//! Captured and intended texts are separate from all current observations.
use super::*;
use crate::canvas::edit::structure::recovery::Action;

pub struct StartTypingReview {
    original_id: String,
    child_id: String,
    original_text: String,
    parent_text: String,
}
impl StartTypingReview {
    pub(super) fn capture(checkpoint: &Checkpoint) -> Result<Option<Self>, AppError> {
        let Action::StartTyping {
            child,
            original_updates,
            ..
        } = &checkpoint.action
        else {
            return Ok(None);
        };
        let plain = |value: &str| -> Result<String, AppError> {
            Ok(TextDraft::from_rtml(value)
                .map_err(|_| invalid())?
                .runs
                .into_iter()
                .map(|r| r.text)
                .collect())
        };
        Ok(Some(Self {
            original_id: checkpoint.target.clone(),
            child_id: child.clone(),
            original_text: plain(&checkpoint.draft)?,
            parent_text: plain(original_updates.last().ok_or_else(invalid)?)?,
        }))
    }
    pub fn original_id(&self) -> &str {
        &self.original_id
    }
    pub fn child_id(&self) -> &str {
        &self.child_id
    }
    pub fn original_text(&self) -> &str {
        &self.original_text
    }
    pub fn parent_text(&self) -> &str {
        &self.parent_text
    }
    pub fn child_text(&self) -> &str {
        ""
    }
}
