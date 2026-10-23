use super::*;

pub struct AcceptedEnd {
    pub(super) intent: Arc<EndIntent>,
    pub(super) parent: Section,
    pub(super) child: Section,
    pub(super) parent_wire: RawMessage,
    pub(super) child_wire: RawMessage,
    pub(super) child_draft: TextDraft,
    pub(super) second: Option<Section>,
    pub(super) second_wire: Option<RawMessage>,
    pub(super) second_draft: Option<TextDraft>,
    pub(super) repeat_typing: bool,
    pub(super) sequence_sections: Vec<Section>,
    pub(super) sequence_rows: Vec<sequence::Member>,
}
impl AcceptedEnd {
    pub fn is_repeat(&self) -> bool {
        self.intent.is_repeat()
    }
    pub fn is_repeat_typing(&self) -> bool {
        self.repeat_typing
    }
    pub fn second_child_section(&self) -> Option<&Section> {
        self.second.as_ref()
    }
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
    pub fn reuses_child(&self) -> bool {
        self.intent.reuses_child()
    }
    pub fn session_id(&self) -> &str {
        &self.intent.session
    }
    pub fn sequence(&self) -> u64 {
        1
    }
    pub fn confirms_readback(&self, snapshot: &CanvasCollabSnapshot) -> Result<bool, Error> {
        if self.is_sequence() {
            return sequence::confirms_readback(self, snapshot);
        }
        if inventory::Owner::capture(snapshot)? != self.intent.inventory.owner
            || super::super::super::eligible_document(snapshot).is_err()
            || self.intent.title.revalidate(snapshot).is_err()
            || snapshot.sections.len()
                != self.intent.inventory.sections.len()
                    + usize::from(!self.reuses_child())
                    + usize::from(self.is_repeat())
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
            let pos = section.raw().string(8)?.ok_or(Error::Unconfirmed)?;
            position::canonical(pos)?;
            if !positions.insert(pos) {
                return Ok(false);
            }
            let pair = if section.id() == self.parent.id() {
                Some((&self.parent, &self.intent.seed, &self.parent_wire))
            } else if section.id() == self.child.id() {
                Some((&self.child, &self.child_draft, &self.child_wire))
            } else if self.second.as_ref().is_some_and(|s| s.id() == section.id()) {
                Some((
                    self.second.as_ref().ok_or(Error::Unconfirmed)?,
                    self.second_draft.as_ref().ok_or(Error::Unconfirmed)?,
                    self.second_wire.as_ref().ok_or(Error::Unconfirmed)?,
                ))
            } else {
                None
            };
            if let Some((acknowledged, draft, sent)) = pair {
                if split::validation::readback_stable(section)?
                    != split::validation::readback_stable(acknowledged)?
                    || !split::validation::content_equal(
                        section,
                        acknowledged
                            .raw()
                            .message(12)?
                            .as_ref()
                            .ok_or(Error::Unconfirmed)?,
                    )?
                    || validation::history(section, draft, sent).is_err()
                {
                    return Ok(false);
                }
            } else if self.intent.inventory.sections.get(section.id()) != Some(section) {
                return Ok(false);
            }
        }
        Ok(seen.contains(self.parent.id())
            && seen.contains(self.child.id())
            && self.second.as_ref().is_none_or(|s| seen.contains(s.id()))
            && self
                .intent
                .inventory
                .sections
                .keys()
                .all(|id| seen.contains(id.as_str())))
    }
}
