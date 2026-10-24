//! Trusted-input chronology is renderer-owned. This binds immutable source,
//! actual live middle boundary, and every changed pre-Enter CONTENT report.
use super::*;

impl SplitWitness {
    pub fn new_with_pre_enter_updates(
        original: &TextDraft,
        enter_draft: &TextDraft,
        offset_utf16: usize,
        parent: &TextDraft,
        child: &TextDraft,
        pre_updates: &[TextDraft],
    ) -> Result<Self, Error> {
        plain(original)?;
        if pre_updates.last() != Some(enter_draft) {
            return Err(Error::Changed);
        }
        super::super::end::typing::validate_updates_from(original, pre_updates)?;
        budget(pre_updates, &[], &[])?;
        let mut witness = Self::new(enter_draft, offset_utf16, parent, child)?;
        witness.before = original.clone();
        witness.pre_updates = pre_updates.to_vec();
        Ok(witness)
    }
    pub fn pre_enter_updates(&self) -> &[TextDraft] {
        &self.pre_updates
    }
    pub fn enter_draft(&self) -> &TextDraft {
        self.pre_updates.last().unwrap_or(&self.before)
    }
    pub(super) fn checked(&self) -> Result<Self, Error> {
        let witness = if self.pre_updates.is_empty() {
            Self::new(&self.before, self.offset, &self.parent, &self.child)?
        } else {
            Self::new_with_pre_enter_updates(
                &self.before,
                self.enter_draft(),
                self.offset,
                &self.parent,
                &self.child,
                &self.pre_updates,
            )?
        };
        let first = witness
            .with_parent_updates(&self.parent_updates)?
            .with_child_updates(&self.updates)?;
        if let Some(second) = &self.second {
            first
                .with_second_split(
                    second.target,
                    &second.enter,
                    second.offset,
                    &second.parent,
                    &second.child,
                )?
                .with_final_updates(&second.updates[0], &second.updates[1], &second.updates[2])
        } else {
            Ok(first)
        }
    }
}

// Prelude, parent and child reports share the existing operation-wide resource budget.
pub(super) fn budget(
    pre: &[TextDraft],
    parent: &[TextDraft],
    child: &[TextDraft],
) -> Result<(), Error> {
    if pre
        .len()
        .checked_add(parent.len())
        .and_then(|n| n.checked_add(child.len()))
        .ok_or(Error::Unsupported)?
        > 4096
    {
        return Err(Error::Unsupported);
    }
    let mut bytes = 0usize;
    for draft in pre.iter().chain(parent).chain(child) {
        bytes = bytes
            .checked_add(draft.rtml()?.len())
            .ok_or(Error::Unsupported)?;
        if bytes > 8 * 1024 * 1024 {
            return Err(Error::Unsupported);
        }
    }
    Ok(())
}

pub(super) fn live(
    seed: &RawMessage,
    witness: &SplitWitness,
    session: &str,
    now: u64,
) -> Result<RawMessage, Error> {
    super::super::end::typing::build_reports(seed, true, &witness.pre_updates, session, now)
}
