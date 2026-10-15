//! One consumed two-target replacement for the Main Canvas renderer policy.
//! Current 6Dac canvas mode sets skipUntitledOnNewDoc; current-selected P3oa
//! maps it to omitBuiltInHeadingOrTitleSection, with retired registry disabled.
//! This is not an editor/flexpane/preview constructor or a replay capability.
use super::*;
use crate::canvas::wire::{DocumentData, EditAck};

pub struct ReplacementIntent {
    inventory: Inventory,
    title: CanonicalTitle,
    original: Section,
    draft: TextDraft,
    replacement: String,
    position: String,
    session: String,
}

/// Canonical shape is necessary; the explicitly named public constructor also
/// requires its caller to own a Main Canvas view and the targeted body action.
fn sole_body(inventory: &Inventory, target: &str) -> Result<(), Error> {
    if inventory.sections.len() != 2 || inventory.ordered.last().map(String::as_str) != Some(target)
    {
        return Err(Error::Unsupported);
    }
    let title = inventory.get(&inventory.ordered[0])?;
    if title.section_type()? != Some(64) || title.style()? != Some(48) {
        return Err(Error::Unsupported);
    }
    Ok(())
}

/// Read-only presentation check; creates neither an ID nor a session.
pub fn can_replace_main_canvas(snapshot: &CanvasCollabSnapshot, target: &str, now: u64) -> bool {
    // Ordinary multi-row canvases must not clone/validate their complete
    // inventory once for every visible paragraph during presentation.
    if snapshot.sections.len() != 2 {
        return false;
    }
    (|| -> Result<(), Error> {
        let inventory = Inventory::capture(snapshot)?;
        sole_body(&inventory, target)?;
        CanonicalTitle::capture(snapshot)?;
        inventory.paragraph(inventory.get(target)?, now)?;
        inventory.insertion_position(target, Side::After)?;
        Ok(())
    })()
    .is_ok()
}

impl ReplacementIntent {
    /// Capture an explicit delete of the sole body in a Main Canvas surface.
    /// The renderer must own that surface/action and its file-wide mutation lock.
    /// Do not use for editor, preview or flexpane modes: their title is reachable.
    pub fn main_canvas(
        snapshot: &CanvasCollabSnapshot,
        target: &str,
        now: u64,
    ) -> Result<Self, Error> {
        let inventory = Inventory::capture(snapshot)?;
        sole_body(&inventory, target)?;
        let original = inventory.get(target)?.clone();
        let draft = inventory.paragraph(&original, now)?;
        let position = inventory.insertion_position(target, Side::After)?;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let id = position::section_id(snapshot, &suffix[..25])?;
        if inventory.sections.contains_key(&id) {
            return Err(Error::Changed);
        }
        let session = super::super::EditSession::new().id;
        Ok(Self {
            inventory,
            title: CanonicalTitle::capture(snapshot)?,
            original,
            draft,
            replacement: id,
            position,
            session,
        })
    }

    pub fn target_section(&self) -> &str {
        self.original.id()
    }
    pub fn replacement_section(&self) -> &str {
        &self.replacement
    }

    #[cfg(test)]
    fn fixture(
        snapshot: &CanvasCollabSnapshot,
        target: &str,
        hidden_title: bool,
        disabled_registry: bool,
        now: u64,
    ) -> Result<Self, Error> {
        if !hidden_title || !disabled_registry {
            return Err(Error::Unsupported);
        }
        Self::main_canvas(snapshot, target, now)
    }

    pub fn prepare(
        self,
        fresh: &CanvasCollabSnapshot,
        now: u64,
    ) -> Result<PreparedReplacement, Error> {
        self.inventory.revalidate(fresh)?;
        self.title.revalidate(fresh)?;
        sole_body(&self.inventory, self.original.id())?;
        self.inventory.paragraph(&self.original, now)?;
        if self
            .inventory
            .insertion_position(self.original.id(), Side::After)?
            != self.position
        {
            return Err(Error::Changed);
        }
        // One captured inventory/title/session creates both private validators.
        // The second immutable inventory is captured from the already revalidated
        // borrowed snapshot, with no await or external mutation in between.
        let insertion = StructureIntent {
            inventory: Inventory::capture(fresh)?,
            title: CanonicalTitle::capture(fresh)?,
            operation: Operation::Insert {
                id: self.replacement,
                anchor: self.original.id().into(),
                side: Side::After,
                position: self.position,
                draft: TextDraft::default(),
            },
            session: self.session.clone(),
        }
        .prepare(fresh, now)?;
        // A dedicated pair: the single-row safe_delete guard stays unchanged.
        let deletion = PreparedStructureEdit::build(
            StructureIntent {
                inventory: self.inventory,
                title: self.title,
                operation: Operation::Delete {
                    id: self.original.id().into(),
                    draft: self.draft,
                },
                session: self.session,
            },
            now,
        )?;
        let old = deletion
            .document_data()
            .message(1)?
            .ok_or(Error::Unsupported)?;
        let new = insertion
            .document_data()
            .message(1)?
            .ok_or(Error::Unsupported)?;
        let data = RawMessage::from_fields(&[
            F::message(1, &old)?,
            F::message(1, &new)?,
            F::message(
                14,
                &deletion
                    .document_data()
                    .message(14)?
                    .ok_or(Error::Unsupported)?,
            )?,
        ])?;
        Ok(PreparedReplacement {
            deletion,
            insertion,
            original: self.original,
            data,
        })
    }
}

pub struct PreparedReplacement {
    deletion: PreparedStructureEdit,
    insertion: PreparedStructureEdit,
    original: Section,
    data: RawMessage,
}
impl PreparedReplacement {
    pub fn workspace_id(&self) -> &str {
        self.deletion.workspace_id()
    }
    pub fn user_id(&self) -> &str {
        self.deletion.user_id()
    }
    pub fn file_id(&self) -> &str {
        self.deletion.file_id()
    }
    pub fn thread_id(&self) -> &str {
        self.deletion.thread_id()
    }
    pub fn document_id(&self) -> &str {
        self.deletion.document_id()
    }
    pub fn target_section(&self) -> &str {
        self.deletion.target_section()
    }
    pub fn session_id(&self) -> &str {
        self.deletion.session_id()
    }
    pub fn sequence(&self) -> u64 {
        self.deletion.sequence()
    }
    pub fn form_title(&self) -> &str {
        self.deletion.form_title()
    }
    pub fn replacement_section(&self) -> &str {
        self.insertion.target_section()
    }

    pub fn document_data(&self) -> &RawMessage {
        &self.data
    }
    pub fn checkpoint(&self) -> Result<recovery::Checkpoint, Error> {
        recovery::Checkpoint::pair(
            self.deletion.checkpoint()?,
            self.insertion.checkpoint()?,
            &self.data,
        )
    }
    pub fn validate_snapshot(&self, fresh: &CanvasCollabSnapshot, now: u64) -> Result<(), Error> {
        self.deletion.validate_snapshot(fresh, now)?;
        self.insertion.validate_snapshot(fresh, now)
    }
    pub fn validate_ack(self, ack: &EditAck) -> Result<AcceptedReplacement, Error> {
        // Derive both public projections from the same retained response bytes.
        let decoded = EditAck::decode(ack.raw().raw())?;
        let ack = &decoded;
        // A bounded complete envelope: unreviewed auxiliary updates cannot be
        // discarded merely because each requested section appears somewhere.
        if ack
            .raw()
            .fields()
            .iter()
            .any(|f| !matches!(f.tag(), 1 | 2 | 6))
        {
            return Err(Error::Unconfirmed);
        }
        let data = ack.final_data.as_ref().ok_or(Error::Unconfirmed)?;
        if data
            .raw()
            .fields()
            .iter()
            .any(|f| !matches!(f.tag(), 1 | 15))
            || data.sections().len() != 2
        {
            return Err(Error::Unconfirmed);
        }
        let old = data
            .sections()
            .iter()
            .find(|s| s.id() == self.deletion.target_section())
            .ok_or(Error::Unconfirmed)?;
        let new = data
            .sections()
            .iter()
            .find(|s| s.id() == self.insertion.target_section())
            .ok_or(Error::Unconfirmed)?;
        let trace = format!("{}-0", self.deletion.session_id());
        if old.id() == new.id()
            || old.raw().string(33)? != Some(trace.as_str())
            || new.raw().string(33)? != Some(trace.as_str())
        {
            return Err(Error::Unconfirmed);
        }
        let member = |section: &Section| -> Result<EditAck, Error> {
            // Retain every outer field and all document identity metadata.
            // Only the reviewed section collection is narrowed for reuse of
            // the unchanged single-member validator. No accepted member escapes.
            let raw = data
                .raw()
                .replace_fields(1, &[F::message(1, section.raw())?])?;
            let mut result = ack.clone();
            result.final_data = Some(DocumentData::from_raw(raw)?);
            Ok(result)
        };
        let deletion = self.deletion.validate_ack(&member(old)?)?;
        let insertion = self.insertion.validate_ack(&member(new)?)?;
        Ok(AcceptedReplacement {
            deletion,
            insertion,
            original: self.original,
        })
    }
}

pub struct AcceptedReplacement {
    deletion: AcceptedStructure,
    insertion: AcceptedStructure,
    original: Section,
}
impl AcceptedReplacement {
    pub fn target_section(&self) -> &str {
        self.deletion.target_section()
    }
    pub fn session_id(&self) -> &str {
        self.deletion.session_id()
    }
    pub fn sequence(&self) -> u64 {
        self.deletion.sequence()
    }
    pub fn replacement_section(&self) -> &str {
        self.insertion.target_section()
    }
    pub fn deleted_section(&self) -> &Section {
        self.deletion.section()
    }
    pub fn inserted_section(&self) -> &Section {
        self.insertion.section()
    }

    pub fn confirms_readback(&self, snapshot: &CanvasCollabSnapshot) -> Result<bool, Error> {
        let mut ids = std::collections::BTreeSet::new();
        let mut bytes = 0usize;
        for section in &snapshot.sections {
            bytes = bytes
                .checked_add(section.raw().raw().len())
                .ok_or(Error::Unsupported)?;
            if !ids.insert(section.id()) || bytes > crate::canvas::wire::MAX_BYTES {
                return Ok(false);
            }
        }
        if snapshot.sections.len() > crate::canvas::wire::MAX_SECTIONS {
            return Ok(false);
        }
        // Algebraic composition of the existing full-inventory proofs. The
        // insertion is excluded only from the deletion view; its exact sealed
        // target/history is checked by the insertion view. The old original is
        // restored only in that latter view; the deletion view checks its actual
        // tombstone/history or sealed absence. Unknown untouched bytes stay exact.
        let mut deleted_view = snapshot.clone();
        deleted_view
            .sections
            .retain(|s| s.id() != self.insertion.target_section());
        if !self.deletion.confirms_readback(&deleted_view)? {
            return Ok(false);
        }
        let mut inserted_view = snapshot.clone();
        inserted_view
            .sections
            .retain(|s| s.id() != self.deletion.target_section());
        inserted_view.sections.push(self.original.clone());
        self.insertion.confirms_readback(&inserted_view)
    }
}

#[cfg(test)]
mod tests;
