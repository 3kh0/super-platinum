//! v14 ordered paragraph sequence evidence. Validation never grants replay authority.
use super::*;
use crate::canvas::edit::structure::{end as operation, inventory};
use std::collections::BTreeMap;

impl Checkpoint {
    pub(in crate::canvas::edit::structure) fn end_sequence(
        inventory: &Inventory,
        target: &str,
        session: &str,
        before: &TextDraft,
        members: &[operation::sequence::Member],
        data: &RawMessage,
    ) -> Result<Self, Error> {
        let owner = &inventory.owner;
        let result = Self {
            version: 14,
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
            action: Action::EndSequence {
                members: members
                    .iter()
                    .map(|m| {
                        Ok(SequenceMember {
                            id: m.id.clone(),
                            position: m.position.clone(),
                            reused: m.reused,
                            updates: m
                                .updates
                                .iter()
                                .map(TextDraft::rtml)
                                .collect::<Result<Vec<_>, _>>()?,
                        })
                    })
                    .collect::<Result<Vec<_>, Error>>()?,
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
    pub(super) fn validate_end_sequence(&self) -> Result<(), Error> {
        let Action::EndSequence { members } = &self.action else {
            return Err(Error::Unsupported);
        };
        // Bound inert serialized reports before parsing or cloning any drafts.
        let mut reports = 0usize;
        let mut report_bytes = 0usize;
        operation::sequence::budget::counts(members.len(), 0, 0)?;
        for m in members {
            reports = reports
                .checked_add(m.updates.len())
                .ok_or(Error::Unsupported)?;
            operation::sequence::budget::counts(members.len(), reports, report_bytes)?;
            for raw in &m.updates {
                report_bytes = report_bytes
                    .checked_add(raw.len())
                    .ok_or(Error::Unsupported)?;
                operation::sequence::budget::counts(members.len(), reports, report_bytes)?;
            }
        }
        let sections = self
            .sections
            .iter()
            .map(|raw| {
                let s = Section::decode(raw)?;
                Ok((s.id().to_owned(), s))
            })
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        let mut ordered = BTreeMap::new();
        let mut title = None;
        for s in sections.values() {
            if s.raw().uint(11)?.unwrap_or(0) != 0
                || s.raw().boolean(29)? == Some(true)
                || s.raw().has(15)
            {
                return Err(Error::Unsupported);
            }
            inventory::validate_placement(s, &sections)?;
            ordered.insert(s.raw().string(8)?.ok_or(Error::Unsupported)?, s);
            if s.section_type()? == Some(64) || s.style()? == Some(48) {
                if title.replace(s).is_some() {
                    return Err(Error::Unsupported);
                }
            }
        }
        let original = sections.get(&self.target).ok_or(Error::Unsupported)?;
        let before = TextDraft::from_rtml(&self.draft)?;
        operation::repeat::admit(original, &before)?;
        let selected = operation::empty::candidate(
            &ordered.values().copied().collect::<Vec<_>>(),
            &self.target,
        )?;
        operation::sequence::budget::inventory(sections.len(), members.len(), selected.is_some())?;
        let mut seen = BTreeSet::from([self.target.as_str()]);
        let mut previous = original.raw().string(8)?.ok_or(Error::Unsupported)?;
        let interval_start = selected.map_or(Ok(previous), |s| {
            s.raw().string(8)?.ok_or(Error::Unsupported)
        })?;
        let next = ordered.keys().copied().find(|p| *p > interval_start);
        let data = RawMessage::decode(&self.request)?;
        let rows = data.messages(1)?;
        if rows.len() != members.len() + 1
            || data.message(14)?
                != Some(CanonicalTitle::rich_from_section(
                    title.ok_or(Error::Unsupported)?,
                )?)
        {
            return Err(Error::Unsupported);
        }
        let mut plan = Vec::new();
        for (i, m) in members.iter().enumerate() {
            if !seen.insert(m.id.as_str()) || m.reused != (i == 0 && selected.is_some()) {
                return Err(Error::Unsupported);
            }
            if m.reused {
                let source = selected.ok_or(Error::Unsupported)?;
                if source.id() != m.id || source.raw().string(8)? != Some(m.position.as_str()) {
                    return Err(Error::Unsupported);
                }
            } else {
                let previous_base = position::base(previous)?;
                if sections.contains_key(&m.id)
                    || !position::modern_id(&m.id)
                    || !m.position.ends_with(":temp")
                    || position::between(Some(previous_base), next)? != m.position
                {
                    return Err(Error::Unsupported);
                }
            }
            previous = &m.position;
            let updates = m
                .updates
                .iter()
                .map(|raw| {
                    let d = TextDraft::from_rtml(raw)?;
                    if d.rtml()? != *raw {
                        return Err(Error::Unsupported);
                    }
                    Ok(d)
                })
                .collect::<Result<Vec<_>, Error>>()?;
            plan.push(operation::sequence::Member {
                id: m.id.clone(),
                position: m.position.clone(),
                reused: m.reused,
                draft: updates.last().cloned().unwrap_or_default(),
                updates,
                wire: rows[i + 1].message(38)?.ok_or(Error::Unsupported)?,
            });
        }
        operation::sequence::validate(original, selected, &self.session, &before, &plan, &data)
    }
}
