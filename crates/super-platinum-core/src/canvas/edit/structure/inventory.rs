use super::*;
use std::collections::BTreeMap;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Owner {
    pub workspace: String,
    pub user: String,
    pub file: String,
    pub thread: String,
    pub document: String,
}
impl Owner {
    pub fn capture(snapshot: &CanvasCollabSnapshot) -> Result<Self, Error> {
        Ok(Self {
            workspace: snapshot.init.workspace_id().into(),
            user: snapshot
                .init
                .options()
                .string(33)?
                .ok_or(Error::Changed)?
                .into(),
            file: snapshot.file_id().into(),
            thread: snapshot.thread_id().into(),
            document: snapshot.document_id().into(),
        })
    }
}

/// Raw bytes of *all* sections, including unknown extension fields. Order comes
/// from every canonical positioned entity, never a filtered visible projection.
pub(super) struct Inventory {
    pub owner: Owner,
    pub sections: BTreeMap<String, Section>,
    pub ordered: Vec<String>,
    review_only: bool,
    init: RawMessage,
    thread: RawMessage,
    document: RawMessage,
}
impl Inventory {
    pub fn capture(snapshot: &CanvasCollabSnapshot) -> Result<Self, Error> {
        Self::capture_policy(snapshot, false)
    }
    pub fn capture_for_local_review(snapshot: &CanvasCollabSnapshot) -> Result<Self, Error> {
        Self::capture_policy(snapshot, true)
    }
    fn capture_policy(snapshot: &CanvasCollabSnapshot, review_only: bool) -> Result<Self, Error> {
        super::super::eligible_document(snapshot)?;
        if snapshot.sections.is_empty()
            || snapshot.sections.len() > crate::canvas::wire::MAX_SECTIONS
        {
            return Err(Error::Unsupported);
        }
        let owner = Owner::capture(snapshot)?;
        let mut sections = BTreeMap::new();
        let mut positions = BTreeMap::new();
        let mut bytes = 0usize;
        for section in &snapshot.sections {
            bytes = bytes
                .checked_add(section.raw().raw().len())
                .ok_or(Error::Unsupported)?;
            if bytes > crate::canvas::wire::MAX_BYTES
                || section.document_id()? != Some(owner.document.as_str())
                || section.thread_id()? != Some(owner.thread.as_str())
                || section
                    .sequence()?
                    .is_none_or(|seq| seq == 0 || seq > i64::MAX as u64)
                || (!review_only && section.deleted()? == Some(true))
                || section.raw().uint(11)?.unwrap_or(0) != 0
                || section.parents()?.is_some_and(|p| !p.fields().is_empty())
                || section.raw().boolean(29)? == Some(true)
                || section.raw().has(15)
            {
                return Err(Error::Unsupported);
            }
            let kind = (
                section.section_type()?.unwrap_or(0),
                section.style()?.unwrap_or(0),
            );
            if !matches!(kind, (0, 0) | (64, 48)) {
                return Err(Error::Unsupported);
            }
            let value = section.raw().string(8)?.ok_or(Error::Unsupported)?;
            // Canonical-only bounded cohort: temporary positions can require a
            // second source mutation (B61P deduplication), which is not this intent.
            position::canonical(value)?;
            if section.raw().string(21)?.is_some_and(|path| path != value)
                || positions
                    .insert(value.to_owned(), section.id().to_owned())
                    .is_some()
                || sections
                    .insert(section.id().to_owned(), section.clone())
                    .is_some()
            {
                return Err(Error::Unsupported);
            }
        }
        Ok(Self {
            owner,
            sections,
            ordered: positions.into_values().collect(),
            review_only,
            init: snapshot.init.options().clone(),
            thread: snapshot.thread.clone(),
            document: snapshot.document.clone(),
        })
    }
    pub fn revalidate(&self, snapshot: &CanvasCollabSnapshot) -> Result<(), Error> {
        let fresh = Self::capture_policy(snapshot, self.review_only)?;
        if self.owner != fresh.owner
            || self.sections != fresh.sections
            || self.ordered != fresh.ordered
            || self.init != fresh.init
            || self.thread != fresh.thread
            || self.document != fresh.document
        {
            return Err(Error::Changed);
        }
        Ok(())
    }
    pub fn get(&self, id: &str) -> Result<&Section, Error> {
        self.sections.get(id).ok_or(Error::Changed)
    }
    pub fn paragraph(&self, section: &Section, now: u64) -> Result<TextDraft, Error> {
        if section.section_type()?.unwrap_or(0) != 0
            || section.style()?.unwrap_or(0) != 0
            || section.read_only()? == Some(true)
            || section.attributes()?.is_some_and(|a| a.has(29))
        {
            return Err(Error::Unsupported);
        }
        let draft = text::canonical_draft(section)?;
        if let Some(wire) = section.crdt()? {
            // A modern stable ID may retain the initial base_sequence0 after
            // its canonical section sequence advances. It is not a new local
            // entity or a legacy temporary-ID remap.
            crdt::validate(
                &wire,
                section,
                &draft,
                false,
                Some(now),
                position::modern_id(section.id()),
            )?;
        }
        Ok(draft)
    }
    pub fn insertion_position(&self, anchor: &str, side: Side) -> Result<String, Error> {
        let index = self
            .ordered
            .iter()
            .position(|id| id == anchor)
            .ok_or(Error::Changed)?;
        self.insertion_position_at(index, side)
    }
    pub fn insertion_position_at(&self, index: usize, side: Side) -> Result<String, Error> {
        if index >= self.ordered.len() {
            return Err(Error::Changed);
        }
        let value = |index: usize| -> Result<&str, Error> {
            self.get(&self.ordered[index])?
                .raw()
                .string(8)?
                .ok_or(Error::Unsupported)
        };
        let (before, after) = match side {
            Side::Before => (
                index.checked_sub(1).map(value).transpose()?,
                Some(value(index)?),
            ),
            Side::After => (
                Some(value(index)?),
                (index + 1 < self.ordered.len())
                    .then(|| value(index + 1))
                    .transpose()?,
            ),
        };
        position::between(before, after)
    }
    pub fn safe_delete(&self, id: &str) -> Result<(), Error> {
        self.get(id)?;
        // previousSibling()/nextSibling() use REACHABLE, not ALL. A static title
        // can be hidden by renderer configuration, so it is not proof of a
        // reachable sibling. Ordinary flat paragraphs cannot be hidden titles,
        // orphans, missing-layout children or descendants in this inventory.
        let mut sibling = false;
        for section in self.sections.values() {
            if section.id() != id
                && section.section_type()?.unwrap_or(0) == 0
                && section.style()?.unwrap_or(0) == 0
            {
                sibling = true;
                break;
            }
        }
        if !sibling {
            return Err(Error::Unsupported);
        }
        Ok(())
    }
}
