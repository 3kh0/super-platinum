//! Both intended members of an unresolved end Enter; never replay authority.
use super::*;
use crate::canvas::edit::structure::recovery::Action;
use crate::canvas::edit::text;
use crate::canvas::wire::Section;

mod sequence;
pub use sequence::EndReviewAppend;

pub struct EndReview {
    sequence_appends: Vec<sequence::Member>,
    second_child_id: Option<String>,
    second_child_text: Option<String>,
    repeat_typing: bool,
    empty_enter: bool,
    empty_typing: bool,
    parent_id: String,
    child_id: String,
    original_text: String,
    parent_text: String,
    parent_updates: Vec<String>,
    reused: bool,
    child_text: String,
    original_child_rtml: Option<String>,
}
impl EndReview {
    pub(super) fn capture(checkpoint: &Checkpoint) -> Result<Option<Self>, AppError> {
        if checkpoint.is_sequence() {
            return sequence::capture(checkpoint).map(Some);
        }
        let (child, reused) = match &checkpoint.action {
            Action::End { child, .. } => (child, false),
            Action::EndReuse { child, .. } | Action::EndClearing { child, .. } => (child, true),
            Action::EndRepeat { child, reused, .. }
            | Action::EndRepeatTyping { child, reused, .. }
            | Action::EmptyEnter { child, reused, .. }
            | Action::EmptyTyping { child, reused, .. }
            | Action::EndTyping { child, reused, .. } => (child, *reused),
            _ => return Ok(None),
        };
        let text: String = TextDraft::from_rtml(&checkpoint.draft)
            .map_err(|_| invalid())?
            .runs
            .into_iter()
            .map(|r| r.text)
            .collect();
        let original_child_rtml = if matches!(checkpoint.action, Action::EndClearing { .. }) {
            let source = checkpoint
                .sections
                .iter()
                .map(|raw| Section::decode(raw))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| invalid())?;
            let section = source
                .iter()
                .find(|s| s.id() == child)
                .ok_or_else(invalid)?;
            Some(
                text::canonical_draft(section)
                    .map_err(|_| invalid())?
                    .rtml()
                    .map_err(|_| invalid())?,
            )
        } else {
            None
        };
        let child_text = match &checkpoint.action {
            Action::EndClearing { child_updates, .. } => {
                TextDraft::from_rtml(child_updates.last().map(String::as_str).unwrap_or(""))
                    .map_err(|_| invalid())?
                    .runs
                    .into_iter()
                    .map(|r| r.text)
                    .collect()
            }
            Action::EndTyping { child_updates, .. } | Action::EmptyTyping { child_updates, .. } => {
                TextDraft::from_rtml(child_updates.last().ok_or_else(invalid)?)
                    .map_err(|_| invalid())?
                    .runs
                    .into_iter()
                    .map(|r| r.text)
                    .collect()
            }
            _ => String::new(),
        };
        Ok(Some(Self {
            sequence_appends: Vec::new(),
            second_child_id: match &checkpoint.action {
                Action::EndRepeat { second_child, .. }
                | Action::EndRepeatTyping { second_child, .. } => Some(second_child.clone()),
                _ => None,
            },
            second_child_text: match &checkpoint.action {
                Action::EndRepeat { .. } => Some(String::new()),
                Action::EndRepeatTyping {
                    second_child_updates,
                    ..
                } => Some(
                    TextDraft::from_rtml(second_child_updates.last().ok_or_else(invalid)?)
                        .map_err(|_| invalid())?
                        .runs
                        .into_iter()
                        .map(|r| r.text)
                        .collect(),
                ),
                _ => None,
            },
            repeat_typing: checkpoint.is_repeat_typing(),
            empty_enter: checkpoint.is_empty_enter(),
            empty_typing: checkpoint.is_empty_typing(),
            parent_id: checkpoint.target.clone(),
            child_id: child.clone(),
            parent_text: text.clone(),
            parent_updates: Vec::new(),
            original_text: text,
            reused,
            child_text,
            original_child_rtml,
        }))
    }
    pub fn is_repeat(&self) -> bool {
        self.second_child_id.is_some()
    }
    pub fn is_repeat_typing(&self) -> bool {
        self.repeat_typing
    }
    pub fn second_child_id(&self) -> Option<&str> {
        self.second_child_id.as_deref()
    }
    pub fn second_child_text(&self) -> Option<&str> {
        self.second_child_text.as_deref()
    }
    pub fn is_empty_enter(&self) -> bool {
        self.empty_enter
    }
    pub fn is_empty_typing(&self) -> bool {
        self.empty_typing
    }
    pub fn clears_child(&self) -> bool {
        self.original_child_rtml.is_some()
    }
    pub fn original_child_rtml(&self) -> Option<&str> {
        self.original_child_rtml.as_deref()
    }
    pub fn parent_id(&self) -> &str {
        &self.parent_id
    }
    pub fn child_id(&self) -> &str {
        &self.child_id
    }
    pub fn reuses_child(&self) -> bool {
        self.reused
    }
    pub fn original_text(&self) -> &str {
        &self.original_text
    }
    pub fn parent_text(&self) -> &str {
        &self.parent_text
    }
    pub fn parent_report_count(&self) -> usize {
        self.parent_updates.len()
    }
    pub fn parent_updates(&self) -> &[String] {
        &self.parent_updates
    }
    pub fn child_text(&self) -> &str {
        &self.child_text
    }
}
