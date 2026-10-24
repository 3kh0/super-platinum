//! Native end Enter: unchanged parent metadata plus a new or reused paragraph.
//! Optional ordered child typing retains its history; parent SplitInfo is never added.
use super::*;
use std::sync::Arc;

mod accepted;
pub(in crate::canvas::edit::structure) mod clearing;
pub(in crate::canvas::edit::structure) mod empty;
mod prepared;
pub(in crate::canvas::edit::structure) mod sequence;
pub use sequence::EndAppend;
pub(in crate::canvas::edit::structure) mod repeat;
pub(in crate::canvas::edit::structure) mod repeat_typing;
mod reuse;
pub(in crate::canvas::edit::structure) mod typing;
pub(in crate::canvas::edit::structure) mod validation;
pub use accepted::AcceptedEnd;
pub use prepared::PreparedEndEdit;
pub(in crate::canvas::edit::structure) use reuse::candidate as reuse_candidate;
pub(in crate::canvas::edit::structure) use reuse::validate_checkpoint_reuse;
pub(in crate::canvas::edit::structure) use validation::validate_checkpoint_end;

/// Logical agreement only. The renderer owns trusted Enter/range/revision proof.
#[derive(Clone)]
pub struct EndWitness {
    before: TextDraft,
    offset: usize,
    updates: Vec<TextDraft>,
    second_updates: Vec<TextDraft>,
    sequence_updates: Option<Vec<Vec<TextDraft>>>,
    parent_updates: Vec<TextDraft>,
    pre_enter: Option<sequence::witness::Boundary>,
    empty_enter: bool,
    repeat: bool,
}
impl EndWitness {
    pub fn new(
        before: &TextDraft,
        offset_utf16: usize,
        parent: &TextDraft,
        child: &TextDraft,
    ) -> Result<Self, Error> {
        let source = split::plain(before)?;
        if parent != before
            || child != &TextDraft::default()
            || offset_utf16 != source.encode_utf16().count()
        {
            return Err(Error::Changed);
        }
        Ok(Self {
            before: before.clone(),
            offset: offset_utf16,
            updates: Vec::new(),
            second_updates: Vec::new(),
            sequence_updates: None,
            parent_updates: Vec::new(),
            pre_enter: None,
            empty_enter: false,
            repeat: false,
        })
    }
    /// Logical empty-to-two-empty agreement; native event authority stays in the renderer.
    pub fn empty() -> Self {
        Self {
            before: TextDraft::default(),
            offset: 0,
            updates: Vec::new(),
            second_updates: Vec::new(),
            sequence_updates: None,
            parent_updates: Vec::new(),
            pre_enter: None,
            empty_enter: true,
            repeat: false,
        }
    }
    /// Two logical end Enters, with no intervening or subsequent typing.
    pub fn repeat(
        before: &TextDraft,
        offset_utf16: usize,
        parent: &TextDraft,
        first: &TextDraft,
        second: &TextDraft,
    ) -> Result<Self, Error> {
        let mut witness = Self::new(before, offset_utf16, parent, first)?;
        if second != &TextDraft::default() {
            return Err(Error::Changed);
        }
        witness.repeat = true;
        Ok(witness)
    }
    pub fn is_repeat(&self) -> bool {
        self.repeat
    }
    /// Ordered reports in the third paragraph after exactly two end Enters.
    /// The first selected paragraph remains empty; renderer provenance is separate.
    pub fn with_second_child_updates(mut self, updates: &[TextDraft]) -> Result<Self, Error> {
        if self.is_sequence()
            || !self.repeat
            || self.empty_enter
            || !self.updates.is_empty()
            || updates.is_empty()
        {
            return Err(Error::Unsupported);
        }
        typing::validate_updates(updates)?;
        self.second_updates = updates.to_vec();
        Ok(self)
    }
    pub fn is_repeat_typing(&self) -> bool {
        self.repeat && !self.second_updates.is_empty()
    }
    pub fn is_empty_enter(&self) -> bool {
        self.empty_enter
    }
    /// Ordered state-changing, source-equivalent CONTENT reports after Enter.
    /// These are logical snapshots; native input provenance belongs to the renderer.
    pub fn with_child_updates(mut self, updates: &[TextDraft]) -> Result<Self, Error> {
        if self.is_sequence() || self.repeat || (self.empty_enter && updates.is_empty()) {
            return Err(Error::Unsupported);
        }
        typing::validate_updates(updates)?;
        self.updates = updates.to_vec();
        Ok(self)
    }
    pub fn is_empty_typing(&self) -> bool {
        self.empty_enter && !self.updates.is_empty()
    }
    fn child_draft(&self) -> TextDraft {
        self.updates.last().cloned().unwrap_or_default()
    }
}

pub struct EndIntent {
    inventory: Inventory,
    title: CanonicalTitle,
    original: Section,
    seed: TextDraft,
    child: String,
    position: String,
    session: String,
    child_kind: ChildKind,
    empty_enter: bool,
    second: Option<repeat::Second>,
    sequence_members: Option<Vec<repeat::Second>>,
}
enum ChildKind {
    New,
    Reuse(Section),
}
impl EndIntent {
    pub fn capture(snapshot: &CanvasCollabSnapshot, target: &str, now: u64) -> Result<Self, Error> {
        Self::capture_mode(snapshot, target, now, false, false, false)
    }
    /// One Enter in a canonical literal-empty paragraph, optionally followed by child typing.
    pub fn capture_empty(
        snapshot: &CanvasCollabSnapshot,
        target: &str,
        now: u64,
    ) -> Result<Self, Error> {
        Self::capture_mode(snapshot, target, now, true, false, false)
    }
    pub fn capture_repeat(
        snapshot: &CanvasCollabSnapshot,
        target: &str,
        now: u64,
    ) -> Result<Self, Error> {
        Self::capture_mode(snapshot, target, now, false, true, false)
    }
    pub fn is_repeat(&self) -> bool {
        self.second.is_some()
    }
    pub fn is_empty_enter(&self) -> bool {
        self.empty_enter
    }
    fn capture_mode(
        snapshot: &CanvasCollabSnapshot,
        target: &str,
        now: u64,
        empty_enter: bool,
        repeat: bool,
        sequence_cohort: bool,
    ) -> Result<Self, Error> {
        let inventory = Inventory::capture(snapshot)?;
        let original = inventory.get(target)?.clone();
        let seed = inventory.paragraph(&original, now)?;
        if repeat || sequence_cohort {
            repeat::admit(&original, &seed)?;
        } else if empty_enter {
            empty::admit(&original)?;
        } else {
            admit_source(&original, &seed)?;
        }
        let (child, position, child_kind) = if let Some(reused) =
            if empty_enter || repeat || sequence_cohort {
                empty::select(&inventory, target, now)?
            } else {
                reuse::select(&inventory, target, now)?
            } {
            (
                reused.id().to_owned(),
                reused
                    .raw()
                    .string(8)?
                    .ok_or(Error::Unsupported)?
                    .to_owned(),
                ChildKind::Reuse(reused),
            )
        } else {
            let position = inventory.insertion_position(target, Side::After)?;
            let suffix = uuid::Uuid::new_v4().simple().to_string();
            let child = position::section_id(snapshot, &suffix[..25])?;
            if inventory.sections.contains_key(&child) {
                return Err(Error::Changed);
            }
            (child, position, ChildKind::New)
        };
        let second = if repeat {
            Some(repeat::second(snapshot, &inventory, &child, &position)?)
        } else {
            None
        };
        Ok(Self {
            inventory,
            title: CanonicalTitle::capture(snapshot)?,
            original,
            seed,
            child,
            position,
            session: super::super::EditSession::new().id,
            child_kind,
            empty_enter,
            second,
            sequence_members: None,
        })
    }
    pub fn seed(&self) -> &TextDraft {
        &self.seed
    }
    pub fn target_section(&self) -> &str {
        self.original.id()
    }
    pub fn reuses_child(&self) -> bool {
        self.reused().is_some()
    }
    /// Enter clears a captured nonliteral source before optional child typing.
    pub fn clears_child(&self) -> bool {
        self.reused()
            .is_some_and(|s| clearing::source_draft(s).is_ok())
    }
    fn reused(&self) -> Option<&Section> {
        match &self.child_kind {
            ChildKind::New => None,
            ChildKind::Reuse(s) => Some(s),
        }
    }
    pub fn prepare(
        self,
        fresh: &CanvasCollabSnapshot,
        witness: &EndWitness,
        now: u64,
    ) -> Result<PreparedEndEdit, Error> {
        self.revalidate(fresh, now)?;
        if !witness.is_sequence()
            && (!witness.parent_updates.is_empty() || witness.pre_enter.is_some())
        {
            return Err(Error::Changed);
        }
        if self.empty_enter != witness.empty_enter
            || self.is_repeat() != witness.repeat
            || self.is_sequence() != witness.is_sequence()
        {
            return Err(Error::Changed);
        }
        if self.is_sequence() {
            sequence::validate_witness(witness)?;
            if witness.before != self.seed
                || witness.append_count()
                    != self.sequence_members.as_ref().ok_or(Error::Changed)?.len()
            {
                return Err(Error::Changed);
            }
        } else if self.is_repeat() {
            if witness.is_repeat_typing() {
                repeat_typing::witness(witness)?;
            } else {
                repeat::witness(witness)?;
            }
        } else if self.empty_enter {
            if witness.updates.is_empty() {
                empty::witness(witness)?;
            } else {
                empty::typing_witness(witness)?;
            }
        } else {
            EndWitness::new(
                &witness.before,
                witness.offset,
                &self.seed,
                &TextDraft::default(),
            )?;
        }
        PreparedEndEdit::build(Arc::new(self), witness.clone(), now)
    }
    fn revalidate(&self, fresh: &CanvasCollabSnapshot, now: u64) -> Result<(), Error> {
        self.inventory.revalidate(fresh)?;
        self.title.revalidate(fresh)?;
        self.inventory.paragraph(&self.original, now)?;
        if self.is_sequence() {
            sequence::revalidate_plan(self)?;
        }
        if let Some(second) = &self.second {
            repeat::admit(&self.original, &self.seed)?;
            if repeat::second_position(&self.inventory, &self.child, &self.position)?
                != second.position
            {
                return Err(Error::Changed);
            }
        }
        if let Some(reused) = self.reused() {
            let selected = if self.empty_enter || self.is_repeat() || self.is_sequence() {
                empty::select(&self.inventory, self.original.id(), now)?
            } else {
                reuse::select(&self.inventory, self.original.id(), now)?
            };
            if selected.as_ref() != Some(reused) {
                return Err(Error::Changed);
            }
            return Ok(());
        }
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

pub(super) fn admit_source(section: &Section, draft: &TextDraft) -> Result<(), Error> {
    split::plain(draft)?;
    if section.section_type()?.unwrap_or(0) != 0
        || section.style()?.unwrap_or(0) != 0
        || section.parents()?.is_some_and(|p| !p.fields().is_empty())
        || section.read_only()? == Some(true)
        || section.attributes()?.is_some_and(|a| a.has(29))
    {
        return Err(Error::Unsupported);
    }
    if let Some(wire) = section.crdt()? {
        // Fresh-end cohort is a clean existing positive-base ordinary parent.
        // Repeated/split/merge/zero-base-parent protocols remain separate.
        if wire.has(9) || wire.has(10) || wire.uint(2)? == Some(0) {
            return Err(Error::Unsupported);
        }
        let doc = text::crdt::validate(&wire, section, draft, None)?;
        let heads = doc.get_heads();
        if heads.len() != 1 || !split::validation::processed(&wire, &heads[0].to_string())? {
            return Err(Error::Unsupported);
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
