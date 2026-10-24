//! Read-only presentation of one validated, durable split receipt. Text and IDs
//! are not an editable draft, current server result or permission to replay.
use super::*;
use crate::canvas::edit::structure::recovery::Action;

mod sequence;
pub struct SplitReview {
    parent_id: String,
    child_id: String,
    original_text: String,
    parent_text: String,
    initial_parent_text: String,
    parent_report_count: usize,
    child_text: String,
    initial_child_text: String,
    has_child_updates: bool,
    enter_text: String,
    pre_enter_report_count: usize,
    second: Option<sequence::SecondReview>,
}
impl SplitReview {
    pub(super) fn capture(checkpoint: &Checkpoint) -> Result<Option<Self>, AppError> {
        if matches!(checkpoint.action, Action::SplitSequence { .. }) {
            return sequence::capture(checkpoint).map(Some);
        }
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
                Action::SplitPreEnter {
                    child,
                    parent_draft,
                    child_draft,
                    child_updates,
                    ..
                }
                | Action::SplitParent {
                    child,
                    parent_draft,
                    child_draft,
                    child_updates,
                    ..
                } => (
                    child,
                    parent_draft,
                    child_draft,
                    child_updates.last().unwrap_or(child_draft),
                    !child_updates.is_empty(),
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
        let (enter, pre_enter_report_count) = match &checkpoint.action {
            Action::SplitPreEnter {
                enter_draft,
                pre_enter_updates,
                ..
            }
            | Action::SplitParent {
                enter_draft,
                pre_enter_updates,
                ..
            } => (enter_draft, pre_enter_updates.len()),
            _ => (&checkpoint.draft, 0),
        };
        let (final_parent, parent_report_count) = match &checkpoint.action {
            Action::SplitParent { parent_updates, .. } => (
                parent_updates.last().ok_or_else(invalid)?,
                parent_updates.len(),
            ),
            _ => (parent_draft, 0),
        };
        Ok(Some(Self {
            parent_id: checkpoint.target.clone(),
            child_id: child.clone(),
            original_text: plain(&checkpoint.draft)?,
            parent_text: plain(final_parent)?,
            initial_parent_text: plain(parent_draft)?,
            parent_report_count,
            child_text: plain(final_child)?,
            initial_child_text: plain(initial)?,
            has_child_updates,
            enter_text: plain(enter)?,
            pre_enter_report_count,
            second: None,
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
    pub fn initial_parent_text(&self) -> &str {
        &self.initial_parent_text
    }
    pub fn parent_report_count(&self) -> usize {
        self.parent_report_count
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
    pub fn enter_text(&self) -> &str {
        &self.enter_text
    }
    pub fn pre_enter_report_count(&self) -> usize {
        self.pre_enter_report_count
    }
    pub fn has_child_updates(&self) -> bool {
        self.has_child_updates
    }
}

#[cfg(test)]
mod tests;
