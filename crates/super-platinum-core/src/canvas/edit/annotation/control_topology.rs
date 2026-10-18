//! Complete-load control placement, separate from section-highlight membership.
//! Clearing membership does not detach a control that is still in body RTML.
//! This model is inert: it supplies no operation, transport, or replay authority.
use super::{CanvasCollabSnapshot, Error, Section, inline, position};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct ControlTopology {
    originals: Vec<Section>,
    list_layout: bool,
    media_layout: bool,
    table_layout: bool,
    table_bodies: bool,
    quote_layout: bool,
    column_layout: bool,
    // Closed canonical TextContent references, never an HTML export or altRTML.
    containing: BTreeMap<String, String>,
    positions: BTreeMap<String, String>,
}

impl ControlTopology {
    pub(super) fn capture(snapshot: &CanvasCollabSnapshot) -> Result<Self, Error> {
        Self::from_sections(
            &snapshot.sections,
            snapshot.thread_id(),
            snapshot.document_id(),
        )
    }

    pub(super) fn from_sections(
        sections: &[Section],
        thread: &str,
        document: &str,
    ) -> Result<Self, Error> {
        Self::from_parts(
            sections, thread, document, false, false, false, false, false, false,
        )
    }

    pub(super) fn from_sections_with_lists(
        sections: &[Section],
        thread: &str,
        document: &str,
    ) -> Result<Self, Error> {
        Self::from_parts(
            sections, thread, document, true, false, false, false, false, false,
        )
    }

    pub(super) fn from_sections_with_media(
        sections: &[Section],
        thread: &str,
        document: &str,
    ) -> Result<Self, Error> {
        Self::from_parts(
            sections, thread, document, true, true, false, false, false, false,
        )
    }

    pub(super) fn from_sections_with_tables(
        sections: &[Section],
        thread: &str,
        document: &str,
    ) -> Result<Self, Error> {
        Self::from_parts(
            sections, thread, document, true, true, true, false, false, false,
        )
    }

    pub(super) fn from_sections_with_table_bodies(
        sections: &[Section],
        thread: &str,
        document: &str,
    ) -> Result<Self, Error> {
        Self::from_parts(
            sections, thread, document, true, true, true, true, false, false,
        )
    }

    pub(super) fn from_sections_with_quotes(
        sections: &[Section],
        thread: &str,
        document: &str,
    ) -> Result<Self, Error> {
        Self::from_parts(
            sections, thread, document, true, true, true, true, true, false,
        )
    }
    pub(super) fn from_sections_with_columns(
        sections: &[Section],
        thread: &str,
        document: &str,
    ) -> Result<Self, Error> {
        Self::from_parts(
            sections, thread, document, true, true, true, true, true, true,
        )
    }
    fn from_parts(
        sections: &[Section],
        thread: &str,
        document: &str,
        list_layout: bool,
        media_layout: bool,
        table_layout: bool,
        table_bodies: bool,
        quote_layout: bool,
        column_layout: bool,
    ) -> Result<Self, Error> {
        let tables = table_layout
            .then(|| {
                if column_layout {
                    super::table_layout::Tables::capture_columns(sections)
                } else if quote_layout {
                    super::table_layout::Tables::capture_quotes(sections)
                } else if table_bodies {
                    super::table_layout::Tables::capture_bodies(sections)
                } else {
                    super::table_layout::Tables::capture(sections)
                }
            })
            .transpose()?;
        let mut ids = BTreeSet::new();
        let mut containing = BTreeMap::new();
        let mut positions = BTreeMap::new();
        // Layout membership groups siblings, but is not part of the emitted
        // control path. Only ordinary Parents contribute that path prefix.
        let mut ordered: BTreeMap<(String, String), BTreeMap<String, String>> = BTreeMap::new();
        let mut bytes = 0usize;
        for section in sections {
            if !ids.insert(section.id()) {
                return Err(Error::Unsupported);
            }
            bytes = bytes
                .checked_add(section.raw().raw().len())
                .filter(|n| *n <= crate::canvas::wire::MAX_BYTES)
                .ok_or(Error::Unsupported)?;
            if section.thread_id()? != Some(thread) || section.document_id()? != Some(document) {
                return Err(Error::Changed);
            }
            let deleted = section.deleted()? == Some(true);
            if deleted && !list_layout {
                // Keep the historical v6/v7 inert derivation unchanged. New
                // list-aware plans use the loaded ALL sibling set below.
                continue;
            }
            if section.raw().uint(11)?.unwrap_or(0) == 1 {
                // An opaque/control container could own additional body
                // placement. Do not claim absence of references inside it.
                if section.section_type()? != Some(9)
                    || section
                        .raw()
                        .message(12)?
                        .is_none_or(|m| m.fields().len() != 1 || m.fields()[0].tag() != 7)
                {
                    return Err(Error::Unsupported);
                }
                continue;
            }
            // Version-8 plans admit ordinary top-level lists and their text
            // children. Older inert plans retain the closed flat topology.
            let kind = section.section_type()?.unwrap_or(0);
            if section.raw().uint(11)?.unwrap_or(0) != 0
                || (!matches!(kind, 0 | 64)
                    && !(list_layout && kind == 1)
                    && !(media_layout && matches!(kind, 2 | 16))
                    && !(table_layout && kind == 33)
                    && !(quote_layout && kind == 71)
                    && !(column_layout && matches!(kind, 26 | 69)))
                || section.raw().boolean(36)? == Some(true)
            {
                return Err(Error::Unsupported);
            }
            let path = position::base(section.raw().string(8)?.ok_or(Error::Unsupported)?)?;
            let parent = section.parents()?.filter(|p| !p.fields().is_empty());
            let prefix = if let Some(parent) = parent {
                if !list_layout
                    || kind != 0
                    || section.style()?.unwrap_or(0) != 0
                    || parent.fields().iter().any(|f| !matches!(f.tag(), 1 | 4))
                {
                    return Err(Error::Unsupported);
                }
                let parent_id = parent.string(1)?.ok_or(Error::Unsupported)?;
                let owner = sections
                    .iter()
                    .find(|s| s.id() == parent_id)
                    .ok_or(Error::Unsupported)?;
                if table_bodies && tables.as_ref().is_some_and(|t| t.owns_body(owner.id())) {
                    super::list_body::inventory_shape(owner, true)?;
                } else {
                    super::list_body::shape(owner)?;
                }
                if parent.uint(4)? != owner.style()? {
                    return Err(Error::Unsupported);
                }
                let p = position::base(owner.raw().string(8)?.ok_or(Error::Unsupported)?)?;
                if p >= path || section.raw().string(21)? != Some(format!("{p}-{path}").as_str()) {
                    return Err(Error::Unsupported);
                }
                p.to_owned()
            } else {
                if list_layout && section.raw().string(21)?.is_some_and(|v| v != path) {
                    return Err(Error::Unsupported);
                }
                String::new()
            };
            if ordered
                .entry((
                    prefix,
                    tables
                        .as_ref()
                        .and_then(|t| t.layout_owner(section.id()))
                        .map(str::to_owned)
                        .unwrap_or_default(),
                ))
                .or_default()
                .insert(path.to_owned(), section.id().to_owned())
                .is_some()
            {
                return Err(Error::Unsupported);
            }
            if (table_layout && kind == 33)
                || (quote_layout && kind == 71)
                || (column_layout && matches!(kind, 26 | 69))
            {
                // Tables own child placement but never have annotation roots.
                // The exact layout inventory above validated their raw shape.
                continue;
            }
            if media_layout && kind == 2 {
                super::image_body::inventory_shape_with_layout(
                    section,
                    table_bodies && tables.as_ref().is_some_and(|t| t.owns_body(section.id())),
                )?;
                continue;
            }
            if media_layout && kind == 16 {
                // HorizontalRule inherits empty referencedControls and forbids
                // section highlights. It still occupies an ordinary ALL sibling
                // position. Page-break presentation does not grant authoring.
                let content = section.raw().message(12)?.ok_or(Error::Unsupported)?;
                if section.style()? != Some(18)
                    || section.parents()?.is_some_and(|p| !p.fields().is_empty())
                    || section.raw().has(15)
                    || section.crdt()?.is_some()
                    || content.fields().iter().any(|f| f.tag() != 43)
                {
                    return Err(Error::Unsupported);
                }
                if let Some(rule) = content.message(43)? {
                    if rule.fields().iter().any(|f| f.tag() != 1) {
                        return Err(Error::Unsupported);
                    }
                    rule.boolean(1)?;
                }
                continue;
            }
            if kind == 1 {
                super::list_body::inventory_shape(
                    section,
                    table_bodies && tables.as_ref().is_some_and(|t| t.owns_body(section.id())),
                )?;
                if !(table_bodies && tables.as_ref().is_some_and(|t| t.owns_body(section.id()))) {
                    super::list_body::shape(section)?;
                }
                continue;
            }
            // TITLE_TYPE has its own plain title payload, never TextContent's
            // control-bearing RTML. It still participates in sibling position.
            // Match the existing canonical title decoder's closed content shape.
            if section.section_type()? == Some(64) {
                let content = section.raw().message(12)?.ok_or(Error::Unsupported)?;
                let title = content.message(58)?.ok_or(Error::Unsupported)?;
                if section.style()? != Some(48)
                    || content.fields().iter().any(|f| !matches!(f.tag(), 1 | 58))
                    || title.fields().iter().any(|f| !matches!(f.tag(), 1..=4))
                    || title.string(1)?.is_none()
                {
                    return Err(Error::Unsupported);
                }
                continue;
            }
            // In the new list-aware topology, deleted TextContent still has
            // its canonical wrapper: deleteEntity adds the deleted class;
            // findContainingSection has no reachability filter. Retain those
            // references too rather than declaring an attached control orphaned.
            let original = inline::original(section)?;
            if tables
                .as_ref()
                .and_then(|t| t.containing_child(section.id()))
                .is_some_and(|c| c.merged_out)
            {
                // Modern table setContentPb removes these children's DOM.
                // Their text cannot supply findContainingSection authority.
                continue;
            }
            for span in original.spans {
                let control = sections
                    .iter()
                    .find(|s| s.id() == span.id)
                    .ok_or(Error::Unsupported)?;
                if control.deleted()? == Some(true)
                    || control.section_type()? != Some(9)
                    || control.raw().uint(11)? != Some(1)
                    || containing
                        .insert(span.id, section.id().to_owned())
                        .is_some()
                {
                    return Err(Error::Unsupported);
                }
            }
        }
        // Section.positionPathForControl uses previousSibling(true), not only
        // the preceding annotatable paragraph. Every body section participates.
        for ((prefix, _layout), siblings) in &ordered {
            let mut before: Option<&str> = None;
            for (path, id) in siblings {
                let layout_previous =
                    if let Some(tables) = tables.as_ref().filter(|t| t.owns_body(id)) {
                        tables.previous_position(id)?
                    } else {
                        None
                    };
                let previous = if tables.as_ref().is_some_and(|t| t.owns_body(id)) {
                    layout_previous.as_deref()
                } else {
                    before
                };
                let interval = position::between(previous, Some(path))?;
                positions.insert(
                    id.clone(),
                    if prefix.is_empty() {
                        interval
                    } else {
                        format!("{prefix}-{interval}")
                    },
                );
                before = Some(path);
            }
        }
        Ok(Self {
            originals: sections.to_vec(),
            list_layout,
            media_layout,
            table_layout,
            table_bodies,
            quote_layout,
            column_layout,
            containing,
            positions,
        })
    }

    pub(super) fn after_clear(&self, control: &str) -> Result<String, Error> {
        if !self
            .originals
            .iter()
            .any(|s| s.id() == control && s.section_type().ok() == Some(Some(9)))
        {
            return Err(Error::Changed);
        }
        match self.containing.get(control) {
            None => Ok("zzzzzz-orphaned-m".into()),
            Some(section) => self.positions.get(section).cloned().ok_or(Error::Changed),
        }
    }

    pub(super) fn current(&self, snapshot: &CanvasCollabSnapshot) -> Result<(), Error> {
        if snapshot.sections != self.originals {
            return Err(Error::Changed);
        }
        let now = Self::from_parts(
            &snapshot.sections,
            snapshot.thread_id(),
            snapshot.document_id(),
            self.list_layout,
            self.media_layout,
            self.table_layout,
            self.table_bodies,
            self.quote_layout,
            self.column_layout,
        )?;
        if now.containing != self.containing || now.positions != self.positions {
            return Err(Error::Changed);
        }
        Ok(())
    }
}
