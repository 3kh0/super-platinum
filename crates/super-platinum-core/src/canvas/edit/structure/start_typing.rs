//! Start Enter followed by ordered edits to the original, second paragraph.
//! The new empty paragraph before it remains a separate one-change bootstrap.
use super::*;
use std::sync::Arc;

mod prepared;
mod validation;
pub use prepared::{AcceptedStartTyping, PreparedStartTypingEdit};
pub(in crate::canvas::edit::structure) use validation::validate_request;

/// Logical snapshots only; trusted Enter/caret/event authority belongs to UI.
#[derive(Clone)]
pub struct StartTypingWitness {
    before: TextDraft,
    updates: Vec<TextDraft>,
}
impl StartTypingWitness {
    pub fn new(before: &TextDraft, original_updates: &[TextDraft]) -> Result<Self, Error> {
        split::plain(before)?;
        if original_updates.is_empty() {
            return Err(Error::Unsupported);
        }
        end::typing::validate_updates_from(before, original_updates)?;
        Ok(Self {
            before: before.clone(),
            updates: original_updates.to_vec(),
        })
    }
    pub fn original_draft(&self) -> &TextDraft {
        &self.before
    }
    pub fn parent_draft(&self) -> &TextDraft {
        self.updates.last().expect("validated reports")
    }
    pub fn original_updates(&self) -> &[TextDraft] {
        &self.updates
    }
}

pub struct StartTypingIntent {
    inventory: Inventory,
    title: CanonicalTitle,
    original: Section,
    seed: TextDraft,
    child: String,
    position: String,
    session: String,
}
impl StartTypingIntent {
    pub fn capture(snapshot: &CanvasCollabSnapshot, target: &str, now: u64) -> Result<Self, Error> {
        let inventory = Inventory::capture(snapshot)?;
        let original = inventory.get(target)?.clone();
        let seed = inventory.paragraph(&original, now)?;
        admit(&original, &seed)?;
        let position = inventory.insertion_position(target, Side::Before)?;
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
        witness: &StartTypingWitness,
        now: u64,
    ) -> Result<PreparedStartTypingEdit, Error> {
        self.revalidate(fresh, now)?;
        if witness.before != self.seed {
            return Err(Error::Changed);
        }
        StartTypingWitness::new(&witness.before, &witness.updates)?;
        PreparedStartTypingEdit::build(Arc::new(self), witness.clone(), now)
    }
    fn revalidate(&self, fresh: &CanvasCollabSnapshot, now: u64) -> Result<(), Error> {
        self.inventory.revalidate(fresh)?;
        self.title.revalidate(fresh)?;
        self.inventory.paragraph(&self.original, now)?;
        admit(&self.original, &self.seed)?;
        if self
            .inventory
            .insertion_position(self.original.id(), Side::Before)?
            != self.position
        {
            return Err(Error::Changed);
        }
        Ok(())
    }
}
fn admit(original: &Section, draft: &TextDraft) -> Result<(), Error> {
    end::admit_source(original, draft)?;
    if original.attributes()?.is_some_and(|a| a.has(13)) {
        return Err(Error::Unsupported);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
