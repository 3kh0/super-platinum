use super::*;
use crate::canvas::wire::EditAck;

pub struct PreparedSplitEdit {
    pub(super) intent: Arc<SplitIntent>,
    pub(super) witness: SplitWitness,
    pub(super) data: RawMessage,
    pub(super) parent_wire: RawMessage,
    pub(super) child_wire: RawMessage,
    pub(super) lineage: RawMessage,
    expiry: Option<u64>,
}
impl PreparedSplitEdit {
    pub(super) fn build(
        intent: Arc<SplitIntent>,
        witness: SplitWitness,
        now: u64,
    ) -> Result<Self, Error> {
        let seed = match intent.original.crdt()? {
            Some(wire) => wire,
            None => text::crdt::bootstrap(&intent.original, &intent.seed, &intent.session, now)?,
        };
        let doc = text::crdt::validate(&seed, &intent.original, &intent.seed, None)?;
        let heads = doc.get_heads();
        if heads.len() != 1 || !validation::processed(&seed, &heads[0].to_string())? {
            return Err(Error::Unsupported);
        }
        let lineage = RawMessage::from_fields(&[
            F::uint(1, 0)?,
            F::bytes(2, format!("{}00000000", intent.session).as_bytes())?,
            F::bytes(3, intent.child.as_bytes())?,
            // Original getContent().length is serialized RTML UTF16 length.
            F::uint(4, witness.parent.rtml()?.encode_utf16().count() as u64)?,
            F::bytes(
                5,
                serde_json::to_string(&heads.iter().map(ToString::to_string).collect::<Vec<_>>())
                    .map_err(|_| Error::Unsupported)?
                    .as_bytes(),
            )?,
            F::bytes(
                6,
                format!(
                    "{}-{}",
                    seed.uint(2)?.ok_or(Error::Unsupported)?,
                    seed.string(8)?.ok_or(Error::Unsupported)?
                )
                .as_bytes(),
            )?,
            F::bytes(7, &doc.save())?,
        ])?;
        let parent_wire = text::crdt::update(
            &seed,
            &intent.original,
            &intent.seed,
            &witness.parent,
            &intent.session,
            now,
        )?
        .replace_fields(10, &[F::message(10, &lineage)?])?;
        // Original reportChange queues split head + previously processed heads;
        // FOR_SERVER appends that queue to the existing processed list.
        let mut fields = Vec::new();
        for wire in [&parent_wire, &seed] {
            for f in wire.fields().iter().filter(|f| f.tag() == 6) {
                fields.push(F::bytes(
                    6,
                    RawMessage::decode(wire.encoded_field(f)?)?
                        .bytes(6)?
                        .ok_or(Error::Unsupported)?,
                )?);
            }
        }
        let parent_wire = parent_wire.replace_fields(6, &fields)?;
        let child_seed = crdt::insert(&intent.child, &witness.child, &intent.session, now)?;
        let child_wire = if witness.updates.is_empty() {
            child_seed
        } else {
            super::super::end::typing::build_reports(
                &child_seed,
                false,
                &witness.updates,
                &intent.session,
                now,
            )?
        };
        let data = project(
            &intent.original,
            &intent.child,
            &intent.position,
            &intent.session,
            &witness,
            &parent_wire,
            &child_wire,
            intent.title.rich_text(),
        )?;
        Ok(Self {
            intent,
            witness,
            data,
            parent_wire,
            child_wire,
            lineage,
            expiry: seed.uint(4)?,
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
        self.intent.original.id()
    }
    pub fn child_section(&self) -> &str {
        &self.intent.child
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
    pub fn initial_child_draft(&self) -> &TextDraft {
        self.witness.initial_child_draft()
    }
    pub fn child_draft(&self) -> &TextDraft {
        self.witness.child_draft()
    }
    pub fn child_updates(&self) -> &[TextDraft] {
        self.witness.child_updates()
    }
    pub fn checkpoint(&self) -> Result<recovery::Checkpoint, Error> {
        recovery::Checkpoint::split(
            &self.intent.inventory,
            self.target_section(),
            self.child_section(),
            &self.intent.position,
            self.session_id(),
            &self.witness.before,
            &self.witness.parent,
            &self.witness.child,
            self.witness.offset,
            &self.witness.updates,
            &self.data,
        )
    }
    pub fn validate_snapshot(&self, fresh: &CanvasCollabSnapshot, now: u64) -> Result<(), Error> {
        self.intent.revalidate(fresh, now)?;
        if self.expiry.is_some_and(|expires| expires <= now) {
            return Err(Error::Expired);
        }
        Ok(())
    }
    pub fn validate_ack(&self, received: &EditAck) -> Result<AcceptedSplit, Error> {
        // Decode actual response bytes; mutable public projections are not proof.
        let ack = EditAck::decode(received.raw().raw())?;
        if ack.changed != Some(true)
            || ack.is_new_edit_session == Some(true)
            || ack
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
            || data
                .document_id()?
                .is_some_and(|id| id != self.document_id())
            || data.sections().len() != 2
        {
            return Err(Error::Unconfirmed);
        }
        let parent = data
            .sections()
            .iter()
            .find(|s| s.id() == self.target_section())
            .ok_or(Error::Unconfirmed)?;
        let child = data
            .sections()
            .iter()
            .find(|s| s.id() == self.child_section())
            .ok_or(Error::Unconfirmed)?;
        validation::parent(self, parent)?;
        validation::child(self, child)?;
        Ok(AcceptedSplit {
            intent: Arc::clone(&self.intent),
            witness: self.witness.clone(),
            parent: self.intent.original.merge_partial(parent)?,
            child: child.clone(),
            parent_wire: self.parent_wire.clone(),
            child_wire: self.child_wire.clone(),
            lineage: self.lineage.clone(),
        })
    }
}

pub struct AcceptedSplit {
    intent: Arc<SplitIntent>,
    witness: SplitWitness,
    parent: Section,
    child: Section,
    parent_wire: RawMessage,
    child_wire: RawMessage,
    lineage: RawMessage,
}
impl AcceptedSplit {
    pub fn parent_section(&self) -> &Section {
        &self.parent
    }
    pub fn child_section(&self) -> &Section {
        &self.child
    }
    pub fn target_section(&self) -> &str {
        self.parent.id()
    }
    pub fn child_id(&self) -> &str {
        self.child.id()
    }
    pub fn session_id(&self) -> &str {
        &self.intent.session
    }
    pub fn sequence(&self) -> u64 {
        1
    }
    pub fn confirms_readback(&self, snapshot: &CanvasCollabSnapshot) -> Result<bool, Error> {
        if inventory::Owner::capture(snapshot)? != self.intent.inventory.owner
            || super::super::super::eligible_document(snapshot).is_err()
            || self.intent.title.revalidate(snapshot).is_err()
            || snapshot.sections.len() != self.intent.inventory.sections.len() + 1
        {
            return Ok(false);
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut positions = std::collections::BTreeSet::new();
        let mut bytes = 0usize;
        for section in &snapshot.sections {
            bytes = bytes
                .checked_add(section.raw().raw().len())
                .ok_or(Error::Unsupported)?;
            if !seen.insert(section.id())
                || bytes > crate::canvas::wire::MAX_BYTES
                || section.document_id()? != Some(self.intent.inventory.owner.document.as_str())
                || section.thread_id()? != Some(self.intent.inventory.owner.thread.as_str())
            {
                return Ok(false);
            }
            let p = section.raw().string(8)?.ok_or(Error::Unconfirmed)?;
            position::canonical(p)?;
            if !positions.insert(p) {
                return Ok(false);
            }
            let target = if section.id() == self.parent.id() {
                Some((
                    &self.parent,
                    &self.witness.parent,
                    &self.parent_wire,
                    Some(&self.lineage),
                ))
            } else if section.id() == self.child.id() {
                Some((
                    &self.child,
                    self.witness.child_draft(),
                    &self.child_wire,
                    None,
                ))
            } else {
                None
            };
            if let Some((acknowledged, draft, sent, lineage)) = target {
                if validation::readback_stable(section)?
                    != validation::readback_stable(acknowledged)?
                    || !validation::content_equal(
                        section,
                        acknowledged
                            .raw()
                            .message(12)?
                            .as_ref()
                            .ok_or(Error::Unconfirmed)?,
                    )?
                    || validation::history(section, draft, sent, lineage).is_err()
                    || !validation::contains_history(
                        section,
                        &acknowledged.crdt()?.ok_or(Error::Unconfirmed)?,
                    )?
                {
                    return Ok(false);
                }
            } else if self.intent.inventory.sections.get(section.id()) != Some(section) {
                return Ok(false);
            }
        }
        Ok(seen.contains(self.parent.id())
            && seen.contains(self.child.id())
            && self
                .intent
                .inventory
                .sections
                .keys()
                .all(|id| seen.contains(id.as_str())))
    }
}

pub(super) fn project(
    original: &Section,
    child_id: &str,
    position: &str,
    session: &str,
    witness: &SplitWitness,
    parent_wire: &RawMessage,
    child_wire: &RawMessage,
    title: &RawMessage,
) -> Result<RawMessage, Error> {
    let trace = format!("{}-0", session);
    let empty = RawMessage::decode(&[])?;
    let parent = RawMessage::from_fields(&[
        F::bytes(1, original.id().as_bytes())?,
        F::uint(2, original.sequence()?.ok_or(Error::Unsupported)?)?,
        F::uint(9, 0)?,
        F::message(12, &text::content(original, &witness.parent)?)?,
        F::message(13, &empty)?,
        F::uint(19, 1)?,
        F::uint(20, 1)?,
        F::bytes(33, trace.as_bytes())?,
        F::message(38, parent_wire)?,
    ])?;
    let child = RawMessage::from_fields(&[
        F::bytes(1, child_id.as_bytes())?,
        F::uint(2, 0)?,
        F::uint(3, 0)?,
        F::bytes(8, position.as_bytes())?,
        F::uint(9, 0)?,
        F::uint(10, 0)?,
        F::uint(11, 0)?,
        F::message(12, &validation::child_content(witness.child_draft())?)?,
        F::message(13, &empty)?,
        F::message(16, &RawMessage::from_fields(&[F::uint(4, 0)?])?)?,
        F::uint(19, 1)?,
        F::uint(20, 1)?,
        F::bytes(21, position.as_bytes())?,
        F::uint(29, 0)?,
        F::bytes(33, trace.as_bytes())?,
        F::uint(35, 0)?,
        F::message(38, child_wire)?,
    ])?;
    let data = RawMessage::from_fields(&[
        F::message(1, &parent)?,
        F::message(1, &child)?,
        F::message(14, title)?,
    ])?;
    Ok(data)
}
