//! Read-only presentation of one validated, durable split receipt. Text and IDs
//! are not an editable draft, current server result or permission to replay.
use super::*;
use crate::canvas::edit::structure::recovery::Action;

pub struct SplitReview {
    parent_id: String,
    child_id: String,
    original_text: String,
    parent_text: String,
    child_text: String,
    initial_child_text: String,
    has_child_updates: bool,
}
impl SplitReview {
    pub(super) fn capture(checkpoint: &Checkpoint) -> Result<Option<Self>, AppError> {
        let (child, parent_draft, initial, final_child, has_child_updates) =
            match &checkpoint.action {
                Action::Split {
                    child,
                    parent_draft,
                    child_draft,
                    ..
                } => (child, parent_draft, child_draft, child_draft, false),
                Action::SplitTyping {
                    child,
                    parent_draft,
                    child_draft,
                    child_updates,
                    ..
                } => (
                    child,
                    parent_draft,
                    child_draft,
                    child_updates.last().ok_or_else(invalid)?,
                    true,
                ),
                _ => return Ok(None),
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
            parent_id: checkpoint.target.clone(),
            child_id: child.clone(),
            original_text: plain(&checkpoint.draft)?,
            parent_text: plain(parent_draft)?,
            child_text: plain(final_child)?,
            initial_child_text: plain(initial)?,
            has_child_updates,
        }))
    }
    pub fn parent_id(&self) -> &str {
        &self.parent_id
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
        &self.child_text
    }
    pub fn initial_child_text(&self) -> &str {
        &self.initial_child_text
    }
    pub fn has_child_updates(&self) -> bool {
        self.has_child_updates
    }
}

#[cfg(test)]
mod tests;
