//! v12/v13 retain the complete three-row attempt; validation grants no replay authority.
use super::*;
use crate::canvas::edit::structure::{end as operation, inventory};
use std::collections::BTreeMap;

impl Checkpoint {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::canvas::edit::structure) fn end_repeat(
        inventory: &Inventory,
        target: &str,
        child: &str,
        position: &str,
        session: &str,
        reused: bool,
        second_child: &str,
        second_position: &str,
        before: &TextDraft,
        second_child_updates: &[TextDraft],
        data: &RawMessage,
    ) -> Result<Self, Error> {
        let owner = &inventory.owner;
        let result = Self {
            version: if second_child_updates.is_empty() {
                12
            } else {
                13
            },
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
            action: if second_child_updates.is_empty() {
                Action::EndRepeat {
                    child: child.into(),
                    position: position.into(),
                    reused,
                    second_child: second_child.into(),
                    second_position: second_position.into(),
                }
            } else {
                Action::EndRepeatTyping {
                    child: child.into(),
                    position: position.into(),
                    reused,
                    second_child: second_child.into(),
                    second_position: second_position.into(),
                    second_child_updates: second_child_updates
                        .iter()
                        .map(TextDraft::rtml)
                        .collect::<Result<Vec<_>, _>>()?,
                }
            },
            draft: before.rtml()?,
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

    pub(super) fn validate_end_repeat(&self) -> Result<(), Error> {
        let (child, first_position, reused, second_child, second_position, reports) =
            match &self.action {
                Action::EndRepeat {
                    child,
                    position,
                    reused,
                    second_child,
                    second_position,
                } => (child, position, reused, second_child, second_position, None),
                Action::EndRepeatTyping {
                    child,
                    position,
                    reused,
                    second_child,
                    second_position,
                    second_child_updates,
                } => (
                    child,
                    position,
                    reused,
                    second_child,
                    second_position,
                    Some(second_child_updates),
                ),
                _ => return Err(Error::Unsupported),
            };
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
        let original = sections.get(&self.target).ok_or(Error::Unsupported)?;
        let before = TextDraft::from_rtml(&self.draft)?;
        operation::repeat::admit(original, &before)?;
        let ordered_rows = ordered.values().copied().collect::<Vec<_>>();
        let selected = operation::empty::candidate(&ordered_rows, &self.target)?;
        if *reused {
            let selected = selected.ok_or(Error::Unsupported)?;
            if selected.id() != child || selected.raw().string(8)? != Some(first_position.as_str())
            {
                return Err(Error::Unsupported);
            }
        } else {
            if selected.is_some()
                || sections.contains_key(child)
                || !position::modern_id(child)
                || !first_position.ends_with(":temp")
            {
                return Err(Error::Unsupported);
            }
            let original_position = original.raw().string(8)?.ok_or(Error::Unsupported)?;
            let next = ordered.keys().copied().find(|p| *p > original_position);
            if position::between(Some(original_position), next)? != *first_position {
                return Err(Error::Unsupported);
            }
        }
        if sections.contains_key(second_child)
            || child == second_child
            || !position::modern_id(second_child)
            || !second_position.ends_with(":temp")
        {
            return Err(Error::Unsupported);
        }
        let first_base = position::base(first_position)?;
        let next = ordered.keys().copied().find(|p| *p > first_base);
        if position::between(Some(first_base), next)? != *second_position {
            return Err(Error::Unsupported);
        }
        let data = RawMessage::decode(&self.request)?;
        let expected_title = CanonicalTitle::rich_from_section(title.ok_or(Error::Unsupported)?)?;
        if data.message(14)?.as_ref() != Some(&expected_title) {
            return Err(Error::Unsupported);
        }
        if let Some(reports) = reports {
            let drafts = reports
                .iter()
                .map(|value| {
                    let draft = TextDraft::from_rtml(value)?;
                    if draft.rtml()? != *value {
                        return Err(Error::Unsupported);
                    }
                    Ok(draft)
                })
                .collect::<Result<Vec<_>, Error>>()?;
            operation::repeat_typing::validate(
                original,
                selected,
                child,
                first_position,
                second_child,
                second_position,
                &self.session,
                &before,
                &drafts,
                &data,
            )
        } else {
            operation::repeat::validate(
                original,
                selected,
                child,
                first_position,
                second_child,
                second_position,
                &self.session,
                &before,
                &data,
            )
        }
    }
}
