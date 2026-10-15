//! v2 stores one complete two-target request. v1 remains byte-for-byte readable;
//! opening either version only constructs inert evidence, never an edit/actor.
use super::*;
use automerge::ReadDoc;

impl Checkpoint {
    pub(in crate::canvas::edit::structure) fn pair(
        deletion: Self,
        insertion: Self,
        data: &RawMessage,
    ) -> Result<Self, Error> {
        deletion.validate()?;
        insertion.validate()?;
        let Action::Insert {
            anchor,
            before: false,
            position,
        } = &insertion.action
        else {
            return Err(Error::Unsupported);
        };
        if deletion.action != Action::Delete
            || anchor != &deletion.target
            || deletion.scope != insertion.scope
            || deletion.thread != insertion.thread
            || deletion.document != insertion.document
            || deletion.sections != insertion.sections
            || deletion.session != insertion.session
            || insertion.draft != TextDraft::default().rtml()?
        {
            return Err(Error::Unsupported);
        }
        let old_data = RawMessage::decode(&deletion.request)?;
        let new_data = RawMessage::decode(&insertion.request)?;
        let expected = RawMessage::from_fields(&[
            F::message(1, &old_data.message(1)?.ok_or(Error::Unsupported)?)?,
            F::message(1, &new_data.message(1)?.ok_or(Error::Unsupported)?)?,
            F::message(14, &old_data.message(14)?.ok_or(Error::Unsupported)?)?,
        ])?;
        if &expected != data || old_data.message(14)? != new_data.message(14)? {
            return Err(Error::Unsupported);
        }
        let result = Self {
            version: 2,
            action: Action::Replace {
                replacement: insertion.target,
                position: position.clone(),
            },
            request: data.raw().to_vec(),
            ..deletion
        };
        result.validate()?;
        Ok(result)
    }

    pub(super) fn validate_pair(&self) -> Result<(), Error> {
        let Action::Replace {
            replacement,
            position: inserted_position,
        } = &self.action
        else {
            return Err(Error::Unsupported);
        };
        if self.version != 2 || self.sections.len() != 2 || replacement == &self.target {
            return Err(Error::Unsupported);
        }
        let sections = self
            .sections
            .iter()
            .map(|b| Section::decode(b))
            .collect::<Result<Vec<_>, _>>()?;
        let old = sections
            .iter()
            .find(|s| s.id() == self.target)
            .ok_or(Error::Unsupported)?;
        let title = sections
            .iter()
            .find(|s| s.id() != self.target)
            .ok_or(Error::Unsupported)?;
        if old.section_type()?.unwrap_or(0) != 0
            || old.style()?.unwrap_or(0) != 0
            || old.read_only()? == Some(true)
            || old.attributes()?.is_some_and(|a| a.has(29))
            || title.section_type()? != Some(64)
            || title.style()? != Some(48)
            || text::canonical_draft(old)?.rtml()? != self.draft
        {
            return Err(Error::Unsupported);
        }
        for section in &sections {
            if section.parents()?.is_some_and(|p| !p.fields().is_empty())
                || section.raw().uint(11)?.unwrap_or(0) != 0
                || section.raw().boolean(29)? == Some(true)
                || section.raw().has(15)
                || section
                    .raw()
                    .string(21)?
                    .is_some_and(|path| section.raw().string(8).ok().flatten() != Some(path))
            {
                return Err(Error::Unsupported);
            }
        }
        let old_position = old.raw().string(8)?.ok_or(Error::Unsupported)?;
        if title.raw().string(8)?.ok_or(Error::Unsupported)? >= old_position
            || position::between(Some(old_position), None)? != *inserted_position
        {
            return Err(Error::Unsupported);
        }
        let data = RawMessage::decode(&self.request)?;
        if data.fields().iter().any(|f| !matches!(f.tag(), 1 | 14)) {
            return Err(Error::Unsupported);
        }
        let updates = data.messages(1)?;
        let [deleted, inserted] = updates.as_slice() else {
            return Err(Error::Unsupported);
        };
        // Reuse v1 identity/trace/raw-inventory checks for each member, without
        // rewriting the durable original request or inventing a second receipt.
        for (update, id, action, draft) in [
            (
                deleted,
                self.target.as_str(),
                Action::Delete,
                self.draft.clone(),
            ),
            (
                inserted,
                replacement.as_str(),
                Action::Insert {
                    anchor: self.target.clone(),
                    before: false,
                    position: inserted_position.clone(),
                },
                TextDraft::default().rtml()?,
            ),
        ] {
            let member = Self {
                version: 1,
                target: id.into(),
                action,
                draft,
                request: data
                    .replace_fields(1, &[F::message(1, update)?])?
                    .raw()
                    .to_vec(),
                ..self.clone()
            };
            member.validate()?;
            let expected_tags: &[u32] = if member.is_delete() {
                &[1, 2, 3, 9, 13, 19, 20, 33, 38]
            } else {
                &[
                    1, 2, 3, 8, 9, 10, 11, 12, 13, 16, 19, 20, 21, 29, 33, 35, 38,
                ]
            };
            if update.fields().iter().map(|f| f.tag()).collect::<Vec<_>>() != expected_tags {
                return Err(Error::Unsupported);
            }
            let wire = update.message(38)?.ok_or(Error::Unsupported)?;
            let expected_base = if member.is_delete() {
                old.sequence()?.ok_or(Error::Unsupported)?
            } else {
                0
            };
            if update.uint(2)? != Some(expected_base)
                || update.uint(19)? != Some(1)
                || wire.uint(3)? != Some(expected_base)
                || update.uint(9)? != Some(0)
                || update.message(13)?.is_none_or(|p| !p.fields().is_empty())
            {
                return Err(Error::Unsupported);
            }
            let canonical = if member.is_delete() {
                old.merge_partial(&Section::from_raw(update.clone())?)?
            } else {
                if update.string(8)? != Some(inserted_position.as_str())
                    || update.string(21)? != Some(inserted_position.as_str())
                    || update.uint(10)? != Some(0)
                    || update.uint(11)? != Some(0)
                    || update.uint(29)? != Some(0)
                    || update.uint(35)? != Some(0)
                    || wire.uint(2)? != Some(0)
                    || update.message(16)? != Some(RawMessage::from_fields(&[F::uint(4, 0)?])?)
                    || text::canonical_draft(&Section::from_raw(update.clone())?)?.rtml()?
                        != member.draft
                {
                    return Err(Error::Unsupported);
                }
                Section::from_raw(update.clone())?
            };
            let doc = crdt::validate(
                &wire,
                &canonical,
                &TextDraft::from_rtml(&member.draft)?,
                member.is_delete(),
                None,
                !member.is_delete() || position::modern_id(old.id()),
            )?;
            if member.is_delete() {
                if let Some(seed) = old.crdt()? {
                    let original_doc = crdt::validate(
                        &seed,
                        old,
                        &TextDraft::from_rtml(&self.draft)?,
                        false,
                        None,
                        position::modern_id(old.id()),
                    )?;
                    if wire.uint(2)? != seed.uint(2)?
                        || wire.string(8)? != seed.string(8)?
                        || original_doc
                            .get_changes(&[])
                            .iter()
                            .any(|change| doc.get_change_by_hash(&change.hash()).is_none())
                    {
                        return Err(Error::Unsupported);
                    }
                } else if wire.uint(2)? != old.sequence()? {
                    return Err(Error::Unsupported);
                }
            }
            let head = wire.string(12)?.ok_or(Error::Unsupported)?;
            if !wire.fields().iter().filter(|f| f.tag() == 6).any(|f| {
                wire.encoded_field(f)
                    .ok()
                    .and_then(|b| RawMessage::decode(b).ok())
                    .is_some_and(|r| r.string(6).ok().flatten() == Some(head))
            }) || doc
                .get_heads()
                .first()
                .is_none_or(|h| h.to_string() != head)
            {
                return Err(Error::Unsupported);
            }
        }
        Ok(())
    }
}
