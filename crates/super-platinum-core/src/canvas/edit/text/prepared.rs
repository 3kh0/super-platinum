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
    pub(super) split_child: Option<Section>,
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
    pub fn heading_level(&self) -> Option<u8> {
        heading_level(&self.canonical)
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
        if eligible(fresh, section, now)? != self.split_child {
            return Err(Error::Changed);
        }
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
        let update = self.data.message(1)?.ok_or(Error::Unsupported)?;
        let outgoing = update.message(38)?.ok_or(Error::Unsupported)?;
        if !super::super::structure::same_split_lineage(&wire, &outgoing)? {
            return Err(Error::Unconfirmed);
        }
        if self.split_child.is_some() {
            if !processed_contains(&wire, outgoing.string(12)?.ok_or(Error::Unconfirmed)?)?
                || section.raw().uint(20)? != update.uint(20)?
                || section.raw().string(33)? != update.string(33)?
                || zero_stable(&self.canonical)?
                    != zero_stable(&self.canonical.merge_partial(section)?)?
            {
                return Err(Error::Unconfirmed);
            }
        }
        let zero_sent_head = if outgoing.uint(2)? == Some(0) {
            let sent_head = outgoing.string(12)?.ok_or(Error::Unsupported)?;
            if !processed_contains(&wire, sent_head)?
                || wire.uint(2)? != Some(0)
                || section.raw().uint(20)? != update.uint(20)?
                || wire.string(8)? != outgoing.string(8)?
                || zero_stable(&self.canonical)?
                    != zero_stable(&self.canonical.merge_partial(section)?)?
            {
                return Err(Error::Unconfirmed);
            }
            let sent = automerge::Automerge::load(outgoing.bytes(5)?.ok_or(Error::Unsupported)?)
                .map_err(|_| Error::Unsupported)?;
            if sent
                .get_changes(&[])
                .iter()
                .any(|change| document.get_change_by_hash(&change.hash()).is_none())
            {
                return Err(Error::Unconfirmed);
            }
            Some(sent_head.to_owned())
        } else {
            None
        };
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
        // canonical_draft above admits only singular Content.text and
        // ContentText.text/language with their exact types. A server serializer
        // may place text before retained language; field order is not content.
        // Compare exact payloads (including absent vs empty language), not the
        // normalized draft, so different RTML cannot confirm the sent body.
        let expected = content(&self.canonical, &self.desired)?
            .message(1)?
            .ok_or(Error::Unconfirmed)?;
        let actual = section
            .raw()
            .message(12)?
            .ok_or(Error::Unconfirmed)?
            .message(1)?
            .ok_or(Error::Unconfirmed)?;
        if actual.bytes(1)? != expected.bytes(1)? || actual.bytes(2)? != expected.bytes(2)? {
            return Err(Error::Unconfirmed);
        }
        let baseline = self.canonical.merge_partial(section)?;
        // CONTENT projection omits heading style and structural siblings. Check
        // the merged canonical row, not defaults inferred from a partial ACK.
        if self.heading_level().is_some()
            && heading_stable(&self.canonical)? != heading_stable(&baseline)?
        {
            return Err(Error::Unconfirmed);
        }
        Ok(AcceptedText {
            key: self.key.clone(),
            baseline,
            section: section.clone(),
            desired: self.desired.clone(),
            session: self.session.clone(),
            sequence: self.sequence,
            zero_sent_head,
            split_child: self.split_child.clone(),
        })
    }
}

pub struct AcceptedText {
    split_child: Option<Section>,
    // Sealed outgoing head; a descendant alone cannot confirm this save.
    zero_sent_head: Option<String>,
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
    pub fn heading_level(&self) -> Option<u8> {
        heading_level(&self.baseline)
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
        if let Some(child) = self.split_child.as_ref() {
            return self.confirms_split_readback(snapshot, section, child);
        }
        // Even a later revision cannot confirm this text-only operation across
        // a heading conversion or structural change. Paragraph policy is intact.
        if self.heading_level().is_some()
            && heading_stable(&self.baseline)? != heading_stable(section)?
        {
            return Ok(false);
        }
        if self
            .section
            .crdt()?
            .is_some_and(|wire| wire.uint(2).ok() == Some(Some(0)))
        {
            return self.confirms_zero_base_readback(section);
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
    fn confirms_split_readback(
        &self,
        snapshot: &CanvasCollabSnapshot,
        section: &Section,
        child: &Section,
    ) -> Result<bool, Error> {
        if section.sequence()? != self.section.sequence()?
            || snapshot.section(child.id()) != Some(child)
            || readback_association(section)? != readback_association(&self.baseline)?
        {
            return Ok(false);
        }
        let wire = section.crdt()?.ok_or(Error::Unconfirmed)?;
        let acknowledged = self.section.crdt()?.ok_or(Error::Unconfirmed)?;
        if !super::super::structure::same_split_lineage(&wire, &acknowledged)?
            || super::super::structure::split_child(&snapshot.sections, section, 0)?.as_ref()
                != Some(child)
            || wire.uint(2)? != acknowledged.uint(2)?
            || wire.uint(3)? != section.sequence()?
            || wire.string(8)? != acknowledged.string(8)?
        {
            return Ok(false);
        }
        let doc = crdt::validate(&wire, section, &self.desired, Some(0))?;
        let ack_doc = automerge::Automerge::load(acknowledged.bytes(5)?.ok_or(Error::Unconfirmed)?)
            .map_err(|_| Error::Unconfirmed)?;
        Ok(ack_doc
            .get_changes(&[])
            .iter()
            .all(|c| doc.get_change_by_hash(&c.hash()).is_some()))
    }
    fn confirms_zero_base_readback(&self, section: &Section) -> Result<bool, Error> {
        // No current-server omission or later-revision normalization is claimed
        // for this new cohort. All existing positive-base policy stays unchanged.
        if section.sequence()? != self.section.sequence()?
            || readback_association(section)? != readback_association(&self.baseline)?
        {
            return Ok(false);
        }
        let Some(wire) = section.crdt()? else {
            return Ok(false);
        };
        let acknowledged = self.section.crdt()?.ok_or(Error::Unconfirmed)?;
        let Some(sent_head) = self.zero_sent_head.as_deref() else {
            return Ok(false);
        };
        if !processed_contains(&wire, sent_head)?
            || wire.uint(2)? != Some(0)
            || wire.string(8)? != acknowledged.string(8)?
        {
            return Ok(false);
        }
        let Ok(doc) = crdt::validate(&wire, section, &self.desired, Some(0)) else {
            return Ok(false);
        };
        let ack_doc = automerge::Automerge::load(acknowledged.bytes(5)?.ok_or(Error::Unconfirmed)?)
            .map_err(|_| Error::Unconfirmed)?;
        Ok(ack_doc
            .get_changes(&[])
            .iter()
            .all(|change| doc.get_change_by_hash(&change.hash()).is_some()))
    }
}

// Preserve exact stable sibling bytes, including ContentText.language, while
// permitting text and the already-reviewed write/receipt bookkeeping to change.
fn heading_stable(section: &Section) -> Result<RawMessage, Error> {
    canonical_draft(section)?;
    let content = section.raw().message(12)?.ok_or(Error::Unconfirmed)?;
    let text = content.message(1)?.ok_or(Error::Unconfirmed)?.remove(1)?;
    let content = content.replace_fields(1, &[F::message(1, &text)?])?;
    Ok(zero_stable(section)?.replace_fields(12, &[F::message(12, &content)?])?)
}

// Only reviewed write/receipt fields may differ in a base0 text ACK. Reorder
// independent identity tags only; preserve all other relative raw-field bytes.
fn zero_stable(section: &Section) -> Result<RawMessage, Error> {
    if section.section_type()?.unwrap_or(0) != 0 {
        return Err(Error::Unconfirmed);
    }
    let mut raw = retained::association(section)?;
    for tag in [2, 9, 12, 19, 20, 33] {
        raw = raw.remove(tag)?;
    }
    let mut fields = raw.fields().iter().collect::<Vec<_>>();
    // Mechanical partial merge appends present fields. Equal heading attrs
    // may therefore move around untouched opaque siblings. Normalize only the
    // independent outer tag16 position; nested bytes/order stay exact.
    let heading = heading_level(section).is_some();
    fields.sort_by_key(|field| match field.tag() {
        1 | 6 | 7 => field.tag(),
        16 if heading => 16,
        _ => 0,
    });
    let mut bytes = Vec::new();
    for field in fields {
        bytes.extend_from_slice(raw.encoded_field(field)?);
    }
    Ok(RawMessage::decode(&bytes)?)
}

fn processed_contains(wire: &RawMessage, head: &str) -> Result<bool, Error> {
    for field in wire.fields().iter().filter(|field| field.tag() == 6) {
        let value = RawMessage::decode(wire.encoded_field(field)?)?;
        if value.string(6)? == Some(head) {
            return Ok(true);
        }
    }
    Ok(false)
}

// Keep the base0 exact-sequence/body/history association, allowing only the
// outer attribute relocation caused by merging omitted versus explicit attrs.
fn readback_association(section: &Section) -> Result<RawMessage, Error> {
    let raw = retained::association(section)?;
    if heading_level(section).is_none() {
        return Ok(raw);
    }
    let mut fields = raw.fields().iter().collect::<Vec<_>>();
    fields.sort_by_key(|field| field.tag() == 16);
    let mut bytes = Vec::new();
    for field in fields {
        bytes.extend_from_slice(raw.encoded_field(field)?);
    }
    Ok(RawMessage::decode(&bytes)?)
}
