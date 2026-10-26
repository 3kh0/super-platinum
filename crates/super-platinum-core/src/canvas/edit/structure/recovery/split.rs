//! A split checkpoint is one inert pair with versioned ordered report evidence. Decoding never recreates an intent,
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
        enter: &TextDraft,
        pre_updates: &[TextDraft],
        parent: &TextDraft,
        parent_updates: &[TextDraft],
        child_draft: &TextDraft,
        offset_utf16: usize,
        updates: &[TextDraft],
        data: &RawMessage,
    ) -> Result<Self, Error> {
        let owner = &inventory.owner;
        let result = Self {
            version: if !parent_updates.is_empty() {
                17
            } else if !pre_updates.is_empty() {
                16
            } else if updates.is_empty() {
                3
            } else {
                8
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
            action: if !parent_updates.is_empty() {
                Action::SplitParent {
                    child: child.into(),
                    position: position.into(),
                    offset_utf16,
                    enter_draft: enter.rtml()?,
                    parent_draft: parent.rtml()?,
                    child_draft: child_draft.rtml()?,
                    pre_enter_updates: pre_updates
                        .iter()
                        .map(TextDraft::rtml)
                        .collect::<Result<Vec<_>, _>>()?,
                    parent_updates: parent_updates
                        .iter()
                        .map(TextDraft::rtml)
                        .collect::<Result<Vec<_>, _>>()?,
                    child_updates: updates
                        .iter()
                        .map(TextDraft::rtml)
                        .collect::<Result<Vec<_>, _>>()?,
                }
            } else if !pre_updates.is_empty() {
                Action::SplitPreEnter {
                    child: child.into(),
                    position: position.into(),
                    offset_utf16,
                    enter_draft: enter.rtml()?,
                    parent_draft: parent.rtml()?,
                    child_draft: child_draft.rtml()?,
                    pre_enter_updates: pre_updates
                        .iter()
                        .map(TextDraft::rtml)
                        .collect::<Result<Vec<_>, _>>()?,
                    child_updates: updates
                        .iter()
                        .map(TextDraft::rtml)
                        .collect::<Result<Vec<_>, _>>()?,
                }
            } else if updates.is_empty() {
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
            provenance: inventory.provenance_evidence(),
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
                Action::SplitPreEnter {
                    child,
                    position,
                    offset_utf16,
                    parent_draft,
                    child_draft,
                    pre_enter_updates,
                    child_updates,
                    ..
                } if self.version == 16 && !pre_enter_updates.is_empty() => (
                    child,
                    position,
                    offset_utf16,
                    parent_draft,
                    child_draft,
                    child_updates.as_slice(),
                ),
                Action::SplitParent {
                    child,
                    position,
                    offset_utf16,
                    parent_draft,
                    child_draft,
                    parent_updates,
                    child_updates,
                    ..
                } if self.version == 17 && !parent_updates.is_empty() => (
                    child,
                    position,
                    offset_utf16,
                    parent_draft,
                    child_draft,
                    child_updates.as_slice(),
                ),
                Action::SplitSequence {
                    child,
                    position,
                    offset_utf16,
                    parent_draft,
                    child_draft,
                    child_updates,
                    ..
                } if self.version == 18 => (
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
        let witness = if let Action::SplitPreEnter {
            enter_draft,
            pre_enter_updates,
            ..
        }
        | Action::SplitParent {
            enter_draft,
            pre_enter_updates,
            ..
        } = &self.action
        {
            let enter = TextDraft::from_rtml(enter_draft)?;
            let pre = pre_enter_updates
                .iter()
                .map(|value| {
                    let draft = TextDraft::from_rtml(value)?;
                    if draft.rtml()? != *value {
                        return Err(Error::Unsupported);
                    }
                    Ok(draft)
                })
                .collect::<Result<Vec<_>, Error>>()?;
            if enter.rtml()? != *enter_draft {
                return Err(Error::Unsupported);
            }
            if pre.is_empty() {
                if enter != before {
                    return Err(Error::Changed);
                }
                operation::SplitWitness::new(&before, *offset_utf16, &parent, &tail)?
            } else {
                operation::SplitWitness::new_with_pre_enter_updates(
                    &before,
                    &enter,
                    *offset_utf16,
                    &parent,
                    &tail,
                    &pre,
                )?
            }
        } else {
            operation::SplitWitness::new(&before, *offset_utf16, &parent, &tail)?
        }
        .with_child_updates(&updates)?;
        let witness = if let Action::SplitParent { parent_updates, .. }
        | Action::SplitSequence { parent_updates, .. } = &self.action
        {
            let reports = parent_updates
                .iter()
                .map(|value| {
                    let draft = TextDraft::from_rtml(value)?;
                    if draft.rtml()? != *value {
                        return Err(Error::Unsupported);
                    }
                    Ok(draft)
                })
                .collect::<Result<Vec<_>, Error>>()?;
            witness.with_parent_updates(&reports)?
        } else {
            witness
        };
        let witness = if let Action::SplitSequence {
            second_target,
            second_enter_draft,
            second_offset_utf16,
            second_parent_draft,
            second_child_draft,
            final_updates,
            ..
        } = &self.action
        {
            let draft = |value: &str| -> Result<TextDraft, Error> {
                let d = TextDraft::from_rtml(value)?;
                if d.rtml()? != value {
                    return Err(Error::Changed);
                }
                Ok(d)
            };
            let groups = final_updates
                .iter()
                .map(|g| g.iter().map(|v| draft(v)).collect::<Result<Vec<_>, _>>())
                .collect::<Result<Vec<_>, _>>()?;
            witness
                .with_second_split(
                    *second_target,
                    &draft(second_enter_draft)?,
                    *second_offset_utf16,
                    &draft(second_parent_draft)?,
                    &draft(second_child_draft)?,
                )?
                .with_final_updates(&groups[0], &groups[1], &groups[2])?
        } else {
            witness
        };
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
        if let Action::SplitSequence {
            second_child,
            second_position,
            second_target,
            ..
        } = &self.action
        {
            if sections.contains_key(second_child)
                || second_child == child
                || second_child == &self.target
                || !position::modern_id(second_child)
            {
                return Err(Error::Changed);
            }
            let expected = match second_target {
                super::super::SplitSequenceTarget::Original => position::between(
                    Some(original_position),
                    Some(position::base(inserted_position)?),
                )?,
                super::super::SplitSequenceTarget::FirstChild => {
                    position::between(Some(position::base(inserted_position)?), next)?
                }
            };
            if expected != *second_position {
                return Err(Error::Changed);
            }
            return operation::sequence::validate(
                source,
                child,
                inserted_position,
                second_child,
                second_position,
                &self.session,
                &witness,
                &data,
            );
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

impl Checkpoint {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::canvas::edit::structure) fn split_sequence(
        inventory: &Inventory,
        target: &str,
        child: &str,
        position: &str,
        second_child: &str,
        second_position: &str,
        session: &str,
        witness: &operation::SplitWitness,
        data: &RawMessage,
    ) -> Result<Self, Error> {
        let owner = &inventory.owner;
        let reports = |values: &[TextDraft]| {
            values
                .iter()
                .map(TextDraft::rtml)
                .collect::<Result<Vec<_>, _>>()
        };
        let final_reports = witness.final_updates().ok_or(Error::Unsupported)?;
        let result = Self {
            version: 18,
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
            action: Action::SplitSequence {
                child: child.into(),
                position: position.into(),
                offset_utf16: witness.offset_utf16(),
                parent_draft: witness.initial_parent_draft().rtml()?,
                child_draft: witness.initial_child_draft().rtml()?,
                parent_updates: reports(witness.parent_updates())?,
                child_updates: reports(witness.child_updates())?,
                second_child: second_child.into(),
                second_position: second_position.into(),
                second_target: witness.second_split_target().ok_or(Error::Unsupported)?,
                second_enter_draft: witness
                    .second_enter_draft()
                    .ok_or(Error::Unsupported)?
                    .rtml()?,
                second_offset_utf16: witness.second_offset_utf16().ok_or(Error::Unsupported)?,
                second_parent_draft: witness
                    .second_initial_parent_draft()
                    .ok_or(Error::Unsupported)?
                    .rtml()?,
                second_child_draft: witness
                    .second_initial_child_draft()
                    .ok_or(Error::Unsupported)?
                    .rtml()?,
                final_updates: [
                    reports(&final_reports[0])?,
                    reports(&final_reports[1])?,
                    reports(&final_reports[2])?,
                ],
            },
            draft: text::canonical_draft(
                inventory.sections.get(target).ok_or(Error::Unsupported)?,
            )?
            .rtml()?,
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
}
