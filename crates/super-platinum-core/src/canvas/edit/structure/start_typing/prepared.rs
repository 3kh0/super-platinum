use super::*;
use crate::canvas::wire::EditAck;

pub struct PreparedStartTypingEdit {
    pub(super) intent: Arc<StartTypingIntent>,
    pub(super) witness: StartTypingWitness,
    pub(super) data: RawMessage,
    pub(super) parent_wire: RawMessage,
    pub(super) child_wire: RawMessage,
}
impl PreparedStartTypingEdit {
    pub(super) fn build(
        intent: Arc<StartTypingIntent>,
        witness: StartTypingWitness,
        now: u64,
    ) -> Result<Self, Error> {
        let seed = match intent.original.crdt()? {
            Some(wire) => wire,
            None => text::crdt::bootstrap(&intent.original, &intent.seed, &intent.session, now)?,
        };
        let parent_wire =
            end::typing::build_reports(&seed, true, &witness.updates, &intent.session, now)?;
        let child_wire = crdt::insert(&intent.child, &TextDraft::default(), &intent.session, now)?;
        let data = project(
            &intent.original,
            &intent.child,
            &intent.position,
            &intent.session,
            witness.parent_draft(),
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
    pub fn original_draft(&self) -> &TextDraft {
        self.witness.original_draft()
    }
    pub fn parent_draft(&self) -> &TextDraft {
        self.witness.parent_draft()
    }
    pub fn original_updates(&self) -> &[TextDraft] {
        self.witness.original_updates()
    }
    pub fn checkpoint(&self) -> Result<recovery::Checkpoint, Error> {
        recovery::Checkpoint::start_typing(
            &self.intent.inventory,
            self.target_section(),
            self.child_section(),
            &self.intent.position,
            self.session_id(),
            &self.witness,
            &self.data,
        )
    }
    pub fn validate_snapshot(&self, fresh: &CanvasCollabSnapshot, now: u64) -> Result<(), Error> {
        self.intent.revalidate(fresh, now)
    }
    pub fn validate_ack(&self, received: &EditAck) -> Result<AcceptedStartTyping, Error> {
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
        Ok(AcceptedStartTyping {
            intent: Arc::clone(&self.intent),
            parent: self.intent.original.merge_partial(parent)?,
            child: child.clone(),
            parent_wire: self.parent_wire.clone(),
            child_wire: self.child_wire.clone(),
            parent_draft: self.parent_draft().clone(),
        })
    }
}

pub struct AcceptedStartTyping {
    intent: Arc<StartTypingIntent>,
    parent: Section,
    child: Section,
    parent_wire: RawMessage,
    child_wire: RawMessage,
    parent_draft: TextDraft,
}
impl AcceptedStartTyping {
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
        let empty = TextDraft::default();
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
            let pos = section.raw().string(8)?.ok_or(Error::Unconfirmed)?;
            position::canonical(pos)?;
            if !positions.insert(pos) {
                return Ok(false);
            }
            let pair = if section.id() == self.parent.id() {
                Some((&self.parent, &self.parent_draft, &self.parent_wire))
            } else if section.id() == self.child.id() {
                Some((&self.child, &empty, &self.child_wire))
            } else {
                None
            };
            if let Some((accepted, draft, sent)) = pair {
                if split::validation::readback_stable(section)?
                    != split::validation::readback_stable(accepted)?
                    || end::validation::history(section, draft, sent).is_err()
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

#[allow(clippy::too_many_arguments)]
pub(super) fn project(
    original: &Section,
    child: &str,
    position: &str,
    session: &str,
    final_draft: &TextDraft,
    parent_wire: &RawMessage,
    child_wire: &RawMessage,
    title: &RawMessage,
) -> Result<RawMessage, Error> {
    let parent = RawMessage::from_fields(&[
        F::bytes(1, original.id().as_bytes())?,
        F::uint(2, original.sequence()?.ok_or(Error::Unsupported)?)?,
        F::uint(9, 0)?,
        F::message(12, &text::content(original, final_draft)?)?,
        F::message(13, &RawMessage::decode(&[])?)?,
        F::uint(19, 1)?,
        F::uint(20, 1)?,
        F::bytes(33, format!("{session}-0").as_bytes())?,
        F::message(38, parent_wire)?,
    ])?;
    let child = RawMessage::from_fields(&super::super::prepared::creation_fields(
        child,
        position,
        session,
        &TextDraft::default(),
        child_wire,
    )?)?;
    Ok(RawMessage::from_fields(&[
        F::message(1, &parent)?,
        F::message(1, &child)?,
        F::message(14, title)?,
    ])?)
}
