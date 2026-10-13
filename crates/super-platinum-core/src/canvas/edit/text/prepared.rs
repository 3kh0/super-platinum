use super::*;
use crate::canvas::wire::EditAck;
use automerge::ReadDoc;

/// Consumed by one request. No Clone or raw-body constructor is exposed.
pub struct PreparedTextEdit {
    pub(super) key: retained::Key,
    pub(super) canonical: Section,
    pub(super) desired: TextDraft,
    pub(super) title: Arc<CanonicalTitle>,
    pub(super) session: String,
    pub(super) sequence: u64,
    pub(super) data: RawMessage,
    pub(super) expiry: Option<u64>,
}
impl PreparedTextEdit {
    pub fn file_id(&self) -> &str {
        &self.key.file
    }
    pub fn thread_id(&self) -> &str {
        &self.key.thread
    }
    pub fn document_id(&self) -> &str {
        &self.key.document
    }
    pub fn workspace_id(&self) -> &str {
        &self.key.workspace
    }
    pub fn user_id(&self) -> &str {
        &self.key.user
    }
    pub fn section_id(&self) -> &str {
        &self.key.section
    }
    pub fn target_section(&self) -> &str {
        &self.key.section
    }
    pub fn session_id(&self) -> &str {
        &self.session
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn base_sequence(&self) -> u64 {
        self.canonical.sequence().ok().flatten().unwrap_or(0)
    }
    pub fn form_title(&self) -> &str {
        self.title.form_title()
    }
    pub fn document_data(&self) -> &RawMessage {
        &self.data
    }
    pub fn desired(&self) -> &TextDraft {
        &self.desired
    }
    pub(crate) fn validate_snapshot(
        &self,
        fresh: &CanvasCollabSnapshot,
        now: u64,
    ) -> Result<(), Error> {
        super::super::eligible_document(fresh)?;
        let section = fresh.section(&self.key.section).ok_or(Error::Changed)?;
        if retained::Key::capture(fresh, section)? != self.key || section != &self.canonical {
            return Err(Error::Changed);
        }
        if self.expiry.is_some_and(|expires| expires <= now) {
            return Err(Error::Expired);
        }
        eligible(fresh, section, now)?;
        self.title.revalidate(fresh)
    }
    pub fn validate_ack(&self, ack: &EditAck) -> Result<AcceptedText, Error> {
        if ack.changed != Some(true)
            || ack.is_new_edit_session == Some(true)
            || ack.cell_data.is_some()
            || ack.import_sections.is_some()
            || ack.mount_points.is_some()
        {
            return Err(Error::Unconfirmed);
        }
        let data = ack.final_data.as_ref().ok_or(Error::Unconfirmed)?;
        if data
            .document_id()?
            .is_some_and(|id| id != self.key.document)
        {
            return Err(Error::Unconfirmed);
        }
        let [section] = data.sections() else {
            return Err(Error::Unconfirmed);
        };
        if section.id() != self.key.section
            || section.document_id()? != Some(self.key.document.as_str())
            || section.thread_id()? != Some(self.key.thread.as_str())
            || section.deleted()? == Some(true)
            || section
                .sequence()?
                .is_none_or(|seq| seq <= self.base_sequence())
            || section.parents()?.is_some_and(|p| !p.fields().is_empty())
            || canonical_draft(section)? != self.desired
        {
            return Err(Error::Unconfirmed);
        }
        let wire = section.crdt()?.ok_or(Error::Unconfirmed)?;
        let document = crdt::validate(&wire, section, &self.desired, Some(0))?;
        let outgoing = self
            .data
            .message(1)?
            .ok_or(Error::Unsupported)?
            .message(38)?
            .ok_or(Error::Unsupported)?;
        let sent_head = outgoing
            .string(12)?
            .ok_or(Error::Unsupported)?
            .parse()
            .map_err(|_| Error::Unsupported)?;
        if document.get_change_by_hash(&sent_head).is_none()
            || wire.uint(3)? != section.sequence()?
        {
            return Err(Error::Unconfirmed);
        }
        // ContentText.language must not disappear while confirming a text edit.
        let expected = content(&self.canonical, &self.desired)?;
        if section.raw().message(12)? != Some(expected) {
            return Err(Error::Unconfirmed);
        }
        Ok(AcceptedText {
            key: self.key.clone(),
            baseline: self.canonical.merge_partial(section)?,
            section: section.clone(),
            desired: self.desired.clone(),
            session: self.session.clone(),
            sequence: self.sequence,
        })
    }
}

pub struct AcceptedText {
    pub(super) key: retained::Key,
    pub(super) baseline: Section,
    pub(super) section: Section,
    pub(super) desired: TextDraft,
    pub(super) session: String,
    pub(super) sequence: u64,
}
impl AcceptedText {
    pub fn section(&self) -> &Section {
        &self.section
    }
    pub fn session_id(&self) -> &str {
        &self.session
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn confirms_readback(&self, snapshot: &CanvasCollabSnapshot) -> Result<bool, Error> {
        let Some(section) = snapshot.section(&self.key.section) else {
            return Ok(false);
        };
        if retained::Key::capture(snapshot, section)? != self.key
            || section.deleted()? == Some(true)
            || section
                .sequence()?
                .is_none_or(|seq| seq < self.section.sequence().ok().flatten().unwrap_or(u64::MAX))
        {
            return Ok(false);
        }
        // The direct sealed ACK already proves this save. A later canonical
        // revision may contain another author's subsequent edit; do not hold
        // the file's mutation lock until that author recreates our old text.
        if section.sequence()? > self.section.sequence()? {
            return Ok(true);
        }
        // ACK sections are partials. For an ordinary top-level paragraph the
        // server can omit Parents, while a full read encodes an empty message.
        // Both describe zero ancestors; compare the merged canonical baseline,
        // never the presence bit of an optional field in the raw ACK.
        Ok(canonical_draft(section)? == self.desired
            && section.parents()?.is_none_or(|p| p.fields().is_empty())
            && self
                .baseline
                .parents()?
                .is_none_or(|p| p.fields().is_empty()))
    }
}
