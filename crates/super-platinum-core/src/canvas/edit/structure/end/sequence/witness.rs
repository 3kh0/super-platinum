//! Logical first-Enter boundary after source-bound original-row reports.
//! This evidence is not native event authority and is never restored for replay.
use super::*;

#[derive(Clone)]
pub(in crate::canvas::edit::structure::end) struct Boundary {
    draft: TextDraft,
    reports: usize,
}

impl EndWitness {
    /// Original-row reports before first Enter, followed by later row histories.
    /// `original` stays bound to the captured source; the draft and UTF-16 offset
    /// describe the actual first Enter, including a source erased to empty.
    /// The renderer separately attests trusted events, selection and chronology.
    pub fn sequence_with_pre_enter_updates(
        original: &TextDraft,
        enter_draft: &TextDraft,
        actual_offset_utf16: usize,
        pre_updates: &[TextDraft],
        later_parent_updates: &[TextDraft],
        append_reports: &[Vec<TextDraft>],
    ) -> Result<Self, Error> {
        split::plain(original)?;
        if pre_updates.last() != Some(enter_draft) {
            return Err(Error::Changed);
        }
        end_boundary(enter_draft, actual_offset_utf16)?;
        let count = pre_updates
            .len()
            .checked_add(later_parent_updates.len())
            .ok_or(Error::Unsupported)?;
        budget::counts(append_reports.len(), count, 0)?;
        // Bound borrowed parent evidence before allocating the combined history.
        let mut bytes = 0usize;
        for report in pre_updates.iter().chain(later_parent_updates) {
            bytes = bytes
                .checked_add(report.rtml()?.len())
                .ok_or(Error::Unsupported)?;
            budget::counts(append_reports.len(), count, bytes)?;
        }
        typing::validate_updates_from(original, pre_updates)?;
        typing::validate_updates_from(enter_draft, later_parent_updates)?;
        let parent_updates = pre_updates
            .iter()
            .chain(later_parent_updates)
            .cloned()
            .collect::<Vec<_>>();
        validate_borrowed_stages(
            original,
            &parent_updates,
            append_reports.iter().map(Vec::as_slice),
        )?;
        Ok(Self {
            before: original.clone(),
            offset: actual_offset_utf16,
            updates: Vec::new(),
            second_updates: Vec::new(),
            sequence_updates: Some(append_reports.to_vec()),
            parent_updates,
            pre_enter: Some(Boundary {
                draft: enter_draft.clone(),
                reports: pre_updates.len(),
            }),
            empty_enter: false,
            repeat: false,
        })
    }
}

fn end_boundary(draft: &TextDraft, offset: usize) -> Result<(), Error> {
    let length = if draft == &TextDraft::default() {
        0
    } else {
        split::plain(draft)?.encode_utf16().count()
    };
    if offset != length {
        return Err(Error::Changed);
    }
    Ok(())
}

pub(super) fn validate_boundary(w: &EndWitness) -> Result<(), Error> {
    match &w.pre_enter {
        None => {
            EndWitness::new(&w.before, w.offset, &w.before, &TextDraft::default())?;
        }
        Some(boundary) => {
            split::plain(&w.before)?;
            if boundary.reports == 0
                || w.parent_updates.get(boundary.reports - 1) != Some(&boundary.draft)
            {
                return Err(Error::Changed);
            }
            end_boundary(&boundary.draft, w.offset)?;
        }
    }
    Ok(())
}
