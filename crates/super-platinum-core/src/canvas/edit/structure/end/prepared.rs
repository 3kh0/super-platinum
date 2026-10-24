use super::*;
use crate::canvas::wire::EditAck;

pub struct PreparedEndEdit {
    pub(super) intent: Arc<EndIntent>,
    pub(super) witness: EndWitness,
    pub(super) data: RawMessage,
    pub(super) parent_wire: RawMessage,
    pub(super) child_wire: RawMessage,
    pub(super) child_draft: TextDraft,
    pub(super) second_draft: Option<TextDraft>,
    pub(super) second_wire: Option<RawMessage>,
    pub(super) expiry: Option<u64>,
    pub(super) sequence_rows: Vec<sequence::Member>,
}
impl PreparedEndEdit {
    pub(super) fn build(
        intent: Arc<EndIntent>,
        witness: EndWitness,
        now: u64,
    ) -> Result<Self, Error> {
        if intent.is_sequence() {
            return sequence::build(intent, witness, now);
        }
        let seed = match intent.original.crdt()? {
            Some(wire) => wire,
            None => text::crdt::bootstrap(&intent.original, &intent.seed, &intent.session, now)?,
        };
        // Original updateText on unchanged content is a no-op. Session.toPb
        // omits expiry; no queued snapshot means no current_head12 is appended.
        let parent_wire = validation::project_parent_session(&seed)?;
        let child_wire = if let Some(reused) = intent.reused() {
            {
                let seed = match reused.crdt()? {
                    Some(wire) => wire,
                    None if intent.empty_enter || intent.is_repeat() => {
                        text::crdt::bootstrap(reused, &TextDraft::default(), &intent.session, now)?
                    }
                    None => return Err(Error::Unsupported),
                };
                validation::project_parent_session(&seed)?
            }
        } else {
            crdt::insert(&intent.child, &TextDraft::default(), &intent.session, now)?
        };
        let child_draft = witness.child_draft();
        let child_wire = if intent.clears_child() {
            let reports =
                clearing::reports(intent.reused().ok_or(Error::Unsupported)?, &witness.updates)?;
            typing::build_reports(&child_wire, true, &reports, &intent.session, now)?
        } else if witness.updates.is_empty() {
            child_wire
        } else {
            typing::build(
                &child_wire,
                intent.reuses_child(),
                &witness.updates,
                &intent.session,
                now,
            )?
        };
        let second_wire = intent
            .second
            .as_ref()
            .map(|second| crdt::insert(&second.id, &TextDraft::default(), &intent.session, now))
            .transpose()?;
        let second_wire = if witness.is_repeat_typing() {
            Some(typing::build(
                second_wire.as_ref().ok_or(Error::Unsupported)?,
                false,
                &witness.second_updates,
                &intent.session,
                now,
            )?)
        } else {
            second_wire
        };
        let second_draft = intent
            .second
            .as_ref()
            .map(|_| witness.second_updates.last().cloned().unwrap_or_default());
        let data = if let Some(second) = &intent.second {
            repeat_typing::prepared_data(
                &intent,
                &witness,
                &parent_wire,
                &child_wire,
                second,
                second_wire.as_ref().ok_or(Error::Unsupported)?,
            )?
        } else if intent.empty_enter && witness.updates.is_empty() {
            let data = if let Some(reused) = intent.reused() {
                project_reuse(
                    &intent.original,
                    reused,
                    &intent.session,
                    &parent_wire,
                    &child_wire,
                    intent.title.rich_text(),
                )?
            } else {
                project(
                    &intent.original,
                    &intent.child,
                    &intent.position,
                    &intent.session,
                    &parent_wire,
                    &child_wire,
                    intent.title.rich_text(),
                )?
            };
            empty::validate(
                &intent.original,
                intent.reused(),
                &intent.child,
                &intent.position,
                &intent.session,
                &witness,
                &data,
            )?;
            data
        } else if intent.clears_child() || !witness.updates.is_empty() {
            let data = typing::project(
                &intent.original,
                intent.reused(),
                &intent.child,
                &intent.position,
                &intent.session,
                &parent_wire,
                &child_wire,
                intent.title.rich_text(),
                &child_draft,
            )?;
            if intent.empty_enter {
                typing::validate_empty(
                    &intent.original,
                    intent.reused(),
                    &intent.child,
                    &intent.position,
                    &intent.session,
                    &witness,
                    &data,
                )?;
            } else if intent.clears_child() {
                typing::validate_clearing(
                    &intent.original,
                    intent.reused().ok_or(Error::Unsupported)?,
                    &intent.child,
                    &intent.position,
                    &intent.session,
                    &witness,
                    &data,
                )?;
            } else {
                typing::validate(
                    &intent.original,
                    intent.reused(),
                    &intent.child,
                    &intent.position,
                    &intent.session,
                    &witness,
                    &data,
                )?;
            }
            data
        } else if let Some(reused) = intent.reused() {
            let data = project_reuse(
                &intent.original,
                reused,
                &intent.session,
                &parent_wire,
                &child_wire,
                intent.title.rich_text(),
            )?;
            validate_checkpoint_reuse(&intent.original, reused, &intent.session, &witness, &data)?;
            data
        } else {
            let data = project(
                &intent.original,
                &intent.child,
                &intent.position,
                &intent.session,
                &parent_wire,
                &child_wire,
                intent.title.rich_text(),
            )?;
            validate_checkpoint_end(
                &intent.original,
                &intent.child,
                &intent.position,
                &intent.session,
                &witness,
                &data,
            )?;
            data
        };
        let child_expiry = intent
            .reused()
            .map(|s| {
                s.crdt()?
                    .map(|w| w.uint(4))
                    .transpose()
                    .map(Option::flatten)
                    .map_err(Error::from)
            })
            .transpose()?
            .flatten();
        let expiry = [seed.uint(4)?, child_expiry].into_iter().flatten().min();
        Ok(Self {
            intent,
            witness,
            data,
            parent_wire,
            child_wire,
            child_draft,
            second_wire,
            second_draft,
            expiry,
            sequence_rows: Vec::new(),
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
    pub fn is_repeat(&self) -> bool {
        self.intent.is_repeat()
    }
    pub fn second_child_section(&self) -> Option<&str> {
        self.intent.second.as_ref().map(|s| s.id.as_str())
    }
    pub fn second_child_draft(&self) -> Option<&TextDraft> {
        self.second_draft.as_ref()
    }
    pub fn is_repeat_typing(&self) -> bool {
        self.witness.is_repeat_typing()
    }
    pub fn second_child_updates(&self) -> &[TextDraft] {
        &self.witness.second_updates
    }
    pub fn is_empty_enter(&self) -> bool {
        self.intent.empty_enter
    }
    pub fn is_empty_typing(&self) -> bool {
        self.witness.is_empty_typing()
    }
    pub fn reuses_child(&self) -> bool {
        self.intent.reuses_child()
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
    pub fn child_draft(&self) -> &TextDraft {
        &self.child_draft
    }
    pub fn clears_child(&self) -> bool {
        self.intent.clears_child()
    }
    pub fn child_updates(&self) -> &[TextDraft] {
        self.witness
            .sequence_updates()
            .map_or(self.witness.updates.as_slice(), |stages| {
                stages[0].as_slice()
            })
    }
    pub fn parent_draft(&self) -> &TextDraft {
        self.witness
            .parent_updates
            .last()
            .unwrap_or(&self.intent.seed)
    }
    pub fn parent_updates(&self) -> &[TextDraft] {
        self.witness.parent_updates()
    }
    pub fn checkpoint(&self) -> Result<recovery::Checkpoint, Error> {
        if self.is_sequence() {
            return recovery::Checkpoint::end_sequence(
                &self.intent.inventory,
                self.target_section(),
                self.session_id(),
                &self.witness.before,
                &self.witness.parent_updates,
                &self.sequence_rows,
                &self.data,
            );
        }
        if let Some(second) = &self.intent.second {
            return recovery::Checkpoint::end_repeat(
                &self.intent.inventory,
                self.target_section(),
                self.child_section(),
                &self.intent.position,
                self.session_id(),
                self.reuses_child(),
                &second.id,
                &second.position,
                &self.witness.before,
                &self.witness.second_updates,
                &self.data,
            );
        }
        if self.is_empty_typing() {
            return recovery::Checkpoint::empty_typing(
                &self.intent.inventory,
                self.target_section(),
                self.child_section(),
                &self.intent.position,
                self.session_id(),
                self.reuses_child(),
                &self.witness.updates,
                &self.data,
            );
        }
        if self.is_empty_enter() {
            return recovery::Checkpoint::empty_enter(
                &self.intent.inventory,
                self.target_section(),
                self.child_section(),
                &self.intent.position,
                self.session_id(),
                self.reuses_child(),
                &self.data,
            );
        }
        if self.clears_child() {
            return recovery::Checkpoint::end_clearing(
                &self.intent.inventory,
                self.target_section(),
                self.child_section(),
                &self.intent.position,
                self.session_id(),
                &self.witness.before,
                self.witness.offset,
                &self.witness.updates,
                &self.data,
            );
        }
        if !self.witness.updates.is_empty() {
            return recovery::Checkpoint::end_typing(
                &self.intent.inventory,
                self.target_section(),
                self.child_section(),
                &self.intent.position,
                self.session_id(),
                &self.witness.before,
                self.witness.offset,
                self.reuses_child(),
                &self.witness.updates,
                &self.data,
            );
        }
        if self.reuses_child() {
            return recovery::Checkpoint::end_reuse(
                &self.intent.inventory,
                self.target_section(),
                self.child_section(),
                &self.intent.position,
                self.session_id(),
                &self.witness.before,
                self.witness.offset,
                &self.data,
            );
        }
        recovery::Checkpoint::end(
            &self.intent.inventory,
            self.target_section(),
            self.child_section(),
            &self.intent.position,
            self.session_id(),
            &self.witness.before,
            self.witness.offset,
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
    pub fn validate_ack(&self, received: &EditAck) -> Result<AcceptedEnd, Error> {
        if self.is_sequence() {
            return sequence::validate_ack(self, received);
        }
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
            || data.sections().len() != 2 + usize::from(self.is_repeat())
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
        let second = if let Some(second) = &self.intent.second {
            let section = data
                .sections()
                .iter()
                .find(|s| s.id() == second.id)
                .ok_or(Error::Unconfirmed)?;
            validation::new_child(
                self,
                section,
                &second.position,
                self.second_draft.as_ref().ok_or(Error::Unconfirmed)?,
                self.second_wire.as_ref().ok_or(Error::Unconfirmed)?,
            )?;
            Some(section.clone())
        } else {
            None
        };
        Ok(AcceptedEnd {
            intent: Arc::clone(&self.intent),
            parent: self.intent.original.merge_partial(parent)?,
            child: self
                .intent
                .reused()
                .map_or_else(|| Ok(child.clone()), |old| old.merge_partial(child))?,
            parent_wire: self.parent_wire.clone(),
            parent_draft: self.parent_draft().clone(),
            child_wire: self.child_wire.clone(),
            child_draft: self.child_draft.clone(),
            second,
            second_wire: self.second_wire.clone(),
            second_draft: self.second_draft.clone(),
            repeat_typing: self.is_repeat_typing(),
            sequence_sections: Vec::new(),
            sequence_rows: Vec::new(),
        })
    }
}

pub(super) fn project(
    original: &Section,
    child: &str,
    position: &str,
    session: &str,
    parent_wire: &RawMessage,
    child_wire: &RawMessage,
    title: &RawMessage,
) -> Result<RawMessage, Error> {
    let parent = metadata(original, session, parent_wire)?;
    let child = new_row(child, position, session, child_wire, &TextDraft::default())?;
    Ok(RawMessage::from_fields(&[
        F::message(1, &parent)?,
        F::message(1, &child)?,
        F::message(14, title)?,
    ])?)
}
pub(super) fn new_row(
    child: &str,
    position: &str,
    session: &str,
    child_wire: &RawMessage,
    draft: &TextDraft,
) -> Result<RawMessage, Error> {
    let trace = format!("{session}-0");
    let empty = RawMessage::decode(&[])?;
    let child = RawMessage::from_fields(&[
        F::bytes(1, child.as_bytes())?,
        F::uint(2, 0)?,
        F::uint(3, 0)?,
        F::bytes(8, position.as_bytes())?,
        F::uint(9, 0)?,
        F::uint(10, 0)?,
        F::uint(11, 0)?,
        F::message(12, &split::validation::child_content(draft)?)?,
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
    Ok(child)
}

pub(super) fn metadata(
    original: &Section,
    session: &str,
    wire: &RawMessage,
) -> Result<RawMessage, Error> {
    Ok(RawMessage::from_fields(&[
        F::bytes(1, original.id().as_bytes())?,
        F::uint(2, original.sequence()?.ok_or(Error::Unsupported)?)?,
        F::message(13, &RawMessage::decode(&[])?)?,
        F::uint(19, 1)?,
        F::uint(20, 1)?,
        F::bytes(33, format!("{session}-0").as_bytes())?,
        F::message(38, wire)?,
    ])?)
}
pub(super) fn project_reuse(
    parent: &Section,
    child: &Section,
    session: &str,
    parent_wire: &RawMessage,
    child_wire: &RawMessage,
    title: &RawMessage,
) -> Result<RawMessage, Error> {
    Ok(RawMessage::from_fields(&[
        F::message(1, &metadata(parent, session, parent_wire)?)?,
        F::message(1, &metadata(child, session, child_wire)?)?,
        F::message(14, title)?,
    ])?)
}
