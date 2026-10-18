//! Loaded flexbox/callout membership. Canonical custody is separate from the
//! configured renderer mode; neither creates comment permission or a DOM proof.
use super::{Error, Section, position};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Owner {
    pub root: String,
    pub column: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::wire::{EncodedField as F, RawMessage};
    fn t(tag: u32, value: &str) -> F {
        F::bytes(tag, value.as_bytes()).unwrap()
    }
    fn n(tag: u32, value: u64) -> F {
        F::uint(tag, value).unwrap()
    }
    fn raw(fields: &[F]) -> RawMessage {
        RawMessage::from_fields(fields).unwrap()
    }
    fn m(tag: u32, value: &RawMessage) -> F {
        F::message(tag, value).unwrap()
    }
    fn changed(section: &Section, f: F) -> Section {
        Section::from_raw(
            section
                .raw()
                .merge_partial(&RawMessage::decode(f.raw()).unwrap())
                .unwrap(),
        )
        .unwrap()
    }
    fn fixture(callout: bool) -> Vec<Section> {
        let (kind, style, tag) = if callout { (69, 49, 63) } else { (26, 21, 24) };
        let column = |id: &str, children: &[&str]| {
            let mut fields = if callout {
                vec![t(1, id)]
            } else {
                vec![n(2, 3)]
            };
            fields.extend(children.iter().map(|id| t(if callout { 2 } else { 1 }, id)));
            fields.push(t(99, "future column data"));
            m(1, &raw(&fields))
        };
        let root = Section::from_raw(raw(&[
            t(1, "root"),
            n(2, 4),
            t(6, "thread"),
            t(7, "document"),
            t(8, "b"),
            n(9, kind),
            n(10, style),
            m(
                12,
                &raw(&[m(
                    tag,
                    &raw(&[
                        column("first", &["late", "early"]),
                        column("second", &["last"]),
                        n(2, 0),
                    ]),
                )]),
            ),
        ]))
        .unwrap();
        let mut sections = vec![root];
        for (id, order, deleted) in [
            ("early", "c", false),
            ("late", "d", true),
            ("last", "e", false),
        ] {
            sections.push(
                Section::from_raw(raw(&[
                    t(1, id),
                    n(2, 8),
                    n(3, u64::from(deleted)),
                    t(6, "thread"),
                    t(7, "document"),
                    t(8, order),
                    n(9, 0),
                    n(10, 0),
                    n(29, 1),
                    m(12, &raw(&[m(1, &raw(&[t(1, id)]))])),
                ]))
                .unwrap(),
            );
        }
        sections
    }
    #[test]
    fn column_inventory_keeps_declared_column_order_and_deleted_raw_without_granting_visibility() {
        for callout in [false, true] {
            let sections = fixture(callout);
            let index = Columns::capture(&sections).unwrap();
            assert_eq!(index.owner("early").unwrap().column, 0);
            assert_eq!(index.owner("last").unwrap().column, 1);
            assert_eq!(index.root("root"), Some(&sections[0]));
            assert_eq!(index.original_sections().count(), 4);
            index.current_except_anchor(&sections, None).unwrap();
            let mut altered = sections.clone();
            altered[2] = changed(&altered[2], t(99, "changed deleted child"));
            assert!(
                index
                    .current_except_anchor(&altered, Some("early"))
                    .is_err()
            );
            altered = sections.clone();
            altered.remove(2);
            assert!(index.current_except_anchor(&altered, None).is_err());
        }
    }
    #[test]
    fn column_inventory_refuses_foreign_parent_flags_nested_roots_and_duplicate_membership() {
        for callout in [false, true] {
            let sections = fixture(callout);
            for field in [
                t(6, "foreign"),
                n(29, 0),
                m(13, &raw(&[t(1, "ordinary-list")])),
            ] {
                let mut bad = sections.clone();
                bad[1] = changed(&bad[1], field);
                assert!(Columns::capture(&bad).is_err());
            }
            let mut nested = sections.clone();
            nested[0] = changed(&nested[0], n(29, 1));
            assert!(Columns::capture(&nested).is_err());
            let mut duplicate = sections.clone();
            let tag = if callout { 63 } else { 24 };
            let content = sections[0].raw().message(12).unwrap().unwrap();
            let layout = content.message(tag).unwrap().unwrap();
            let first = layout.messages(1).unwrap().remove(0);
            let repeated = layout
                .replace_fields(1, &[m(1, &first), m(1, &first)])
                .unwrap();
            duplicate[0] = changed(&duplicate[0], m(12, &raw(&[m(tag, &repeated)])));
            assert!(Columns::capture(&duplicate).is_err());
        }
    }
    #[test]
    fn loaded_column_ids_and_zero_spans_do_not_replace_real_membership_or_sibling_order() {
        for callout in [false, true] {
            let mut sections = fixture(callout);
            let tag = if callout { 63 } else { 24 };
            let content = sections[0].raw().message(12).unwrap().unwrap();
            let layout = content.message(tag).unwrap().unwrap();
            let cols = layout
                .messages(1)
                .unwrap()
                .iter()
                .map(|c| {
                    let c = if callout {
                        c.remove(1).unwrap()
                    } else {
                        c.replace_fields(2, &[n(2, 0)]).unwrap()
                    };
                    m(1, &c)
                })
                .collect::<Vec<_>>();
            sections[0] = changed(
                &sections[0],
                m(
                    12,
                    &raw(&[m(tag, &layout.replace_fields(1, &cols).unwrap())]),
                ),
            );
            let index = Columns::capture(&sections).unwrap();
            assert_eq!(index.owner("last").unwrap().column, 1);
            // The previous ALL sibling lies in a different column. Neither
            // array index nor placement metadata enters its position path.
            assert_eq!(
                index.previous_position("last").unwrap().as_deref(),
                Some("d")
            );
            assert_eq!(index.previous_position("early").unwrap(), None);
            index.admit_body(&sections[1]).unwrap();
            assert!(index.admit_body(&sections[2]).is_err()); // deleted
        }
    }
}
pub(super) struct Columns {
    roots: BTreeMap<String, Section>,
    originals: BTreeMap<String, Section>,
    owners: BTreeMap<String, Owner>,
    previous: BTreeMap<String, Option<String>>,
}
impl Columns {
    pub(super) fn capture(sections: &[Section]) -> Result<Self, Error> {
        if sections.len() > crate::canvas::wire::MAX_SECTIONS {
            return Err(Error::Unsupported);
        }
        let mut all = BTreeMap::new();
        let mut bytes = 0usize;
        for section in sections {
            bytes = bytes
                .checked_add(section.raw().raw().len())
                .ok_or(Error::Unsupported)?;
            if bytes > crate::canvas::wire::MAX_BYTES || all.insert(section.id(), section).is_some()
            {
                return Err(Error::Unsupported);
            }
        }
        let mut roots = BTreeMap::new();
        let mut originals = BTreeMap::new();
        let mut owners = BTreeMap::new();
        for root in sections {
            let (style, content_tag, member_tag) = match root.section_type()? {
                Some(26) => (21, 24, 1),
                Some(69) => (49, 63, 2),
                _ => continue,
            };
            if root.style()? != Some(style)
                || root.raw().uint(11)?.unwrap_or(0) != 0
                || root.parents()?.is_some_and(|p| !p.fields().is_empty())
                || root.raw().boolean(29)? == Some(true)
                || root.raw().boolean(36)? == Some(true)
                || root.raw().has(15)
                || root.crdt()?.is_some()
            {
                return Err(Error::Unsupported);
            }
            position::base(root.raw().string(8)?.ok_or(Error::Unsupported)?)?;
            let content = root.raw().message(12)?.ok_or(Error::Unsupported)?;
            if content.fields().iter().any(|f| f.tag() != content_tag) {
                return Err(Error::Unsupported);
            }
            let layout = content.message(content_tag)?.ok_or(Error::Unsupported)?;
            if root.section_type()? == Some(26) {
                layout.boolean(2)?;
            } else {
                // Background color and future metadata remain original raw data.
                layout.uint(2)?;
            }
            for (index, column) in layout.messages(1)?.into_iter().enumerate() {
                if member_tag == 2 {
                    // Loaded membership/rendering uses column array position;
                    // its optional ID is metadata, not child-owner authority.
                    // Retain absent/empty/duplicate ID bytes without inventing
                    // a new insertion constraint for existing documents.
                    column.string(1)?;
                } else {
                    // Source span/default diagnostics are not a fabricated
                    // rendering certificate. Preserve the value, including zero.
                    column.uint(2)?;
                }
                for field in column.fields().iter().filter(|f| f.tag() == member_tag) {
                    let part = super::RawMessage::decode(column.encoded_field(field)?)?;
                    let id = part.string(member_tag)?.ok_or(Error::Unsupported)?;
                    let child = all.get(id).ok_or(Error::Changed)?;
                    if child.thread_id()? != root.thread_id()?
                        || child.document_id()? != root.document_id()?
                        || child.raw().boolean(29)? != Some(true)
                        || child.parents()?.is_some_and(|p| !p.fields().is_empty())
                        || child.raw().uint(11)?.unwrap_or(0) != 0
                        || !matches!(child.section_type()?.unwrap_or(0), 0 | 1 | 2 | 16 | 71)
                        || owners
                            .insert(
                                id.to_owned(),
                                Owner {
                                    root: root.id().to_owned(),
                                    column: index,
                                },
                            )
                            .is_some()
                    {
                        return Err(Error::Unsupported);
                    }
                    match child.section_type()? {
                        Some(1) => super::list_body::inventory_shape(child, true)?,
                        Some(2) => super::image_body::inventory_shape_with_layout(child, true)?,
                        Some(71) => super::blockquote_layout::root_shape(child)?,
                        _ => {}
                    }
                    originals.insert(id.to_owned(), (*child).clone());
                }
            }
            roots.insert(root.id().to_owned(), root.clone());
            originals.insert(root.id().to_owned(), root.clone());
        }
        let mut ordered = BTreeMap::new();
        for section in sections {
            if section.raw().uint(11)?.unwrap_or(0) != 0
                || section.parents()?.is_some_and(|p| !p.fields().is_empty())
            {
                continue;
            }
            let path = position::base(section.raw().string(8)?.ok_or(Error::Unsupported)?)?;
            if ordered.insert(path, section).is_some() {
                return Err(Error::Unsupported);
            }
        }
        let mut previous = BTreeMap::new();
        let mut last = BTreeMap::new();
        for (path, section) in ordered {
            if let Some(owner) = owners.get(section.id()) {
                previous.insert(
                    section.id().to_owned(),
                    last.insert(owner.root.clone(), path.to_owned()),
                );
            } else if section.raw().boolean(29)? != Some(true) {
                last.clear();
            }
        }
        Ok(Self {
            roots,
            originals,
            owners,
            previous,
        })
    }
    pub(super) fn owner(&self, id: &str) -> Option<&Owner> {
        self.owners.get(id)
    }
    pub(super) fn root(&self, id: &str) -> Option<&Section> {
        self.roots.get(id)
    }
    pub(super) fn original_sections(&self) -> impl Iterator<Item = &Section> {
        self.originals.values()
    }
    pub(super) fn previous_position(&self, id: &str) -> Result<Option<String>, Error> {
        self.previous.get(id).cloned().ok_or(Error::Changed)
    }
    /// Canonical current-owner admission only. A caller still needs the actual
    /// body/export and comment permission; this does not certify DOM visibility.
    pub(super) fn admit_body(&self, section: &Section) -> Result<(), Error> {
        let owner = self.owner(section.id()).ok_or(Error::Changed)?;
        let root = self.root(&owner.root).ok_or(Error::Changed)?;
        if self.originals.get(section.id()) != Some(section)
            || root.deleted()? == Some(true)
            || root.read_only()? == Some(true)
            || section.deleted()? == Some(true)
            || section.read_only()? == Some(true)
        {
            return Err(Error::Changed);
        }
        Ok(())
    }
    pub(super) fn current_except_anchor(
        &self,
        sections: &[Section],
        anchor: Option<&str>,
    ) -> Result<(), Error> {
        if anchor.is_some_and(|id| !self.owners.contains_key(id)) {
            return Err(Error::Changed);
        }
        let now = Self::capture(sections)?;
        if self.roots != now.roots
            || self.owners != now.owners
            || self
                .originals
                .iter()
                .filter(|(id, _)| Some(id.as_str()) != anchor)
                .ne(now
                    .originals
                    .iter()
                    .filter(|(id, _)| Some(id.as_str()) != anchor))
        {
            return Err(Error::Changed);
        }
        Ok(())
    }
}
