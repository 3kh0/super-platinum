//! A v5 reuse end-Enter checkpoint is one inert pair. Decoding never recreates an intent,
//! actor or request authority; the full original request remains byte-preserved.
use super::*;
use crate::canvas::edit::structure::{end as operation, inventory};
use std::collections::BTreeMap;

impl Checkpoint {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::canvas::edit::structure) fn end_reuse(
        inventory: &Inventory,
        target: &str,
        child: &str,
        position: &str,
        session: &str,
        before: &TextDraft,
        offset_utf16: usize,
        data: &RawMessage,
    ) -> Result<Self, Error> {
        let owner = &inventory.owner;
        let result = Self {
            version: 5,
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
            action: Action::EndReuse {
                child: child.into(),
                position: position.into(),
                offset_utf16,
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

    pub(super) fn validate_end_reuse(&self) -> Result<(), Error> {
        let Action::EndReuse {
            child,
            position: inserted_position,
            offset_utf16,
        } = &self.action
        else {
            return Err(Error::Unsupported);
        };
        if self.version != 5 || child == &self.target {
            return Err(Error::Unsupported);
        }
        let before = TextDraft::from_rtml(&self.draft)?;
        // Logical consistency only. This does not recreate the renderer's
        // original input/range witness or grant a second dispatch.
        let witness =
            operation::EndWitness::new(&before, *offset_utf16, &before, &TextDraft::default())?;
        let sections = self
            .sections
            .iter()
            .map(|raw| {
                let s = Section::decode(raw)?;
                Ok((s.id().to_owned(), s))
            })
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        if !sections.contains_key(child) {
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
        let ordered_rows = ordered.values().copied().collect::<Vec<_>>();
        let reused =
            operation::reuse_candidate(&ordered_rows, &self.target)?.ok_or(Error::Unsupported)?;
        if reused.id() != child || reused.raw().string(8)? != Some(inserted_position.as_str()) {
            return Err(Error::Unsupported);
        }
        let data = RawMessage::decode(&self.request)?;
        let expected_title = CanonicalTitle::rich_from_section(title.ok_or(Error::Unsupported)?)?;
        if data.message(14)?.as_ref() != Some(&expected_title) {
            return Err(Error::Unsupported);
        }
        operation::validate_checkpoint_reuse(source, reused, &self.session, &witness, &data)
    }
}
