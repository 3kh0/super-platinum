//! V7 records mandatory source clearing followed by optional child reports.
use super::*;
use crate::canvas::edit::structure::{end as operation, inventory};
use std::collections::BTreeMap;

impl Checkpoint {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::canvas::edit::structure) fn end_clearing(
        inventory: &Inventory,
        target: &str,
        child: &str,
        position: &str,
        session: &str,
        before: &TextDraft,
        offset_utf16: usize,
        updates: &[TextDraft],
        data: &RawMessage,
    ) -> Result<Self, Error> {
        let owner = &inventory.owner;
        let result = Self {
            version: 7,
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
            action: Action::EndClearing {
                child: child.into(),
                position: position.into(),
                offset_utf16,
                child_updates: updates
                    .iter()
                    .map(TextDraft::rtml)
                    .collect::<Result<Vec<_>, _>>()?,
            },
            draft: before.rtml()?,
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

    pub(super) fn validate_end_clearing(&self) -> Result<(), Error> {
        let Action::EndClearing {
            child,
            position: inserted_position,
            offset_utf16,
            child_updates,
        } = &self.action
        else {
            return Err(Error::Unsupported);
        };
        if self.version != 7 || child == &self.target {
            return Err(Error::Unsupported);
        }
        let before = TextDraft::from_rtml(&self.draft)?;
        let updates = child_updates
            .iter()
            .map(|value| {
                let draft = TextDraft::from_rtml(value)?;
                if draft.rtml()? != *value {
                    return Err(Error::Unsupported);
                }
                Ok(draft)
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let witness =
            operation::EndWitness::new(&before, *offset_utf16, &before, &TextDraft::default())?
                .with_child_updates(&updates)?;
        let sections = self
            .sections
            .iter()
            .map(|raw| {
                let section = Section::decode(raw)?;
                Ok((section.id().to_owned(), section))
            })
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
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
        let rows = ordered.values().copied().collect::<Vec<_>>();
        let reused_source = operation::clearing::candidate(&rows, &self.target)?;
        if reused_source.id() != child
            || reused_source.raw().string(8)? != Some(inserted_position.as_str())
        {
            return Err(Error::Unsupported);
        }
        let data = RawMessage::decode(&self.request)?;
        let title = CanonicalTitle::rich_from_section(title.ok_or(Error::Unsupported)?)?;
        if data.message(14)?.as_ref() != Some(&title) {
            return Err(Error::Unsupported);
        }
        operation::typing::validate_clearing(
            source,
            reused_source,
            child,
            inserted_position,
            &self.session,
            &witness,
            &data,
        )
    }
}
