//! Inert ordered sequence previews; observed current rows are kept separately.
use super::*;
pub(super) struct Member {
    id: String,
    position: String,
    reused: bool,
    original: Option<String>,
    intended: String,
    reports: usize,
}
pub struct EndReviewAppend<'a> {
    member: &'a Member,
}
impl<'a> EndReviewAppend<'a> {
    pub fn id(&self) -> &'a str {
        &self.member.id
    }
    pub fn position(&self) -> &'a str {
        &self.member.position
    }
    pub fn reused(&self) -> bool {
        self.member.reused
    }
    pub fn original_rtml(&self) -> Option<&'a str> {
        self.member.original.as_deref()
    }
    pub fn intended_rtml(&self) -> &'a str {
        &self.member.intended
    }
    pub fn report_count(&self) -> usize {
        self.member.reports
    }
}
impl EndReview {
    pub fn is_sequence(&self) -> bool {
        !self.sequence_appends.is_empty()
    }
    pub fn appended_members(&self) -> impl ExactSizeIterator<Item = EndReviewAppend<'_>> {
        self.sequence_appends
            .iter()
            .map(|member| EndReviewAppend { member })
    }
}
pub(super) fn capture(c: &Checkpoint) -> Result<EndReview, AppError> {
    let Action::EndSequence { members } = &c.action else {
        return Err(invalid());
    };
    let source = c
        .sections
        .iter()
        .map(|raw| Section::decode(raw))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| invalid())?;
    let mut previews = Vec::new();
    for m in members {
        let original = if m.reused {
            Some(
                text::canonical_draft(source.iter().find(|s| s.id() == m.id).ok_or_else(invalid)?)
                    .and_then(|d| d.rtml())
                    .map_err(|_| invalid())?,
            )
        } else {
            None
        };
        previews.push(Member {
            id: m.id.clone(),
            position: m.position.clone(),
            reused: m.reused,
            original,
            intended: m.updates.last().cloned().unwrap_or_default(),
            reports: m.updates.len(),
        });
    }
    let first = members.first().ok_or_else(invalid)?;
    let visible = |rtml: &str| -> Result<String, AppError> {
        Ok(TextDraft::from_rtml(rtml)
            .map_err(|_| invalid())?
            .runs
            .into_iter()
            .map(|r| r.text)
            .collect())
    };
    Ok(EndReview {
        second_child_id: None,
        second_child_text: None,
        repeat_typing: false,
        empty_enter: false,
        empty_typing: false,
        parent_id: c.target.clone(),
        child_id: first.id.clone(),
        original_text: visible(&c.draft)?,
        reused: first.reused,
        child_text: visible(first.updates.last().map(String::as_str).unwrap_or(""))?,
        original_child_rtml: None,
        sequence_appends: previews,
    })
}
