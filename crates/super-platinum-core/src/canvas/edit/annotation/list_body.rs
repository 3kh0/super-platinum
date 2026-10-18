//! Canonical ordinary ListContent and its exact ordered TextContent children.
//! This is read-only body inventory; list annotations never rewrite descendants.
use super::{Error, Section, inline, position};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Inert exact current child bytes for explicit local review, including a
/// child deleted or changed after the original list capture. Never a send proof.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Observation {
    children: Vec<Vec<u8>>,
}

impl Observation {
    pub(super) fn sections(&self, anchor: &Section) -> Result<Vec<Section>, Error> {
        self.validate(anchor)?;
        self.children
            .iter()
            .map(|raw| Section::decode(raw).map_err(Into::into))
            .collect()
    }
    pub(super) fn capture(sections: &[Section], anchor: &Section) -> Result<Self, Error> {
        let mut children = Vec::new();
        for section in sections {
            if section
                .parents()?
                .map(|p| p.string(1).map(|id| id == Some(anchor.id())))
                .transpose()?
                .unwrap_or(false)
            {
                children.push(section.raw().raw().to_vec());
            }
        }
        let value = Self { children };
        value.validate(anchor)?;
        Ok(value)
    }
    pub(super) fn validate(&self, anchor: &Section) -> Result<(), Error> {
        if self.children.len() > crate::canvas::wire::MAX_SECTIONS
            || self
                .children
                .iter()
                .try_fold(0usize, |sum, raw| sum.checked_add(raw.len()))
                .is_none_or(|n| n > crate::canvas::wire::MAX_BYTES)
        {
            return Err(Error::Unsupported);
        }
        let mut ids = std::collections::BTreeSet::new();
        for raw in &self.children {
            let section = Section::decode(raw)?;
            if !ids.insert(section.id().to_owned())
                || section.id() == anchor.id()
                || section.thread_id()? != anchor.thread_id()?
                || section.document_id()? != anchor.document_id()?
                || section
                    .parents()?
                    .map(|p| p.string(1).map(|id| id != Some(anchor.id())))
                    .transpose()?
                    .unwrap_or(true)
            {
                return Err(Error::Changed);
            }
        }
        Ok(())
    }
}

pub(super) struct ListBody<'a> {
    pub(super) style: u64,
    pub(super) children: Vec<Child<'a>>,
}
pub(super) struct Child<'a> {
    pub(super) section: &'a Section,
    pub(super) indent: usize,
    pub(super) checked: bool,
    pub(super) default_content: bool,
}

pub(super) fn shape(section: &Section) -> Result<(), Error> {
    shape_with_layout(section, false)
}
pub(super) fn shape_with_layout(section: &Section, layout: bool) -> Result<(), Error> {
    inventory_shape(section, layout)?;
    if section.deleted()? == Some(true) || section.read_only()? == Some(true) {
        return Err(Error::Unsupported);
    }
    Ok(())
}
pub(super) fn inventory_shape(section: &Section, layout: bool) -> Result<(), Error> {
    if section.section_type()? != Some(1)
        || !matches!(section.style()?, Some(5..=7))
        || section.raw().uint(11)?.unwrap_or(0) != 0
        || section.parents()?.is_some_and(|p| !p.fields().is_empty())
        || (!layout && section.raw().boolean(29)? == Some(true))
        || section.raw().boolean(36)? == Some(true)
        || section.raw().has(15)
        || section.crdt()?.is_some()
        || section
            .raw()
            .message(12)?
            .is_none_or(|p| !p.fields().is_empty())
        || section
            .sequence()?
            .is_none_or(|s| s == 0 || s > i64::MAX as u64)
    {
        return Err(Error::Unsupported);
    }
    let path = position::base(section.raw().string(8)?.ok_or(Error::Unsupported)?)?;
    if section.raw().string(21)?.is_some_and(|v| v != path) {
        return Err(Error::Unsupported);
    }
    Ok(())
}

pub(super) fn default_content(section: &Section) -> Result<bool, Error> {
    Ok(section
        .raw()
        .message(16)?
        .map(|a| a.uint(13))
        .transpose()?
        .flatten()
        .is_some_and(|n| n != 0))
}

impl<'a> ListBody<'a> {
    pub(super) fn capture(sections: &'a [Section], anchor: &'a Section) -> Result<Self, Error> {
        Self::capture_inner(sections, anchor, false)
    }
    pub(super) fn capture_admitted(
        sections: &'a [Section],
        anchor: &'a Section,
    ) -> Result<Self, Error> {
        let layout = anchor.raw().boolean(29)? == Some(true);
        if layout {
            super::table_layout::Tables::capture_bodies(sections)?.admit_child(anchor)?;
        }
        Self::capture_inner(sections, anchor, layout)
    }
    pub(super) fn capture_quotes(
        sections: &'a [Section],
        anchor: &'a Section,
    ) -> Result<Self, Error> {
        let layout = anchor.raw().boolean(29)? == Some(true);
        if layout {
            super::table_layout::Tables::capture_quotes(sections)?.admit_body(anchor)?;
        }
        Self::capture_inner(sections, anchor, layout)
    }
    pub(super) fn capture_columns(
        sections: &'a [Section],
        anchor: &'a Section,
    ) -> Result<Self, Error> {
        let layout = anchor.raw().boolean(29)? == Some(true);
        if layout {
            super::table_layout::Tables::capture_columns(sections)?.admit_body(anchor)?;
        }
        Self::capture_inner(sections, anchor, layout)
    }
    // Local review may show a body whose table changed or disappeared. This
    // inert export never creates an intent or supplies canonical confirmation.
    pub(super) fn capture_observed(
        sections: &'a [Section],
        anchor: &'a Section,
    ) -> Result<Self, Error> {
        Self::capture_inner(sections, anchor, true)
    }
    fn capture_inner(
        sections: &'a [Section],
        anchor: &'a Section,
        layout: bool,
    ) -> Result<Self, Error> {
        shape_with_layout(anchor, layout)?;
        let style = anchor.style()?.ok_or(Error::Unsupported)?;
        let parent_position = position::base(anchor.raw().string(8)?.ok_or(Error::Unsupported)?)?;
        let mut children = BTreeMap::new();
        for section in sections {
            if section.deleted()? == Some(true) {
                continue;
            }
            let Some(parents) = section.parents()?.filter(|p| !p.fields().is_empty()) else {
                continue;
            };
            // Repeated IDs or additional ancestors are not a single ordinary
            // list parent; string() rejects repeated field-1 identities.
            if parents.string(1)? != Some(anchor.id()) {
                continue;
            }
            if parents.fields().iter().any(|f| !matches!(f.tag(), 1 | 4))
                || parents.uint(4)? != Some(style)
                || section.section_type()?.unwrap_or(0) != 0
                || section.style()?.unwrap_or(0) != 0
                || section.raw().uint(11)?.unwrap_or(0) != 0
                || section.raw().boolean(36)? == Some(true)
                || section.raw().boolean(29)? == Some(true)
                || section.raw().has(15)
                || section.document_id()? != anchor.document_id()?
                || section.thread_id()? != anchor.thread_id()?
                || section
                    .sequence()?
                    .is_none_or(|n| n == 0 || n > i64::MAX as u64)
            {
                return Err(Error::Unsupported);
            }
            let position = position::base(section.raw().string(8)?.ok_or(Error::Unsupported)?)?;
            if parent_position >= position
                || section.raw().string(21)?
                    != Some(format!("{parent_position}-{position}").as_str())
            {
                return Err(Error::Unsupported);
            }
            // Canonical inline wrappers are retained by the original parser;
            // their export-only omission never admits a descendant body write.
            inline::original(section)?;
            let attributes = section.raw().message(16)?;
            let indent = attributes
                .as_ref()
                .map(|a| a.uint(1))
                .transpose()?
                .flatten()
                .unwrap_or(0);
            // Bounded nested export work, not a claim about Slack's UI limit.
            if indent > 64 {
                return Err(Error::Unsupported);
            }
            let checked = attributes
                .as_ref()
                .map(|a| a.boolean(2))
                .transpose()?
                .flatten()
                .unwrap_or(false);
            let child = Child {
                section,
                indent: indent as usize,
                checked,
                default_content: default_content(anchor)? || default_content(section)?,
            };
            if children.insert(position.to_owned(), child).is_some() {
                return Err(Error::Unsupported);
            }
        }
        Ok(Self {
            style,
            children: children.into_values().collect(),
        })
    }
}
