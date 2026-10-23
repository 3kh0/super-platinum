//! A v3/v8 split checkpoint is one inert pair. Decoding never recreates an intent,
//! request authority; the full original request remains byte-preserved.
use super::*;
use crate::canvas::edit::structure::{inventory, split as operation};
use std::collections::BTreeMap;

impl Checkpoint {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::canvas::edit::structure) fn split(
        inventory: &Inventory,
        target: &str,
        child: &str,
        position: &str,
        session: &str,
        before: &TextDraft,
        parent: &TextDraft,
        child_draft: &TextDraft,
        offset_utf16: usize,
        updates: &[TextDraft],
        data: &RawMessage,
    ) -> Result<Self, Error> {
        let owner = &inventory.owner;
        let result = Self {
            version: if updates.is_empty() { 3 } else { 8 },
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
            action: if updates.is_empty() {
                Action::Split {
                    child: child.into(),
                    position: position.into(),
                    offset_utf16,
                    parent_draft: parent.rtml()?,
                    child_draft: child_draft.rtml()?,
                }
            } else {
                Action::SplitTyping {
                    child: child.into(),
                    position: position.into(),
                    offset_utf16,
                    parent_draft: parent.rtml()?,
                    child_draft: child_draft.rtml()?,
                    child_updates: updates
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

    pub(super) fn validate_split(&self) -> Result<(), Error> {
        let (child, inserted_position, offset_utf16, parent_draft, child_draft, reports) =
            match &self.action {
                Action::Split {
                    child,
                    position,
                    offset_utf16,
                    parent_draft,
                    child_draft,
                } if self.version == 3 => (
                    child,
                    position,
                    offset_utf16,
                    parent_draft,
                    child_draft,
                    &[][..],
                ),
                Action::SplitTyping {
                    child,
                    position,
                    offset_utf16,
                    parent_draft,
                    child_draft,
                    child_updates,
                } if self.version == 8 && !child_updates.is_empty() => (
                    child,
                    position,
                    offset_utf16,
                    parent_draft,
                    child_draft,
                    child_updates.as_slice(),
                ),
                _ => return Err(Error::Unsupported),
            };
        if child == &self.target
            || !position::modern_id(child)
            || !inserted_position.ends_with(":temp")
        {
            return Err(Error::Unsupported);
        }
        let updates = reports
            .iter()
            .map(|value| {
                let draft = TextDraft::from_rtml(value)?;
                if draft.rtml()? != *value {
                    return Err(Error::Unsupported);
                }
                Ok(draft)
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let before = TextDraft::from_rtml(&self.draft)?;
        let parent = TextDraft::from_rtml(parent_draft)?;
        let tail = TextDraft::from_rtml(child_draft)?;
        if parent.rtml()? != *parent_draft || tail.rtml()? != *child_draft {
            return Err(Error::Unsupported);
        }
        // Logical consistency only. This does not recreate the renderer's
        // original input/range witness or grant a second dispatch.
        let witness = operation::SplitWitness::new(&before, *offset_utf16, &parent, &tail)?
            .with_child_updates(&updates)?;
        let sections = self
            .sections
            .iter()
            .map(|raw| {
                let s = Section::decode(raw)?;
                Ok((s.id().to_owned(), s))
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
        if source.section_type()?.unwrap_or(0) != 0
            || source.style()?.unwrap_or(0) != 0
            || source.parents()?.is_some_and(|p| !p.fields().is_empty())
            || source.read_only()? == Some(true)
            || source.attributes()?.is_some_and(|a| a.has(29))
            || text::canonical_draft(source)? != before
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
        // Match the operation's new-child cohort, never reinterpret trailing
        // empty reuse as a persisted independent insertion.
        if let Some((_, last)) = ordered
            .last_key_value()
            .filter(|(_, s)| s.id() != self.target)
        {
            if last.section_type()?.unwrap_or(0) == 0
                && last.style()?.unwrap_or(0) == 0
                && last.parents()?.is_none_or(|p| p.fields().is_empty())
                && text::canonical_draft(last).is_ok_and(|d| d.runs.is_empty())
            {
                return Err(Error::Unsupported);
            }
        }
        let data = RawMessage::decode(&self.request)?;
        let expected_title = CanonicalTitle::rich_from_section(title.ok_or(Error::Unsupported)?)?;
        if data.message(14)?.as_ref() != Some(&expected_title) {
            return Err(Error::Unsupported);
        }
        operation::validate_checkpoint_split(
            source,
            child,
            inserted_position,
            &self.session,
            &witness,
            &data,
        )
    }
}

#[cfg(test)]
mod tests;
