use super::*;
use crate::canvas::edit::structure::SplitSequenceTarget;
pub(super) struct SecondReview {
    id: String,
    target: SplitSequenceTarget,
    enter: String,
    parent: String,
    child: String,
    text: String,
}
fn plain(value: &str) -> Result<String, AppError> {
    Ok(TextDraft::from_rtml(value)
        .map_err(|_| invalid())?
        .runs
        .into_iter()
        .map(|r| r.text)
        .collect())
}
pub(super) fn capture(c: &Checkpoint) -> Result<SplitReview, AppError> {
    let Action::SplitSequence {
        child,
        parent_draft,
        child_draft,
        parent_updates,
        child_updates,
        second_child,
        second_target,
        second_enter_draft,
        second_parent_draft,
        second_child_draft,
        final_updates,
        ..
    } = &c.action
    else {
        return Err(invalid());
    };
    let before_parent = parent_updates.last().unwrap_or(parent_draft);
    let before_child = child_updates.last().unwrap_or(child_draft);
    let starts = match second_target {
        SplitSequenceTarget::Original => [second_parent_draft, before_child, second_child_draft],
        SplitSequenceTarget::FirstChild => [before_parent, second_parent_draft, second_child_draft],
    };
    let finals = std::array::from_fn::<_, 3, _>(|i| final_updates[i].last().unwrap_or(starts[i]));
    Ok(SplitReview {
        parent_id: c.target.clone(),
        child_id: child.clone(),
        original_text: plain(&c.draft)?,
        parent_text: plain(finals[0])?,
        initial_parent_text: plain(parent_draft)?,
        parent_report_count: parent_updates.len() + final_updates[0].len(),
        child_text: plain(finals[1])?,
        initial_child_text: plain(child_draft)?,
        has_child_updates: !child_updates.is_empty() || !final_updates[1].is_empty(),
        enter_text: plain(&c.draft)?,
        pre_enter_report_count: 0,
        second: Some(SecondReview {
            id: second_child.clone(),
            target: *second_target,
            enter: plain(second_enter_draft)?,
            parent: plain(second_parent_draft)?,
            child: plain(second_child_draft)?,
            text: plain(finals[2])?,
        }),
    })
}
impl SplitReview {
    pub fn second_child_id(&self) -> Option<&str> {
        self.second.as_ref().map(|s| s.id.as_str())
    }
    pub fn second_target(&self) -> Option<SplitSequenceTarget> {
        self.second.as_ref().map(|s| s.target)
    }
    pub fn second_enter_text(&self) -> Option<&str> {
        self.second.as_ref().map(|s| s.enter.as_str())
    }
    pub fn second_initial_parent_text(&self) -> Option<&str> {
        self.second.as_ref().map(|s| s.parent.as_str())
    }
    pub fn second_initial_child_text(&self) -> Option<&str> {
        self.second.as_ref().map(|s| s.child.as_str())
    }
    pub fn member_ids(&self) -> Vec<&str> {
        let mut rows = vec![self.parent_id(), self.child_id()];
        if let Some(s) = &self.second {
            rows.push(&s.id);
        }
        rows
    }
    pub fn member_texts(&self) -> Vec<&str> {
        let mut rows = vec![self.parent_text(), self.child_text()];
        if let Some(s) = &self.second {
            rows.push(&s.text);
        }
        rows
    }
    pub fn visual_order(&self) -> Vec<usize> {
        match self.second_target() {
            Some(SplitSequenceTarget::Original) => vec![0, 2, 1],
            Some(SplitSequenceTarget::FirstChild) => vec![0, 1, 2],
            None => vec![0, 1],
        }
    }
}
