//! Canonical loaded blockquote membership. Not an authoring or layout certificate.
//! Table ownership and native visibility must be checked by the combined caller.
use super::{Error, Section, position};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Quotes {
    originals: BTreeMap<String, Section>,
    roots: BTreeSet<String>,
    owners: BTreeMap<String, String>,
    ordinary: BTreeMap<String, String>,
    previous: BTreeMap<String, Option<String>>,
}

impl Quotes {
    pub(super) fn capture(sections: &[Section]) -> Result<Self, Error> {
        let mut all = BTreeMap::new();
        let mut bytes = 0usize;
        if sections.len() > crate::canvas::wire::MAX_SECTIONS {
            return Err(Error::Unsupported);
        }
        for section in sections {
            bytes = bytes
                .checked_add(section.raw().raw().len())
                .ok_or(Error::Unsupported)?;
            if bytes > crate::canvas::wire::MAX_BYTES || all.insert(section.id(), section).is_some()
            {
                return Err(Error::Unsupported);
            }
        }
        let mut originals = BTreeMap::new();
        let mut roots = BTreeSet::new();
        let mut owners = BTreeMap::new();
        for root in sections {
            if root.section_type()? != Some(71) {
                continue;
            }
            root_shape(root)?;
            roots.insert(root.id().to_owned());
            originals.insert(root.id().to_owned(), root.clone());
            let body = root
                .raw()
                .message(12)?
                .ok_or(Error::Unsupported)?
                .message(65)?
                .ok_or(Error::Unsupported)?;
            for field in body.fields().iter().filter(|f| f.tag() == 1) {
                let entry = super::RawMessage::decode(body.encoded_field(field)?)?;
                let id = entry
                    .string(1)?
                    .filter(|id| !id.is_empty())
                    .ok_or(Error::Unsupported)?;
                let child = all.get(id).ok_or(Error::Changed)?;
                if child.id() == root.id()
                    || child.thread_id()? != root.thread_id()?
                    || child.document_id()? != root.document_id()?
                    || child.raw().boolean(29)? != Some(true)
                    || child.parents()?.is_some_and(|p| !p.fields().is_empty())
                    || child.raw().uint(11)?.unwrap_or(0) != 0
                    || !matches!(child.section_type()?.unwrap_or(0), 0 | 1 | 2 | 71)
                    || owners.insert(id.to_owned(), root.id().to_owned()).is_some()
                {
                    return Err(Error::Unsupported);
                }
                match child.section_type()?.unwrap_or(0) {
                    1 => super::list_body::inventory_shape(child, true)?,
                    2 => super::image_body::inventory_shape_with_layout(child, true)?,
                    71 => root_shape(child)?,
                    _ => {
                        if child.raw().boolean(36)? == Some(true) || child.raw().has(15) {
                            return Err(Error::Unsupported);
                        }
                    }
                }
                originals.insert(id.to_owned(), (*child).clone());
            }
        }
        // Existing loaded nesting is not rewritten to the insertion depth limit.
        // Canonical cycles, unlike deep but finite nesting, cannot establish an owner.
        for root in &roots {
            let mut seen = BTreeSet::new();
            let mut cursor = root.as_str();
            while let Some(parent) = owners.get(cursor) {
                if !seen.insert(cursor) {
                    return Err(Error::Unsupported);
                }
                cursor = parent;
            }
        }
        let mut ordinary = BTreeMap::new();
        for child in sections {
            let Some(parent) = child.parents()?.filter(|p| !p.fields().is_empty()) else {
                continue;
            };
            let Some(id) = parent.string(1)? else {
                continue;
            };
            if !owners.contains_key(id) {
                continue;
            }
            let owner = all.get(id).ok_or(Error::Changed)?;
            if owner.section_type()? != Some(1)
                || parent.fields().iter().any(|f| !matches!(f.tag(), 1 | 4))
                || parent.uint(4)? != owner.style()?
                || child.section_type()?.unwrap_or(0) != 0
                || child.style()?.unwrap_or(0) != 0
                || child.raw().boolean(29)? == Some(true)
                || child.thread_id()? != owner.thread_id()?
                || child.document_id()? != owner.document_id()?
            {
                return Err(Error::Unsupported);
            }
            ordinary.insert(child.id().to_owned(), id.to_owned());
            originals.insert(child.id().to_owned(), child.clone());
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
                    last.insert(owner.clone(), path.to_owned()),
                );
            } else if section.raw().boolean(29)? != Some(true) {
                // Actual ALL sibling traversal skips foreign layouts, but an
                // ordinary top-level section is a boundary for every layout.
                last.clear();
            }
        }
        Ok(Self {
            originals,
            roots,
            owners,
            ordinary,
            previous,
        })
    }

    pub(super) fn original_sections(&self) -> impl Iterator<Item = &Section> {
        self.originals.values()
    }
    pub(super) fn root_section(&self, id: &str) -> Option<&Section> {
        self.roots
            .contains(id)
            .then(|| self.originals.get(id))
            .flatten()
    }
    pub(super) fn owner(&self, id: &str) -> Option<&str> {
        self.owners.get(id).map(String::as_str)
    }
    pub(super) fn direct_body(&self, id: &str) -> Option<&str> {
        if self.owners.contains_key(id) {
            Some(self.originals.get_key_value(id)?.0)
        } else {
            self.ordinary.get(id).map(String::as_str)
        }
    }
    /// Follows only the captured canonical quote ownership tree. A returned root
    /// with raw29=true still needs an actual outer-table association from caller.
    pub(super) fn outer_root(&self, id: &str) -> Option<&str> {
        let mut current = self.direct_body(id)?;
        while let Some(parent) = self.owner(current) {
            current = parent;
        }
        Some(current)
    }
    pub(super) fn previous_position(&self, id: &str) -> Result<Option<String>, Error> {
        self.previous.get(id).cloned().ok_or(Error::Changed)
    }
    pub(super) fn live_chain(&self, section: &Section) -> Result<(), Error> {
        if self.originals.get(section.id()) != Some(section)
            || section.deleted()? == Some(true)
            || section.read_only()? == Some(true)
        {
            return Err(Error::Changed);
        }
        let mut current = self.direct_body(section.id()).ok_or(Error::Changed)?;
        while let Some(parent) = self.owner(current) {
            let root = self.originals.get(parent).ok_or(Error::Changed)?;
            if root.deleted()? == Some(true) || root.read_only()? == Some(true) {
                return Err(Error::Changed);
            }
            current = parent;
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
        if self.owners != now.owners
            || self.ordinary != now.ordinary
            || self.roots != now.roots
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

pub(super) fn root_shape(root: &Section) -> Result<(), Error> {
    if root.section_type()? != Some(71)
        || root.style()? != Some(50)
        || root.raw().uint(11)?.unwrap_or(0) != 0
        || root.parents()?.is_some_and(|p| !p.fields().is_empty())
        || root.raw().boolean(36)? == Some(true)
        || root.raw().has(15)
        || root.crdt()?.is_some()
    {
        return Err(Error::Unsupported);
    }
    let content = root.raw().message(12)?.ok_or(Error::Unsupported)?;
    if content.fields().iter().any(|f| f.tag() != 65) || content.message(65)?.is_none() {
        return Err(Error::Unsupported);
    }
    position::base(root.raw().string(8)?.ok_or(Error::Unsupported)?)?;
    Ok(())
}

#[cfg(test)]
mod tests;
