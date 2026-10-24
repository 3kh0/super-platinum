use super::*;
use crate::canvas::wire::EditAck;
use automerge::ReadDoc;

pub struct PreparedStructureEdit {
    intent: StructureIntent,
    data: RawMessage,
    outgoing_crdt: RawMessage,
    expiry: Option<u64>,
    dependent_delete: bool,
    retained_pair_delete: bool,
}
impl PreparedStructureEdit {
    pub(super) fn build(intent: StructureIntent, now: u64) -> Result<Self, Error> {
        let id = intent.operation.id();
        let trace = format!("{}-0", intent.session);
        let mut dependent_delete = false;
        let mut retained_pair_delete = false;
        let (fields, wire, expiry) = match &intent.operation {
            Operation::Insert {
                position, draft, ..
            } => {
                let wire = crdt::insert(id, draft, &intent.session, now)?;
                let fields = creation_fields(id, position, &intent.session, draft, &wire)?;
                (fields, wire, None)
            }
            Operation::Delete { draft, .. } => {
                let original = intent.inventory.get(id)?;
                let (wire, mut expiry) = crdt::delete(original, draft, &intent.session, now)?;
                if wire.has(10) {
                    let child =
                        owner_delete::live_pair(intent.inventory.sections.values(), original, now)?;
                    retained_pair_delete = child.deleted()? == Some(true);
                    let child_expiry = child
                        .crdt()?
                        .ok_or(Error::Unsupported)?
                        .uint(4)?
                        .ok_or(Error::Expired)?;
                    expiry = Some(expiry.ok_or(Error::Expired)?.min(child_expiry));
                }
                if !wire.has(10) {
                    if let Some(owner) = split::child_delete_owner(
                        intent.inventory.sections.values(),
                        original,
                        now,
                    )? {
                        dependent_delete = true;
                        let lease = owner
                            .crdt()?
                            .ok_or(Error::Unsupported)?
                            .uint(4)?
                            .ok_or(Error::Expired)?;
                        expiry = Some(expiry.ok_or(Error::Expired)?.min(lease));
                    }
                }
                let fields = vec![
                    F::bytes(1, id.as_bytes())?,
                    F::uint(2, original.sequence()?.ok_or(Error::Unsupported)?)?,
                    F::uint(3, 1)?,
                    F::uint(9, 0)?,
                    F::message(13, &RawMessage::decode(&[])?)?,
                    F::uint(19, 1)?,
                    F::uint(20, 1)?,
                    F::bytes(33, trace.as_bytes())?,
                    F::message(38, &wire)?,
                ];
                (fields, wire, expiry)
            }
        };
        let data = RawMessage::from_fields(&[
            F::message(1, &RawMessage::from_fields(&fields)?)?,
            F::message(14, intent.title.rich_text())?,
        ])?;
        Ok(Self {
            intent,
            data,
            outgoing_crdt: wire,
            expiry,
            dependent_delete,
            retained_pair_delete,
        })
    }
    pub fn workspace_id(&self) -> &str {
        &self.intent.inventory.owner.workspace
    }
    pub fn user_id(&self) -> &str {
        &self.intent.inventory.owner.user
    }
    pub fn file_id(&self) -> &str {
        &self.intent.inventory.owner.file
    }
    pub fn thread_id(&self) -> &str {
        &self.intent.inventory.owner.thread
    }
    pub fn document_id(&self) -> &str {
        &self.intent.inventory.owner.document
    }
    pub fn target_section(&self) -> &str {
        self.intent.operation.id()
    }
    pub fn session_id(&self) -> &str {
        &self.intent.session
    }
    pub fn sequence(&self) -> u64 {
        1
    }
    pub fn form_title(&self) -> &str {
        self.intent.title.form_title()
    }
    pub fn document_data(&self) -> &RawMessage {
        &self.data
    }
    pub fn is_split_owner_delete(&self) -> bool {
        self.intent.operation.deleted() && self.outgoing_crdt.has(10)
    }
    pub(crate) fn validate_dispatch_expiry(&self, now: u64) -> Result<(), Error> {
        if self
            .expiry
            .is_some_and(|expires| expires <= now || expires > i64::MAX as u64)
        {
            return Err(Error::Expired);
        }
        Ok(())
    }
    pub(crate) fn validate_snapshot(
        &self,
        fresh: &CanvasCollabSnapshot,
        now: u64,
    ) -> Result<(), Error> {
        self.intent.inventory.revalidate(fresh)?;
        self.intent.title.revalidate(fresh)?;
        self.validate_dispatch_expiry(now)
    }

    /// Only the direct response to this consumed request may produce a receipt.
    /// A matching websocket event or canonical read cannot manufacture this proof.
    pub fn validate_ack(self, ack: &EditAck) -> Result<AcceptedStructure, Error> {
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
            .is_some_and(|id| id != self.document_id())
        {
            return Err(Error::Unconfirmed);
        }
        let [section] = data.sections() else {
            return Err(Error::Unconfirmed);
        };
        let base = match &self.intent.operation {
            Operation::Insert { .. } => 0,
            Operation::Delete { id, .. } => self
                .intent
                .inventory
                .get(id)?
                .sequence()?
                .ok_or(Error::Unsupported)?,
        };
        if section.id() != self.target_section()
            || section.raw().has(17)
            || section.document_id()? != Some(self.document_id())
            || section.thread_id()? != Some(self.thread_id())
            || section.raw().uint(20)? != Some(1)
            || section
                .sequence()?
                .is_none_or(|seq| seq <= base || seq > i64::MAX as u64)
            || section.deleted()?.unwrap_or(false) != self.intent.operation.deleted()
            || section.parents()?.is_some_and(|p| !p.fields().is_empty())
            || section.raw().boolean(29)? == Some(true)
        {
            return Err(Error::Unconfirmed);
        }
        let canonical = match &self.intent.operation {
            Operation::Insert {
                position, draft, ..
            } => {
                // A new entity has no previously reviewed canonical extension
                // state. Admit only this cohort's projection and known receipt
                // metadata; do not silently accept new semantic fields/status.
                if section.raw().fields().iter().any(|field| {
                    !matches!(
                        field.tag(),
                        1 | 2
                            | 3
                            | 6
                            | 7
                            | 8
                            | 9
                            | 10
                            | 11
                            | 12
                            | 13
                            | 16
                            | 18
                            | 19
                            | 20
                            | 21
                            | 29
                            | 33
                            | 35
                            | 38
                            | 103
                            | 104
                    )
                }) || section.section_type()?.unwrap_or(0) != 0
                    || section.style()?.unwrap_or(0) != 0
                    || section.raw().uint(11)?.unwrap_or(0) != 0
                    || section.read_only()? == Some(true)
                    || position::base(section.raw().string(8)?.ok_or(Error::Unconfirmed)?)?
                        != position::base(position)?
                    || section
                        .raw()
                        .string(21)?
                        .is_some_and(|p| position::base(p).ok() != position::base(position).ok())
                    || section.raw().message(12)? != Some(new_content(draft)?)
                    || section.attributes()? != Some(RawMessage::from_fields(&[F::uint(4, 0)?])?)
                {
                    return Err(Error::Unconfirmed);
                }
                section.clone()
            }
            Operation::Delete { id, .. } => {
                let original = self.intent.inventory.get(id)?;
                let merged = original.merge_partial(section)?;
                if deletion_stable(&merged)? != deletion_stable(original)? {
                    return Err(Error::Unconfirmed);
                }
                merged
            }
        };
        let wire = section.crdt()?.ok_or(Error::Unconfirmed)?;
        if self.dependent_delete {
            let original = self.intent.inventory.get(self.target_section())?;
            split::child_delete_transition(original, &wire, &canonical)?;
            let rows = self.intent.inventory.sections.values().map(|r| {
                if r.id() == canonical.id() {
                    &canonical
                } else {
                    r
                }
            });
            split::child_tombstone(rows, &canonical, 0)?;
        }
        if let Operation::Delete { id, draft } = &self.intent.operation {
            let original = self.intent.inventory.get(id)?;
            if original.crdt()?.is_some_and(|w| w.has(10)) {
                if wire.has(12) {
                    return Err(Error::Unconfirmed);
                }
                owner_delete::transition(original, &wire, &canonical, draft)?;
                if self.retained_pair_delete {
                    let rows = self.intent.inventory.sections.values().map(|r| {
                        if r.id() == canonical.id() {
                            &canonical
                        } else {
                            r
                        }
                    });
                    let child = owner_delete::tombstone(rows, &canonical)?;
                    if child.deleted()? != Some(true) {
                        return Err(Error::Unconfirmed);
                    }
                }
            }
        }
        let doc = crdt::validate(
            &wire,
            &canonical,
            self.intent.operation.draft(),
            self.intent.operation.deleted(),
            Some(0),
            self.outgoing_crdt.uint(2)? == Some(0),
        )?;
        let sent = self
            .outgoing_crdt
            .string(12)?
            .ok_or(Error::Unsupported)?
            .parse()
            .map_err(|_| Error::Unsupported)?;
        let mut sent_processed = false;
        for field in wire.fields().iter().filter(|field| field.tag() == 6) {
            let field = RawMessage::decode(wire.encoded_field(field)?)?;
            if field.string(6)? == self.outgoing_crdt.string(12)? {
                sent_processed = true;
            }
        }
        if doc.get_change_by_hash(&sent).is_none()
            || !sent_processed
            || wire.uint(3)? != section.sequence()?
            || wire.uint(2)? != self.outgoing_crdt.uint(2)?
            || wire.string(8)? != self.outgoing_crdt.string(8)?
        {
            return Err(Error::Unconfirmed);
        }
        // Preserve all old changes, not merely equal text/deleted state. This
        // rejects replacement history that happens to contain one copied head.
        let outgoing =
            automerge::Automerge::load(self.outgoing_crdt.bytes(5)?.ok_or(Error::Unsupported)?)
                .map_err(|_| Error::Unsupported)?;
        if outgoing
            .get_changes(&[])
            .iter()
            .any(|change| doc.get_change_by_hash(&change.hash()).is_none())
        {
            return Err(Error::Unconfirmed);
        }
        Ok(AcceptedStructure {
            sent_head: self
                .outgoing_crdt
                .string(12)?
                .ok_or(Error::Unsupported)?
                .to_owned(),
            intent: self.intent,
            section: canonical,
            dependent_delete: self.dependent_delete,
            retained_pair_delete: self.retained_pair_delete,
        })
    }
}

pub struct AcceptedStructure {
    intent: StructureIntent,
    section: Section,
    sent_head: String,
    dependent_delete: bool,
    retained_pair_delete: bool,
}
impl AcceptedStructure {
    pub fn section(&self) -> &Section {
        &self.section
    }
    pub fn target_section(&self) -> &str {
        self.intent.operation.id()
    }
    pub fn session_id(&self) -> &str {
        &self.intent.session
    }
    pub fn sequence(&self) -> u64 {
        1
    }
    /// A separate complete canonical load is required. Later concurrent changes
    /// are retained as unresolved; this method never guesses which writer won.
    pub fn confirms_readback(&self, snapshot: &CanvasCollabSnapshot) -> Result<bool, Error> {
        if inventory::Owner::capture(snapshot)? != self.intent.inventory.owner {
            return Ok(false);
        }
        super::super::eligible_document(snapshot)?;
        if self.intent.title.revalidate(snapshot).is_err() {
            return Ok(false);
        }
        if self.dependent_delete {
            let Some(target) = snapshot.section(self.target_section()) else {
                return Ok(false);
            };
            if target.crdt()? != self.section.crdt()?
                || split::child_tombstone(&snapshot.sections, target, 0).is_err()
            {
                return Ok(false);
            }
        }
        if self.retained_pair_delete {
            let Some(target) = snapshot.section(self.target_section()) else {
                return Ok(false);
            };
            if target.crdt()? != self.section.crdt()?
                || owner_delete::tombstone(&snapshot.sections, target).is_err()
            {
                return Ok(false);
            }
        }
        let mut seen = std::collections::HashSet::new();
        for section in &snapshot.sections {
            if !seen.insert(section.id())
                || section.document_id()? != Some(snapshot.document_id())
                || section.thread_id()? != Some(snapshot.thread_id())
            {
                return Ok(false);
            }
            if section.id() == self.target_section() {
                if section.deleted()?.unwrap_or(false) != self.intent.operation.deleted()
                    || readback_stable(section)? != readback_stable(&self.section)?
                    || !self.target_history_matches(section).unwrap_or(false)
                {
                    return Ok(false);
                }
            } else if self.intent.inventory.sections.get(section.id()) != Some(section) {
                return Ok(false);
            }
        }
        for id in self.intent.inventory.sections.keys() {
            if self.intent.operation.deleted() && id == self.target_section() {
                if self
                    .intent
                    .inventory
                    .get(id)?
                    .crdt()?
                    .is_some_and(|w| w.has(10))
                    && !seen.contains(id.as_str())
                {
                    return Ok(false);
                }
                continue;
            }
            if !seen.contains(id.as_str()) {
                return Ok(false);
            }
        }
        if !self.intent.operation.deleted() && !seen.contains(self.target_section()) {
            return Ok(false);
        }
        // Exact untouched bytes imply the same global neighbors/order; the new
        // section must retain its sealed position and not collide with any of them.
        if let Operation::Insert { position, .. } = &self.intent.operation {
            let inserted = snapshot
                .section(self.target_section())
                .ok_or(Error::Unconfirmed)?;
            let p = position::base(inserted.raw().string(8)?.ok_or(Error::Unconfirmed)?)?;
            if p != position::base(position)? {
                return Ok(false);
            }
            for old in self.intent.inventory.sections.values() {
                if old.raw().string(8)? == Some(p) {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// The direct ACK has already confirmed delivery. A present readback target
    /// separately proves its history; missing/unreadable/replaced CRDT is an
    /// unresolved reconciliation, never permission to bootstrap or replay.
    fn target_history_matches(&self, section: &Section) -> Result<bool, Error> {
        let Some(wire) = section.crdt()? else {
            return Ok(false);
        };
        let acknowledged = self.section.crdt()?.ok_or(Error::Unconfirmed)?;
        if acknowledged.has(10)
            && (wire.has(12) || wire.messages(10)? != acknowledged.messages(10)?)
        {
            return Ok(false);
        }
        let doc = crdt::validate(
            &wire,
            section,
            self.intent.operation.draft(),
            self.intent.operation.deleted(),
            Some(0),
            acknowledged.uint(2)? == Some(0),
        )?;
        if wire.uint(2)? != acknowledged.uint(2)?
            || wire.uint(3)? != section.sequence()?
            || wire.string(8)? != acknowledged.string(8)?
        {
            return Ok(false);
        }
        let mut processed = false;
        for field in wire.fields().iter().filter(|field| field.tag() == 6) {
            let field = RawMessage::decode(wire.encoded_field(field)?)?;
            if field.string(6)? == Some(self.sent_head.as_str()) {
                processed = true;
            }
        }
        if !processed {
            return Ok(false);
        }
        let ack_doc = automerge::Automerge::load(acknowledged.bytes(5)?.ok_or(Error::Unconfirmed)?)
            .map_err(|_| Error::Unconfirmed)?;
        Ok(ack_doc
            .get_changes(&[])
            .iter()
            .all(|change| doc.get_change_by_hash(&change.hash()).is_some()))
    }
}

fn new_content(draft: &TextDraft) -> Result<RawMessage, Error> {
    let text = RawMessage::from_fields(&[F::bytes(1, draft.rtml()?.as_bytes())?])?;
    Ok(RawMessage::from_fields(&[F::message(1, &text)?])?)
}
fn deletion_stable(section: &Section) -> Result<RawMessage, Error> {
    let mut raw = section.raw().clone();
    // Only source-projected fields and documented server receipt metadata may
    // differ. All unknown bytes, content/language/marks/position/style/attrs stay.
    for tag in [2, 3, 9, 13, 18, 19, 20, 33, 38, 103, 104] {
        raw = raw.remove(tag)?;
    }
    if section.section_type()?.unwrap_or(0) != 0
        || section.parents()?.is_some_and(|p| !p.fields().is_empty())
    {
        return Err(Error::Unconfirmed);
    }
    // Partial merge moves present fields to the tail, including these reviewed
    // independent identity fields in a full ACK. Normalize only their placement.
    // Keep every other encoded field in original relative order: unknown tags
    // could carry order-sensitive oneof semantics. Repeated identity occurrences
    // retain their order too; no value or field encoding is rewritten.
    let mut fields = raw.fields().iter().collect::<Vec<_>>();
    fields.sort_by_key(|field| match field.tag() {
        1 | 6 | 7 => field.tag(),
        _ => 0,
    });
    let mut ordered = Vec::new();
    for field in fields {
        ordered.extend_from_slice(raw.encoded_field(field)?);
    }
    Ok(RawMessage::decode(&ordered)?)
}
fn readback_stable(section: &Section) -> Result<RawMessage, Error> {
    let mut raw = section.raw().clone();
    // CRDT38 is validated separately by target_history_matches, including all
    // ACK history and processed sent head. The remaining exclusions are the
    // existing captured ACK/full-load receipt metadata normalization. Unknown
    // fields and local_sequence/revision/order/content remain exact.
    for tag in [18, 38, 103, 104] {
        raw = raw.remove(tag)?;
    }
    if section.parents()?.is_some_and(|p| p.fields().is_empty()) {
        raw = raw.remove(13)?;
    }
    Ok(raw)
}

impl PreparedStructureEdit {
    /// Capture inert credential-free evidence before durable dispatch reservation.
    pub fn checkpoint(&self) -> Result<super::recovery::Checkpoint, Error> {
        super::recovery::Checkpoint::capture(&self.intent, &self.data)
    }
}

pub(super) fn creation_fields(
    id: &str,
    position: &str,
    session: &str,
    draft: &TextDraft,
    wire: &RawMessage,
) -> Result<Vec<F>, Error> {
    let trace = format!("{session}-0");
    Ok(vec![
        F::bytes(1, id.as_bytes())?,
        F::uint(2, 0)?,
        F::uint(3, 0)?,
        F::bytes(8, position.as_bytes())?,
        F::uint(9, 0)?,
        F::uint(10, 0)?,
        F::uint(11, 0)?,
        F::message(12, &new_content(draft)?)?,
        F::message(13, &RawMessage::decode(&[])?)?,
        F::message(16, &RawMessage::from_fields(&[F::uint(4, 0)?])?)?,
        F::uint(19, 1)?,
        F::uint(20, 1)?,
        F::bytes(21, position.as_bytes())?,
        F::uint(29, 0)?,
        F::bytes(33, trace.as_bytes())?,
        F::uint(35, 0)?,
        F::message(38, &wire)?,
    ])
}
