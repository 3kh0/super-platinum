//! Canonical modern Canvas table membership, independent of opaque spreadsheets.
//! Tables cannot be annotated. Their original ordinary child sections can be.
use super::{Error, RawMessage, Section, position};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Observation {
    sections: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Child {
    pub table: String,
    pub cell: String,
    pub row: String,
    pub column: String,
    pub merged_out: bool,
}

pub(super) struct Tables {
    originals: Vec<Section>,
    children: BTreeMap<String, Child>,
    previous: BTreeMap<String, Option<String>>,
    bodies: bool,
    descendants: BTreeMap<String, String>,
    quotes: Option<super::blockquote_layout::Quotes>,
    columns: Option<super::column_layout::Columns>,
}

fn id(raw: &RawMessage, tag: u32) -> Result<String, Error> {
    raw.string(tag)?
        .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
        .map(str::to_owned)
        .ok_or(Error::Unsupported)
}

fn repeated_ids(raw: &RawMessage, tag: u32) -> Result<Vec<String>, Error> {
    raw.fields()
        .iter()
        .filter(|f| f.tag() == tag)
        .map(|f| id(&RawMessage::decode(raw.encoded_field(f)?)?, tag))
        .collect()
}

struct Axis {
    ids: Vec<String>,
    indices: BTreeMap<String, usize>,
}
impl Axis {
    fn decode(raw: &RawMessage, tag: u32) -> Result<Self, Error> {
        let mut positions = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for entry in raw.messages(tag)? {
            let name = id(&entry, 1)?;
            let order = position::base(entry.string(2)?.ok_or(Error::Unsupported)?)?;
            if !seen.insert(name.clone()) || positions.insert(order.to_owned(), name).is_some() {
                return Err(Error::Unsupported);
            }
        }
        let ids: Vec<_> = positions.into_values().collect();
        let indices = ids
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, id)| (id, i))
            .collect();
        Ok(Self { ids, indices })
    }
    fn index(&self, id: &str) -> Result<usize, Error> {
        self.indices.get(id).copied().ok_or(Error::Changed)
    }
    fn bound(&self, raw: Option<RawMessage>, default: usize) -> Result<usize, Error> {
        let Some(raw) = raw else { return Ok(default) };
        // The live table cache uses the boundary ID, not its saved position.
        // Preserve that position in the original table without replacing it.
        raw.string(2)?;
        self.index(&id(&raw, 1)?)
    }
}

struct Cell {
    id: String,
    row: usize,
    column: usize,
    end_row: usize,
    end_column: usize,
    children: Vec<String>,
}

impl Tables {
    pub(super) fn capture(sections: &[Section]) -> Result<Self, Error> {
        Self::capture_mode(sections, false, false, false)
    }
    pub(super) fn capture_bodies(sections: &[Section]) -> Result<Self, Error> {
        Self::capture_mode(sections, true, false, false)
    }
    pub(super) fn capture_quotes(sections: &[Section]) -> Result<Self, Error> {
        Self::capture_mode(sections, true, true, false)
    }
    pub(super) fn capture_columns(sections: &[Section]) -> Result<Self, Error> {
        Self::capture_mode(sections, true, true, true)
    }
    pub(super) fn has_bodies(sections: &[Section]) -> Result<bool, Error> {
        for section in sections {
            if section.raw().boolean(29)? == Some(true)
                && matches!(section.section_type()?, Some(1 | 2))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn capture_mode(
        sections: &[Section],
        bodies: bool,
        quote_layout: bool,
        column_layout: bool,
    ) -> Result<Self, Error> {
        let columns = column_layout
            .then(|| super::column_layout::Columns::capture(sections))
            .transpose()?;
        let quotes = quote_layout
            .then(|| super::blockquote_layout::Quotes::capture(sections))
            .transpose()?;
        let mut originals = Vec::new();
        let mut children = BTreeMap::new();
        let mut section_ids = BTreeMap::new();
        let mut total = 0usize;
        for section in sections {
            total = total
                .checked_add(section.raw().raw().len())
                .ok_or(Error::Unsupported)?;
            if total > crate::canvas::wire::MAX_BYTES
                || section_ids.insert(section.id(), section).is_some()
            {
                return Err(Error::Unsupported);
            }
        }
        if sections.len() > crate::canvas::wire::MAX_SECTIONS {
            return Err(Error::Unsupported);
        }
        for table in sections
            .iter()
            .filter(|s| s.section_type().ok() == Some(Some(33)))
        {
            Self::table(table, &section_ids, &mut children, bodies, quote_layout)?;
            originals.push(table.clone());
        }
        // A layout flag is not an owner. Every direct ordinary layout child
        // must have exactly one actual cell, quote or column owner in this model.
        for section in sections {
            let quote_owner = quotes.as_ref().and_then(|q| q.owner(section.id()));
            let column_owner = columns.as_ref().and_then(|c| c.owner(section.id()));
            let owners = usize::from(children.contains_key(section.id()))
                + usize::from(quote_owner.is_some())
                + usize::from(column_owner.is_some());
            if (section.raw().boolean(29)? == Some(true)) != (owners == 1) || owners > 1 {
                return Err(Error::Changed);
            }
            if children.contains_key(section.id()) {
                originals.push(section.clone());
            }
        }
        let mut descendants = BTreeMap::new();
        if bodies {
            // List export omits deleted children. Custody never does: retain
            // every exact descendant, independent of whether it paints/exported.
            for section in sections {
                let Some(parent) = section.parents()?.filter(|p| !p.fields().is_empty()) else {
                    continue;
                };
                let Some(id) = parent.string(1)? else {
                    continue;
                };
                if !children.contains_key(id)
                    && !columns.as_ref().is_some_and(|c| c.owner(id).is_some())
                {
                    continue;
                }
                let owner = section_ids.get(id).ok_or(Error::Changed)?;
                if owner.section_type()? != Some(1)
                    || parent.fields().iter().any(|f| !matches!(f.tag(), 1 | 4))
                    || parent.uint(4)? != owner.style()?
                    || section.section_type()?.unwrap_or(0) != 0
                    || section.style()?.unwrap_or(0) != 0
                    || section.raw().boolean(29)? == Some(true)
                    || section.thread_id()? != owner.thread_id()?
                    || section.document_id()? != owner.document_id()?
                {
                    return Err(Error::Unsupported);
                }
                descendants.insert(section.id().to_owned(), id.to_owned());
                originals.push(section.clone());
            }
        }
        if let Some(quotes) = &quotes {
            originals.extend(quotes.original_sections().cloned());
        }
        if let Some(columns) = &columns {
            originals.extend(columns.original_sections().cloned());
        }
        originals.sort_by(|a, b| a.id().cmp(b.id()));
        // All copies came from the single duplicate-free canonical section map.
        originals.dedup_by(|a, b| a.id() == b.id());
        let mut ordered = BTreeMap::new();
        for section in sections {
            if section.raw().uint(11)?.unwrap_or(0) != 0
                || section.parents()?.is_some_and(|p| !p.fields().is_empty())
            {
                continue;
            }
            let path = position::base(section.raw().string(8)?.ok_or(Error::Unsupported)?)?;
            if ordered.insert(path, section.id()).is_some() {
                return Err(Error::Unsupported);
            }
        }
        let mut previous = BTreeMap::new();
        let mut last = BTreeMap::new();
        for (path, id) in ordered {
            if let Some(child) = children.get(id) {
                previous.insert(
                    id.to_owned(),
                    last.insert(child.table.clone(), path.to_owned()),
                );
            } else if section_ids
                .get(id)
                .ok_or(Error::Changed)?
                .raw()
                .boolean(29)?
                != Some(true)
            {
                last.clear();
            }
        }
        Ok(Self {
            originals,
            children,
            previous,
            bodies,
            descendants,
            quotes,
            columns,
        })
    }

    fn table(
        table: &Section,
        sections: &BTreeMap<&str, &Section>,
        output: &mut BTreeMap<String, Child>,
        bodies: bool,
        quote_layout: bool,
    ) -> Result<(), Error> {
        if table.style()? != Some(28)
            || table.raw().uint(11)?.unwrap_or(0) != 0
            || table.parents()?.is_some_and(|p| !p.fields().is_empty())
            || table.raw().boolean(29)? == Some(true)
            || table.raw().boolean(36)? == Some(true)
            || table.raw().has(15)
            || table.crdt()?.is_some()
        {
            return Err(Error::Unsupported);
        }
        let content = table.raw().message(12)?.ok_or(Error::Unsupported)?;
        if content.fields().iter().any(|f| f.tag() != 30) {
            return Err(Error::Unsupported);
        }
        let layout = content.message(30)?.ok_or(Error::Unsupported)?;
        layout.boolean(5)?;
        layout.boolean(6)?;
        let rows = Axis::decode(&layout, 1)?;
        let columns = Axis::decode(&layout, 2)?;
        let mut cells = Vec::new();
        let mut ids = BTreeSet::new();
        let mut coordinates = BTreeMap::new();
        for raw in layout.messages(3)? {
            let name = id(&raw, 1)?;
            let row = rows.index(&id(&raw, 2)?)?;
            let column = columns.index(&id(&raw, 3)?)?;
            if !ids.insert(name.clone())
                || coordinates.insert((row, column), name.clone()).is_some()
            {
                return Err(Error::Unsupported);
            }
            let merge = raw.message(6)?;
            let end_row = rows.bound(
                merge.as_ref().map(|m| m.message(1)).transpose()?.flatten(),
                row,
            )?;
            let end_column = columns.bound(
                merge.as_ref().map(|m| m.message(2)).transpose()?.flatten(),
                column,
            )?;
            if end_row < row || end_column < column {
                return Err(Error::Unsupported);
            }
            cells.push(Cell {
                id: name,
                row,
                column,
                end_row,
                end_column,
                children: repeated_ids(&raw, 4)?,
            });
        }
        // Match the source cache's merged-out relation. Refuse corrupt overlap
        // instead of depending on Map insertion order to choose an owner.
        let mut merged = BTreeSet::new();
        let mut visited = 0usize;
        for cell in &cells {
            let area = (cell.end_row - cell.row + 1)
                .checked_mul(cell.end_column - cell.column + 1)
                .ok_or(Error::Unsupported)?;
            visited = visited.checked_add(area).ok_or(Error::Unsupported)?;
            if area > coordinates.len() || visited > crate::canvas::wire::MAX_FIELDS {
                return Err(Error::Unsupported);
            }
            for row in cell.row..=cell.end_row {
                for column in cell.column..=cell.end_column {
                    if row == cell.row && column == cell.column {
                        continue;
                    }
                    let other = coordinates.get(&(row, column)).ok_or(Error::Changed)?;
                    if !merged.insert(other.clone()) {
                        return Err(Error::Unsupported);
                    }
                }
            }
        }
        for cell in cells {
            if merged.contains(&cell.id)
                && (cell.end_row != cell.row || cell.end_column != cell.column)
            {
                return Err(Error::Unsupported);
            }
            for child_id in cell.children {
                let child = sections.get(child_id.as_str()).ok_or(Error::Changed)?;
                if bodies {
                    match child.section_type()? {
                        Some(1) => super::list_body::inventory_shape(child, true)?,
                        Some(2) => super::image_body::inventory_shape_with_layout(child, true)?,
                        Some(71) if quote_layout => super::blockquote_layout::root_shape(child)?,
                        _ => {}
                    }
                }
                if child.thread_id()? != table.thread_id()?
                    || child.document_id()? != table.document_id()?
                    || child.raw().boolean(29)? != Some(true)
                    || child.parents()?.is_some_and(|p| !p.fields().is_empty())
                    || !(child.section_type()?.unwrap_or(0) == 0
                        || (bodies && matches!(child.section_type()?, Some(1 | 2)))
                        || (quote_layout && child.section_type()? == Some(71)))
                    || child.raw().uint(11)?.unwrap_or(0) != 0
                    || output
                        .insert(
                            child_id,
                            Child {
                                table: table.id().to_owned(),
                                cell: cell.id.clone(),
                                row: rows.ids[cell.row].clone(),
                                column: columns.ids[cell.column].clone(),
                                merged_out: merged.contains(&cell.id),
                            },
                        )
                        .is_some()
                {
                    return Err(Error::Unsupported);
                }
            }
        }
        Ok(())
    }

    pub(super) fn child(&self, id: &str) -> Option<&Child> {
        self.children.get(id)
    }

    pub(super) fn containing_child(&self, id: &str) -> Option<&Child> {
        self.child(id)
            .or_else(|| {
                self.descendants
                    .get(id)
                    .and_then(|parent| self.child(parent))
            })
            .or_else(|| {
                self.quotes
                    .as_ref()
                    .and_then(|q| q.outer_root(id))
                    .and_then(|root| self.child(root))
            })
    }

    pub(super) fn previous_position(&self, anchor: &str) -> Result<Option<String>, Error> {
        // The actual display comparator uses positionPath, not placementPath.
        // getSibling_ skips other layouts, but a top-level ordinary section is
        // a boundary: it must not search past it for an older same-layout child.
        if let Some(quotes) = &self.quotes {
            if quotes.owner(anchor).is_some() {
                return quotes.previous_position(anchor);
            }
        }
        if let Some(columns) = &self.columns {
            if columns.owner(anchor).is_some() {
                return columns.previous_position(anchor);
            }
        }
        self.previous.get(anchor).cloned().ok_or(Error::Changed)
    }

    /// Additional canonical reachability check. This is not a comment
    /// permission check and creates no intent, ID, native proof or request.
    pub(super) fn admit_child(&self, section: &Section) -> Result<&Child, Error> {
        let child = self.child(section.id()).ok_or(Error::Changed)?;
        let table = &self.originals[self
            .originals
            .binary_search_by(|s| s.id().cmp(child.table.as_str()))
            .map_err(|_| Error::Changed)?];
        let original = &self.originals[self
            .originals
            .binary_search_by(|s| s.id().cmp(section.id()))
            .map_err(|_| Error::Changed)?];
        if original != section
            || child.merged_out
            || table.deleted()? == Some(true)
            || table.read_only()? == Some(true)
            || section.deleted()? == Some(true)
            || section.read_only()? == Some(true)
        {
            return Err(Error::Changed);
        }
        Ok(child)
    }

    pub(super) fn layout_owner(&self, id: &str) -> Option<&str> {
        self.quotes
            .as_ref()
            .and_then(|q| q.owner(id))
            .or_else(|| {
                self.columns
                    .as_ref()
                    .and_then(|c| c.owner(id))
                    .map(|c| c.root.as_str())
            })
            .or_else(|| self.child(id).map(|c| c.table.as_str()))
    }
    pub(super) fn owns_body(&self, id: &str) -> bool {
        self.child(id).is_some()
            || self.quotes.as_ref().is_some_and(|q| q.owner(id).is_some())
            || self.columns.as_ref().is_some_and(|c| c.owner(id).is_some())
    }
    pub(super) fn admit_body(&self, section: &Section) -> Result<(), Error> {
        if self.child(section.id()).is_some() {
            self.admit_child(section)?;
            return Ok(());
        }
        if let Some(columns) = &self.columns {
            if columns.owner(section.id()).is_some() {
                return columns.admit_body(section);
            }
        }
        let quotes = self.quotes.as_ref().ok_or(Error::Changed)?;
        if quotes.owner(section.id()).is_none() {
            return Err(Error::Changed);
        }
        quotes.live_chain(section)?;
        let root = quotes
            .outer_root(section.id())
            .and_then(|id| quotes.root_section(id))
            .ok_or(Error::Changed)?;
        if root.raw().boolean(29)? == Some(true) {
            if let Some(columns) = &self.columns
                && columns.owner(root.id()).is_some()
            {
                columns.admit_body(root)?;
            } else {
                self.admit_child(root)?;
            }
        }
        Ok(())
    }
    pub(super) fn current(&self, sections: &[Section]) -> Result<(), Error> {
        self.compare(sections, None)
    }

    pub(super) fn observe(&self, sections: &[Section]) -> Result<Observation, Error> {
        let ids: BTreeSet<_> = self.originals.iter().map(|s| s.id()).collect();
        let mut rows = Vec::new();
        let mut seen = BTreeSet::new();
        for section in sections {
            if !seen.insert(section.id()) {
                return Err(Error::Changed);
            }
            if ids.contains(section.id())
                || section.section_type()? == Some(33)
                || (self.quotes.is_some() && section.section_type()? == Some(71))
                || (self.columns.is_some() && matches!(section.section_type()?, Some(26 | 69)))
                || section.raw().boolean(29)? == Some(true)
                || self.related_descendant(section)?
            {
                rows.push(section.clone());
            }
        }
        rows.sort_by(|a, b| a.id().cmp(b.id()));
        let value = Observation {
            sections: rows.into_iter().map(|s| s.raw().raw().to_vec()).collect(),
        };
        self.validate_observation(&value)?;
        Ok(value)
    }

    fn related_descendant(&self, section: &Section) -> Result<bool, Error> {
        if !self.bodies {
            return Ok(false);
        }
        let parent = section.parents()?;
        let id = parent.as_ref().map(|p| p.string(1)).transpose()?.flatten();
        Ok(id.is_some_and(|id| {
            self.children.contains_key(id)
                || self.quotes.as_ref().is_some_and(|q| q.owner(id).is_some())
                || self.columns.as_ref().is_some_and(|c| c.owner(id).is_some())
        }))
    }

    pub(super) fn validate_observation(&self, value: &Observation) -> Result<(), Error> {
        let anchor = self.originals.first().ok_or(Error::Changed)?;
        let ids: BTreeSet<_> = self.originals.iter().map(|s| s.id()).collect();
        let mut previous = None;
        let mut bytes = 0usize;
        if value.sections.len() > crate::canvas::wire::MAX_SECTIONS {
            return Err(Error::Unsupported);
        }
        for raw in &value.sections {
            bytes = bytes.checked_add(raw.len()).ok_or(Error::Unsupported)?;
            if bytes > crate::canvas::wire::MAX_BYTES {
                return Err(Error::Unsupported);
            }
            let section = Section::decode(raw)?;
            if previous.as_deref().is_some_and(|id| id >= section.id())
                || section.thread_id()? != anchor.thread_id()?
                || section.document_id()? != anchor.document_id()?
                || !(ids.contains(section.id())
                    || section.section_type()? == Some(33)
                    || (self.quotes.is_some() && section.section_type()? == Some(71))
                    || (self.columns.is_some() && matches!(section.section_type()?, Some(26 | 69)))
                    || section.raw().boolean(29)? == Some(true)
                    || self.related_descendant(&section)?)
            {
                return Err(Error::Changed);
            }
            previous = Some(section.id().to_owned());
        }
        // Missing or changed layout records are valid review evidence. This
        // cannot create a new intent or satisfy canonical confirmation.
        Ok(())
    }

    // The selected body's ACK/history is checked by the caller's existing
    // exact body proof. No other child or table may change in that transition.
    pub(super) fn current_except_anchor(
        &self,
        sections: &[Section],
        anchor: &str,
    ) -> Result<(), Error> {
        if !self.owns_body(anchor) {
            return Err(Error::Changed);
        }
        self.compare(sections, Some(anchor))
    }
    fn compare(&self, sections: &[Section], anchor: Option<&str>) -> Result<(), Error> {
        if let Some(quotes) = &self.quotes {
            quotes
                .current_except_anchor(sections, anchor.filter(|id| quotes.owner(id).is_some()))?;
        }
        if let Some(columns) = &self.columns {
            columns
                .current_except_anchor(sections, anchor.filter(|id| columns.owner(id).is_some()))?;
        }
        let now = Self::capture_mode(
            sections,
            self.bodies,
            self.quotes.is_some(),
            self.columns.is_some(),
        )?;
        if self
            .originals
            .iter()
            .filter(|s| Some(s.id()) != anchor)
            .ne(now.originals.iter().filter(|s| Some(s.id()) != anchor))
            || self.children != now.children
            || self.descendants != now.descendants
        {
            return Err(Error::Changed);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
