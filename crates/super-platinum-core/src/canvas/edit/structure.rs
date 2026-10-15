//! Explicit top-level structural attempts from a complete canonical snapshot.
//! An intent is consumed once; uncertain delivery never becomes an automatic retry.
use super::text::{self, TextDraft};
use super::{CanonicalTitle, CanvasCollabSnapshot, Error, F, RawMessage, Section};

mod crdt;
pub(super) use crdt::canonical_zero_base_history;
mod inventory;
mod position;
mod prepared;
pub mod recovery;
mod replacement;
use inventory::Inventory;
pub use prepared::{AcceptedStructure, PreparedStructureEdit};
pub use replacement::{
    AcceptedReplacement, PreparedReplacement, ReplacementIntent, can_replace_main_canvas,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Before,
    After,
}

enum Operation {
    Insert {
        id: String,
        anchor: String,
        side: Side,
        position: String,
        draft: TextDraft,
    },
    Delete {
        id: String,
        draft: TextDraft,
    },
}
impl Operation {
    fn id(&self) -> &str {
        match self {
            Self::Insert { id, .. } | Self::Delete { id, .. } => id,
        }
    }
    fn draft(&self) -> &TextDraft {
        match self {
            Self::Insert { draft, .. } | Self::Delete { draft, .. } => draft,
        }
    }
    fn deleted(&self) -> bool {
        matches!(self, Self::Delete { .. })
    }
}

/// Read-only presentation eligibility, never dispatch authority. Captures no ID
/// or session and validates the complete structural inventory once per batch.
pub struct ParagraphActions {
    pub seed: TextDraft,
    pub insert_before: bool,
    pub insert_after: bool,
    pub delete: bool,
}
pub fn paragraph_actions(
    snapshot: &CanvasCollabSnapshot,
    targets: &[&str],
    now: u64,
) -> Vec<Option<ParagraphActions>> {
    let Ok(inventory) = Inventory::capture(snapshot) else {
        return targets.iter().map(|_| None).collect();
    };
    if CanonicalTitle::capture(snapshot).is_err() {
        return targets.iter().map(|_| None).collect();
    }
    // Neighbor lookup must not rescan the document for every projected row.
    let indices: std::collections::BTreeMap<_, _> = inventory
        .ordered
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect();
    targets
        .iter()
        .map(|id| {
            let index = *indices.get(id)?;
            let section = inventory.get(id).ok()?;
            let seed = inventory.paragraph(section, now).ok()?;
            Some(ParagraphActions {
                seed,
                insert_before: inventory.insertion_position_at(index, Side::Before).is_ok(),
                insert_after: inventory.insertion_position_at(index, Side::After).is_ok(),
                delete: inventory.safe_delete(id).is_ok(),
            })
        })
        .collect()
}

/// Renderer must additionally own its account/credential generation and the
/// file-wide mutation lock. This type seals canonical content, not UI ownership.
pub struct StructureIntent {
    inventory: Inventory,
    title: CanonicalTitle,
    operation: Operation,
    session: String,
}
impl StructureIntent {
    pub fn insert(
        snapshot: &CanvasCollabSnapshot,
        anchor: &str,
        side: Side,
        draft: &TextDraft,
        now: u64,
    ) -> Result<Self, Error> {
        let inventory = Inventory::capture(snapshot)?;
        let title = CanonicalTitle::capture(snapshot)?;
        let section = inventory.get(anchor)?;
        inventory.paragraph(section, now)?;
        let position = inventory.insertion_position(anchor, side)?;
        let draft = TextDraft::from_rtml(&draft.rtml()?)?;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let id = position::section_id(snapshot, &suffix[..25])?;
        if inventory.sections.contains_key(&id) {
            return Err(Error::Changed);
        }
        Ok(Self {
            inventory,
            title,
            operation: Operation::Insert {
                id,
                anchor: anchor.into(),
                side,
                position,
                draft,
            },
            session: super::EditSession::new().id,
        })
    }

    pub fn delete(snapshot: &CanvasCollabSnapshot, id: &str, now: u64) -> Result<Self, Error> {
        let inventory = Inventory::capture(snapshot)?;
        let title = CanonicalTitle::capture(snapshot)?;
        let target = inventory.get(id)?;
        let draft = inventory.paragraph(target, now)?;
        inventory.safe_delete(id)?;
        Ok(Self {
            inventory,
            title,
            operation: Operation::Delete {
                id: id.into(),
                draft,
            },
            session: super::EditSession::new().id,
        })
    }

    pub fn target_section(&self) -> &str {
        self.operation.id()
    }

    /// One fresh stock-format session per structural intent. No retained actor
    /// can accidentally recreate a prior uncertain section history.
    pub fn prepare(
        self,
        fresh: &CanvasCollabSnapshot,
        now: u64,
    ) -> Result<PreparedStructureEdit, Error> {
        self.inventory.revalidate(fresh)?;
        self.title.revalidate(fresh)?;
        match &self.operation {
            Operation::Insert {
                anchor,
                side,
                position,
                ..
            } => {
                self.inventory.paragraph(self.inventory.get(anchor)?, now)?;
                if self.inventory.insertion_position(anchor, *side)? != *position {
                    return Err(Error::Changed);
                }
            }
            Operation::Delete { id, .. } => {
                self.inventory.paragraph(self.inventory.get(id)?, now)?;
                self.inventory.safe_delete(id)?;
            }
        }
        PreparedStructureEdit::build(self, now)
    }
}

#[cfg(test)]
pub(crate) mod tests;
