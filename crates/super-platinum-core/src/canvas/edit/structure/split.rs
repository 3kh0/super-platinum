//! One witnessed paragraph split, preserving parent history and original lineage.
//! Renderer generation/range/event authority remains outside this logical model.
use super::*;
use std::sync::Arc;

mod checkpoint;
pub(in crate::canvas::edit::structure) mod sequence;
pub use sequence::SplitSequenceTarget;
mod parent_updates;
mod pre_enter;
mod prepared;
mod typing;
pub(in crate::canvas::edit::structure) use checkpoint::validate_checkpoint_split;
mod lineage;
pub(in crate::canvas::edit::structure) use lineage::gap_tombstone;
pub(in crate::canvas::edit::structure) use lineage::retained_owner_child;
pub(in crate::canvas::edit::structure) use lineage::{
    child_delete_owner, child_delete_transition, child_tombstone,
};
pub(super) mod validation;
pub(in crate::canvas::edit) use lineage::{
    associated_sections as split_child, parent_history as split_parent_history,
    same_lineage as same_split_lineage,
};
pub use prepared::{AcceptedSplit, PreparedSplitEdit};

/// Logical agreement of an immutable source and one middle split. Constructing
/// this value does not attest that Enter occurred; the renderer owns that fact.
#[derive(Clone)]
pub struct SplitWitness {
    before: TextDraft,
    parent: TextDraft,
    child: TextDraft,
    offset: usize,
    updates: Vec<TextDraft>,
    pre_updates: Vec<TextDraft>,
    parent_updates: Vec<TextDraft>,
    second: Option<sequence::Second>,
}
impl SplitWitness {
    pub fn new(
        before: &TextDraft,
        offset_utf16: usize,
        parent: &TextDraft,
        child: &TextDraft,
    ) -> Result<Self, Error> {
        let source = plain(before)?;
        let left = plain(parent)?;
        let right = plain(child)?;
        let boundary = utf16_boundary(&source, offset_utf16).ok_or(Error::Changed)?;
        if boundary == 0
            || boundary == source.len()
            || source.get(..boundary) != Some(left.as_str())
            || source.get(boundary..) != Some(right.as_str())
        {
            return Err(Error::Changed);
        }
        Ok(Self {
            before: before.clone(),
            parent: parent.clone(),
            child: child.clone(),
            offset: offset_utf16,
            updates: Vec::new(),
            pre_updates: Vec::new(),
            parent_updates: Vec::new(),
            second: None,
        })
    }
    /// Logical CONTENT report boundaries; native event authority stays with the renderer.
    pub fn with_child_updates(mut self, updates: &[TextDraft]) -> Result<Self, Error> {
        if self.second.is_some() {
            return Err(Error::Unsupported);
        }
        super::end::typing::validate_updates_from(&self.child, updates)?;
        pre_enter::budget(&self.pre_updates, &self.parent_updates, updates)?;
        self.updates = updates.to_vec();
        Ok(self)
    }
    pub fn initial_child_draft(&self) -> &TextDraft {
        &self.child
    }
    pub fn child_draft(&self) -> &TextDraft {
        self.sequence_final(1)
            .unwrap_or_else(|| self.updates.last().unwrap_or(&self.child))
    }
    pub fn child_updates(&self) -> &[TextDraft] {
        &self.updates
    }
}
/// Convert code units only at a scalar boundary; never round a split inside an
/// astral character. Callers choose visible text or serialized RTML explicitly.
fn utf16_boundary(value: &str, offset: usize) -> Option<usize> {
    let mut units = 0;
    for (byte, ch) in value.char_indices() {
        if units == offset {
            return Some(byte);
        }
        units += ch.len_utf16();
        if units > offset {
            return None;
        }
    }
    (units == offset).then_some(value.len())
}
pub(super) fn plain(draft: &TextDraft) -> Result<String, Error> {
    let value = draft
        .runs
        .iter()
        .map(|r| r.text.as_str())
        .collect::<String>();
    if value.is_empty()
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}' | '\u{feff}'))
        || draft
            .runs
            .iter()
            .any(|r| r.style != text::TextStyle::default())
        || TextDraft::from_rtml(&draft.rtml()?)? != *draft
    {
        return Err(Error::Unsupported);
    }
    Ok(value)
}

pub struct SplitIntent {
    inventory: Inventory,
    title: CanonicalTitle,
    original: Section,
    seed: TextDraft,
    child: String,
    position: String,
    session: String,
}
impl SplitIntent {
    pub fn capture(snapshot: &CanvasCollabSnapshot, target: &str, now: u64) -> Result<Self, Error> {
        let inventory = Inventory::capture(snapshot)?;
        let original = inventory.get(target)?.clone();
        if original.crdt()?.is_some_and(|wire| wire.has(10))
            || inventory.is_retained_split_child(target)?
        {
            return Err(Error::Unsupported);
        }
        let seed = inventory.paragraph(&original, now)?;
        plain(&seed)?;
        // The source may reuse a trailing empty sibling. This new-child cohort
        // refuses that branch rather than creating a different structure.
        if let Some(last) = inventory.ordered.last().filter(|id| id.as_str() != target) {
            let s = inventory.get(last)?;
            if s.section_type()?.unwrap_or(0) == 0
                && s.style()?.unwrap_or(0) == 0
                && s.parents()?.is_none_or(|p| p.fields().is_empty())
                && text::canonical_draft(s).is_ok_and(|d| d.runs.is_empty())
            {
                return Err(Error::Unsupported);
            }
        }
        let position = inventory.insertion_position(target, Side::After)?;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let child = position::section_id(snapshot, &suffix[..25])?;
        if inventory.sections.contains_key(&child) {
            return Err(Error::Changed);
        }
        Ok(Self {
            inventory,
            title: CanonicalTitle::capture(snapshot)?,
            original,
            seed,
            child,
            position,
            session: super::super::EditSession::new().id,
        })
    }
    pub fn seed(&self) -> &TextDraft {
        &self.seed
    }
    pub fn target_section(&self) -> &str {
        self.original.id()
    }
    pub fn prepare(
        self,
        fresh: &CanvasCollabSnapshot,
        witness: &SplitWitness,
        now: u64,
    ) -> Result<PreparedSplitEdit, Error> {
        self.revalidate(fresh, now)?;
        let checked = witness.checked()?;
        if checked.before != self.seed {
            return Err(Error::Changed);
        }
        PreparedSplitEdit::build(Arc::new(self), checked, now)
    }
    fn revalidate(&self, fresh: &CanvasCollabSnapshot, now: u64) -> Result<(), Error> {
        self.inventory.revalidate(fresh)?;
        self.title.revalidate(fresh)?;
        self.inventory.paragraph(&self.original, now)?;
        if self
            .inventory
            .insertion_position(self.original.id(), Side::After)?
            != self.position
        {
            return Err(Error::Changed);
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests;
