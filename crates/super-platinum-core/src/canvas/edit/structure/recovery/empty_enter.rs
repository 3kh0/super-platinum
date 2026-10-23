//! v10 is an inert whole-pair receipt for Enter on a canonical literal empty.
//! v11 adds ordered selected-child reports without reinterpreting v10 or v6.
//! Earlier actions retain their original admission rules and bytes.
use super::*;
use crate::canvas::edit::structure::{end as operation, inventory};
use std::collections::BTreeMap;

impl Checkpoint {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::canvas::edit::structure) fn empty_enter(
        inventory: &Inventory,
        target: &str,
        child: &str,
        position: &str,
        session: &str,
        reused: bool,
        data: &RawMessage,
    ) -> Result<Self, Error> {
        Self::capture_empty_pair(
            inventory, target, child, position, session, reused, None, data,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::canvas::edit::structure) fn empty_typing(
        inventory: &Inventory,
        target: &str,
        child: &str,
        position: &str,
        session: &str,
        reused: bool,
        updates: &[TextDraft],
        data: &RawMessage,
    ) -> Result<Self, Error> {
        Self::capture_empty_pair(
            inventory,
            target,
            child,
            position,
            session,
            reused,
            Some(updates),
            data,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn capture_empty_pair(
        inventory: &Inventory,
        target: &str,
        child: &str,
        position: &str,
        session: &str,
        reused: bool,
        updates: Option<&[TextDraft]>,
        data: &RawMessage,
    ) -> Result<Self, Error> {
        let owner = &inventory.owner;
        let result = Self {
            version: if updates.is_some() { 11 } else { 10 },
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
            action: if let Some(updates) = updates {
                Action::EmptyTyping {
                    child: child.into(),
                    position: position.into(),
                    reused,
                    child_updates: updates
                        .iter()
                        .map(TextDraft::rtml)
                        .collect::<Result<Vec<_>, _>>()?,
                }
            } else {
                Action::EmptyEnter {
                    child: child.into(),
                    position: position.into(),
                    reused,
                }
            },
            draft: String::new(),
            sections: inventory
                .sections
                .values()
                .map(|s| s.raw().raw().to_vec())
                .collect(),
            request: data.raw().to_vec(),
        };
        result.validate()?;
        Ok(result)
    }

    pub(super) fn validate_empty_enter(&self) -> Result<(), Error> {
        let (child, inserted_position, reused, witness) = match &self.action {
            Action::EmptyEnter {
                child,
                position,
                reused,
            } if self.version == 10 => (child, position, reused, operation::EndWitness::empty()),
            Action::EmptyTyping {
                child,
                position,
                reused,
                child_updates,
            } if self.version == 11 => {
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
                (
                    child,
                    position,
                    reused,
                    operation::EndWitness::empty().with_child_updates(&updates)?,
                )
            }
            _ => return Err(Error::Unsupported),
        };
        if !self.draft.is_empty() || child == &self.target {
            return Err(Error::Unsupported);
        }
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
        operation::empty::admit(source)?;
        let ordered_rows = ordered.values().copied().collect::<Vec<_>>();
        let selected = operation::empty::candidate(&ordered_rows, &self.target)?;
        if *reused {
            let selected = selected.ok_or(Error::Unsupported)?;
            if selected.id() != child
                || selected.raw().string(8)? != Some(inserted_position.as_str())
            {
                return Err(Error::Unsupported);
            }
        } else {
            if selected.is_some()
                || sections.contains_key(child)
                || !position::modern_id(child)
                || !inserted_position.ends_with(":temp")
            {
                return Err(Error::Unsupported);
            }
            let original_position = source.raw().string(8)?.ok_or(Error::Unsupported)?;
            let next = ordered
                .range::<&str, _>((
                    std::ops::Bound::Excluded(original_position),
                    std::ops::Bound::Unbounded,
                ))
                .next()
                .map(|(p, _)| *p);
            if position::between(Some(original_position), next)? != *inserted_position {
                return Err(Error::Unsupported);
            }
        }
        let data = RawMessage::decode(&self.request)?;
        let expected_title = CanonicalTitle::rich_from_section(title.ok_or(Error::Unsupported)?)?;
        if data.message(14)?.as_ref() != Some(&expected_title) {
            return Err(Error::Unsupported);
        }
        if witness.is_empty_typing() {
            return operation::typing::validate_empty(
                source,
                selected,
                child,
                inserted_position,
                &self.session,
                &witness,
                &data,
            );
        }
        operation::empty::validate(
            source,
            selected,
            child,
            inserted_position,
            &self.session,
            &witness,
            &data,
        )
    }
}
