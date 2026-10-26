//! V9 is inert evidence for original text reports plus a new blank before it.
use super::*;
use crate::canvas::edit::structure::{inventory, start_typing::StartTypingWitness};
use std::collections::BTreeMap;

impl Checkpoint {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::canvas::edit::structure) fn start_typing(
        inventory: &Inventory,
        target: &str,
        child: &str,
        position: &str,
        session: &str,
        witness: &StartTypingWitness,
        data: &RawMessage,
    ) -> Result<Self, Error> {
        let owner = &inventory.owner;
        let result = Self {
            version: 9,
            scope: Scope {
                workspace: owner.workspace.clone(),
                user: owner.user.clone(),
                file: owner.file.clone(),
            },
            thread: owner.thread.clone(),
            document: owner.document.clone(),
            target: target.into(),
            session: session.into(),
            sequence: 1,
            action: Action::StartTyping {
                child: child.into(),
                position: position.into(),
                original_updates: witness
                    .original_updates()
                    .iter()
                    .map(TextDraft::rtml)
                    .collect::<Result<Vec<_>, _>>()?,
            },
            draft: witness.original_draft().rtml()?,
            sections: inventory
                .sections
                .values()
                .map(|s| s.raw().raw().to_vec())
                .collect(),
            request: data.raw().to_vec(),
            provenance: inventory.provenance_evidence(),
        };
        result.validate()?;
        Ok(result)
    }
    pub(super) fn validate_start_typing(&self) -> Result<(), Error> {
        let Action::StartTyping {
            child,
            position: inserted_position,
            original_updates,
        } = &self.action
        else {
            return Err(Error::Unsupported);
        };
        if self.version != 9
            || child == &self.target
            || !position::modern_id(child)
            || !inserted_position.ends_with(":temp")
        {
            return Err(Error::Unsupported);
        }
        let before = TextDraft::from_rtml(&self.draft)?;
        let updates = original_updates
            .iter()
            .map(|value| {
                let draft = TextDraft::from_rtml(value)?;
                if draft.rtml()? != *value {
                    return Err(Error::Unsupported);
                }
                Ok(draft)
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let witness = StartTypingWitness::new(&before, &updates)?;
        let sections = self
            .sections
            .iter()
            .map(|raw| {
                let section = Section::decode(raw)?;
                Ok((section.id().to_owned(), section))
            })
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        if sections.contains_key(child) {
            return Err(Error::Unsupported);
        }
        let mut ordered = BTreeMap::new();
        let mut title = None;
        for section in sections.values() {
            if section.raw().uint(11)?.unwrap_or(0) != 0
                || section.raw().boolean(29)? == Some(true)
                || section.raw().has(15)
            {
                return Err(Error::Unsupported);
            }
            inventory::validate_placement(section, &sections)?;
            ordered.insert(section.raw().string(8)?.ok_or(Error::Unsupported)?, section);
            if section.section_type()? == Some(64) || section.style()? == Some(48) {
                if title.replace(section).is_some() {
                    return Err(Error::Unsupported);
                }
            }
        }
        let source = sections.get(&self.target).ok_or(Error::Unsupported)?;
        let source_position = source.raw().string(8)?.ok_or(Error::Unsupported)?;
        let previous = ordered
            .range(..source_position)
            .next_back()
            .map(|(p, _)| *p);
        if position::between(previous, Some(source_position))? != *inserted_position {
            return Err(Error::Changed);
        }
        let data = RawMessage::decode(&self.request)?;
        let title = CanonicalTitle::rich_from_section(title.ok_or(Error::Unsupported)?)?;
        if data.message(14)?.as_ref() != Some(&title) {
            return Err(Error::Changed);
        }
        super::super::start_typing::validate_request(
            source,
            child,
            inserted_position,
            &self.session,
            &witness,
            &data,
        )
    }
}
