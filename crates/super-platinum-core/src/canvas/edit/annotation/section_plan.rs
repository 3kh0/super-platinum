//! Exact whole-section cleanup evidence. This inert record cannot recreate a
//! live AnnotationIntent, PreparedAnnotation, or permission to replay a write.
use super::control_batch::ControlBatch;
use super::control_topology::ControlTopology;
use super::control_update::ControlUpdate;
use super::section_inventory::SectionControls;
use super::{AnnotationPresentation, CanvasCollabSnapshot, Error, RawMessage, Section};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SectionPlan {
    workspace: String,
    user: String,
    file: String,
    anchor: String,
    // Complete canonical order/body topology, not the visible viewport.
    sections: Vec<Vec<u8>>,
    document: Vec<u8>,
    presentation: AnnotationPresentation,
    batch: Option<ControlBatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    body: Option<super::inline::Whole>,
    #[serde(default, skip_serializing_if = "is_false")]
    list_layout: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    media_layout: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    table_layout: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    table_bodies: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    quote_layout: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    column_layout: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    image_export: Option<super::ImageExport>,
}
fn is_false(value: &bool) -> bool {
    !*value
}

impl SectionPlan {
    pub(super) fn capture(
        snapshot: &CanvasCollabSnapshot,
        anchor: &str,
        presentation: &AnnotationPresentation,
    ) -> Result<Self, Error> {
        Self::capture_inner(snapshot, anchor, presentation, None)
    }
    pub(super) fn capture_image(
        snapshot: &CanvasCollabSnapshot,
        anchor: &str,
        presentation: &AnnotationPresentation,
        export: super::ImageExport,
    ) -> Result<Self, Error> {
        Self::capture_inner(snapshot, anchor, presentation, Some(export))
    }
    fn capture_inner(
        snapshot: &CanvasCollabSnapshot,
        anchor: &str,
        presentation: &AnnotationPresentation,
        image_export: Option<super::ImageExport>,
    ) -> Result<Self, Error> {
        let media_layout = snapshot
            .sections
            .iter()
            .any(|s| matches!(s.section_type().ok().flatten(), Some(2 | 16)));
        let mut value = Self {
            workspace: snapshot.init.workspace_id().into(),
            user: snapshot
                .init
                .options()
                .string(33)?
                .ok_or(Error::Changed)?
                .into(),
            file: snapshot.file_id().into(),
            anchor: anchor.into(),
            sections: snapshot
                .sections
                .iter()
                .map(|s| s.raw().raw().to_vec())
                .collect(),
            document: snapshot.document.raw().to_vec(),
            presentation: presentation.clone(),
            batch: None,
            media_layout,
            table_layout: snapshot
                .sections
                .iter()
                .any(|s| s.section_type().ok() == Some(Some(33))),
            table_bodies: super::table_layout::Tables::has_bodies(&snapshot.sections)?,
            quote_layout: snapshot
                .sections
                .iter()
                .any(|s| s.section_type().ok() == Some(Some(71))),
            column_layout: snapshot
                .sections
                .iter()
                .any(|s| matches!(s.section_type().ok().flatten(), Some(26 | 69))),
            image_export,
            list_layout: snapshot.sections.iter().any(|s| {
                s.deleted().ok() != Some(Some(true)) && s.section_type().ok() == Some(Some(1))
            }),
            body: if snapshot
                .section(anchor)
                .ok_or(Error::Changed)?
                .section_type()?
                .is_some_and(|kind| matches!(kind, 1 | 2))
            {
                None
            } else {
                super::inline::Whole::capture(
                    snapshot,
                    snapshot.section(anchor).ok_or(Error::Changed)?,
                    presentation,
                )?
            },
        };
        value.batch = value.derive_batch()?;
        value.validate()?;
        Ok(value)
    }

    fn derive_batch(&self) -> Result<Option<ControlBatch>, Error> {
        self.presentation.validate()?;
        for id in [&self.workspace, &self.user, &self.file, &self.anchor] {
            if id.is_empty() || id.len() > 256 || id.chars().any(char::is_control) {
                return Err(Error::Unsupported);
            }
        }
        if self.sections.len() > crate::canvas::wire::MAX_SECTIONS
            || self
                .sections
                .iter()
                .try_fold(self.document.len(), |n, raw| n.checked_add(raw.len()))
                .is_none_or(|n| n > crate::canvas::wire::MAX_BYTES)
        {
            return Err(Error::Unsupported);
        }
        let sections = self
            .sections
            .iter()
            .map(|raw| Section::decode(raw).map_err(Into::into))
            .collect::<Result<Vec<_>, Error>>()?;
        let anchor = sections
            .iter()
            .find(|s| s.id() == self.anchor)
            .ok_or(Error::Changed)?;
        if self.layout_custody() {
            let tables = self.tables(&sections)?;
            if anchor.raw().boolean(29)? == Some(true) {
                tables.admit_body(anchor)?;
                super::section_body_shape(anchor)?;
            } else {
                super::section_shape(anchor)?;
            }
        } else {
            super::section_shape(anchor)?;
        }
        let thread = anchor.thread_id()?.ok_or(Error::Unsupported)?;
        let document_id = anchor.document_id()?.ok_or(Error::Unsupported)?;
        let document = RawMessage::decode(&self.document)?;
        if document.string(1)? != Some(document_id) || document.string(6)? != Some(thread) {
            return Err(Error::Changed);
        }
        if self.list_layout
            != sections.iter().any(|s| {
                s.deleted().ok() != Some(Some(true)) && s.section_type().ok() == Some(Some(1))
            })
        {
            return Err(Error::Changed);
        }
        if self.media_layout
            != sections
                .iter()
                .any(|s| matches!(s.section_type().ok().flatten(), Some(2 | 16)))
            || self.image_export.is_some() != (anchor.section_type()? == Some(2))
        {
            return Err(Error::Changed);
        }
        if self.table_layout
            != sections
                .iter()
                .any(|s| s.section_type().ok() == Some(Some(33)))
        {
            return Err(Error::Changed);
        }
        if self.quote_layout
            != sections
                .iter()
                .any(|s| s.section_type().ok() == Some(Some(71)))
        {
            return Err(Error::Changed);
        }
        if self.column_layout
            != sections
                .iter()
                .any(|s| matches!(s.section_type().ok().flatten(), Some(26 | 69)))
        {
            return Err(Error::Changed);
        }
        if self.table_bodies != super::table_layout::Tables::has_bodies(&sections)?
            || (self.table_bodies && !self.layout_custody())
        {
            return Err(Error::Changed);
        }
        let topology = if self.column_layout {
            ControlTopology::from_sections_with_columns(&sections, thread, document_id)?
        } else if self.quote_layout {
            ControlTopology::from_sections_with_quotes(&sections, thread, document_id)?
        } else if self.table_bodies {
            ControlTopology::from_sections_with_table_bodies(&sections, thread, document_id)?
        } else if self.layout_custody() {
            ControlTopology::from_sections_with_tables(&sections, thread, document_id)?
        } else if self.media_layout {
            ControlTopology::from_sections_with_media(&sections, thread, document_id)?
        } else if self.list_layout {
            ControlTopology::from_sections_with_lists(&sections, thread, document_id)?
        } else {
            ControlTopology::from_sections(&sections, thread, document_id)?
        };
        let body = if anchor.section_type()? == Some(2) {
            super::image_body::shape_with_layout(anchor, self.table_bodies)?;
            None
        } else if anchor.section_type()? == Some(1) {
            if self.column_layout {
                super::list_body::ListBody::capture_columns(&sections, anchor)?;
            } else if self.quote_layout {
                super::list_body::ListBody::capture_quotes(&sections, anchor)?;
            } else if self.table_bodies {
                super::list_body::ListBody::capture_admitted(&sections, anchor)?;
            } else {
                super::list_body::ListBody::capture(&sections, anchor)?;
            }
            None
        } else {
            super::inline::Whole::from_parts(&sections, &document, anchor, &self.presentation)?
        };
        if body != self.body {
            return Err(Error::Changed);
        }
        // Retained fresh Document.update processes controls in incoming order;
        // SectionHighlights.add is append-if-absent. Never sort these by ID.
        let order = registry_order(&sections, &self.anchor)?;
        let controls = SectionControls::from_parts(
            &sections,
            &document,
            anchor,
            (self.workspace.clone(), self.user.clone(), self.file.clone()),
            &order,
            self.presentation.clone(),
        )?;
        // The getOrCreate route must reuse the existing visible/unpositioned
        // root. Such a record must never encode a newly minted replacement.
        if controls.existing().is_some() {
            return Err(Error::Unchanged);
        }
        let updates = controls
            .hidden()
            .map(|(control, metadata)| {
                ControlUpdate::capture(
                    control,
                    metadata,
                    anchor,
                    topology.after_clear(control.id())?,
                )
            })
            .collect::<Result<Vec<_>, Error>>()?;
        if updates.is_empty() {
            Ok(None)
        } else {
            ControlBatch::capture(updates).map(Some)
        }
    }

    pub(super) fn validate(&self) -> Result<(), Error> {
        if self.derive_batch()? != self.batch {
            return Err(Error::Unsupported);
        }
        Ok(())
    }

    pub(super) fn validate_for(
        &self,
        workspace: &str,
        user: &str,
        file: &str,
        anchor: &Section,
    ) -> Result<(), Error> {
        if self.workspace != workspace
            || self.user != user
            || self.file != file
            || self.anchor != anchor.id()
            || self
                .sections
                .iter()
                .find(|raw| {
                    Section::decode(raw)
                        .ok()
                        .is_some_and(|s| s.id() == self.anchor)
                })
                .map(Vec::as_slice)
                != Some(anchor.raw().raw())
        {
            return Err(Error::Changed);
        }
        self.validate()
    }

    pub(super) fn media_layout(&self) -> bool {
        self.media_layout
    }
    pub(super) fn table_bodies(&self) -> bool {
        self.table_bodies
    }
    pub(super) fn tables(
        &self,
        sections: &[Section],
    ) -> Result<super::table_layout::Tables, Error> {
        if !self.layout_custody() {
            return Err(Error::Unsupported);
        }
        if self.column_layout {
            super::table_layout::Tables::capture_columns(sections)
        } else if self.quote_layout {
            super::table_layout::Tables::capture_quotes(sections)
        } else if self.table_bodies {
            super::table_layout::Tables::capture_bodies(sections)
        } else {
            super::table_layout::Tables::capture(sections)
        }
    }
    fn list_body<'a>(
        &self,
        sections: &'a [Section],
        anchor: &'a Section,
    ) -> Result<super::list_body::ListBody<'a>, Error> {
        if self.column_layout {
            super::list_body::ListBody::capture_columns(sections, anchor)
        } else if self.quote_layout {
            super::list_body::ListBody::capture_quotes(sections, anchor)
        } else if self.table_bodies {
            super::list_body::ListBody::capture_admitted(sections, anchor)
        } else {
            super::list_body::ListBody::capture(sections, anchor)
        }
    }

    pub(super) fn column_layout(&self) -> bool {
        self.column_layout
    }
    pub(super) fn quote_layout(&self) -> bool {
        self.quote_layout
    }
    pub(super) fn layout_custody(&self) -> bool {
        self.table_layout || self.quote_layout || self.column_layout
    }
    pub(super) fn table_layout(&self) -> bool {
        self.table_layout
    }
    pub(super) fn loaded_layout(&self) -> bool {
        self.layout_custody() || self.media_layout || self.list_layout
    }
    pub(super) fn image_snippet(
        &self,
        control: &str,
        quip: &str,
        slack: &str,
    ) -> Result<serde_json::Value, Error> {
        let sections = self.original_sections()?;
        let anchor = sections
            .iter()
            .find(|s| s.id() == self.anchor)
            .ok_or(Error::Changed)?;
        let export = self.image_export.ok_or(Error::Unsupported)?;
        super::image_body::shape_with_layout(anchor, self.table_bodies)?;
        super::image_body::ImageBody::metadata(anchor)?
            .snippet(anchor, control, quip, slack, export)
    }

    pub(super) fn list_layout(&self) -> bool {
        self.list_layout
    }

    pub(super) fn list_anchor(&self) -> bool {
        self.list_layout
            && self.sections.iter().any(|raw| {
                Section::decode(raw).ok().is_some_and(|s| {
                    s.id() == self.anchor && s.section_type().ok() == Some(Some(1))
                })
            })
    }

    fn original_sections(&self) -> Result<Vec<Section>, Error> {
        self.sections
            .iter()
            .map(|raw| Section::decode(raw).map_err(Into::into))
            .collect()
    }

    pub(super) fn observe_children(
        &self,
        fresh: &CanvasCollabSnapshot,
    ) -> Result<super::list_body::Observation, Error> {
        if !self.list_anchor() {
            return Err(Error::Unsupported);
        }
        let sections = self.original_sections()?;
        let anchor = sections
            .iter()
            .find(|s| s.id() == self.anchor)
            .ok_or(Error::Changed)?;
        super::list_body::Observation::capture(&fresh.sections, anchor)
    }

    pub(super) fn validate_children_observation(
        &self,
        observed: &super::list_body::Observation,
    ) -> Result<(), Error> {
        if !self.list_anchor() {
            return Err(Error::Unsupported);
        }
        let sections = self.original_sections()?;
        let anchor = sections
            .iter()
            .find(|s| s.id() == self.anchor)
            .ok_or(Error::Changed)?;
        observed.validate(anchor)
    }

    pub(super) fn children_current(&self, fresh: &CanvasCollabSnapshot) -> Result<(), Error> {
        if self.layout_custody() {
            let tables = self.tables(&self.original_sections()?)?;
            if tables.owns_body(&self.anchor) {
                tables.current_except_anchor(&fresh.sections, &self.anchor)?;
            } else {
                tables.current(&fresh.sections)?;
            }
        }
        if !self.list_anchor() {
            return Ok(());
        }
        let sections = self.original_sections()?;
        let anchor = sections
            .iter()
            .find(|s| s.id() == self.anchor)
            .ok_or(Error::Changed)?;
        let original = self.list_body(&sections, anchor)?;
        let now = self.list_body(
            &fresh.sections,
            fresh.section(&self.anchor).ok_or(Error::Changed)?,
        )?;
        // Export intentionally omits deleted children. Confirmation cannot:
        // their original wrappers and positions participate in v8 topology.
        // Compare every direct child's exact canonical bytes, keyed by ID;
        // incoming response row order is not the position-path order.
        let mut original_children =
            super::list_body::Observation::capture(&sections, anchor)?.sections(anchor)?;
        let current_anchor = fresh.section(&self.anchor).ok_or(Error::Changed)?;
        let mut current_children =
            super::list_body::Observation::capture(&fresh.sections, current_anchor)?
                .sections(current_anchor)?;
        original_children.sort_by(|a, b| a.id().cmp(b.id()));
        current_children.sort_by(|a, b| a.id().cmp(b.id()));
        if original.style != now.style || original_children != current_children {
            return Err(Error::Changed);
        }
        Ok(())
    }

    pub(super) fn observe_table(
        &self,
        fresh: &CanvasCollabSnapshot,
    ) -> Result<super::table_layout::Observation, Error> {
        if !self.layout_custody() {
            return Err(Error::Unsupported);
        }
        self.tables(&self.original_sections()?)?
            .observe(&fresh.sections)
    }

    pub(super) fn validate_table_observation(
        &self,
        value: &super::table_layout::Observation,
    ) -> Result<(), Error> {
        if !self.layout_custody() {
            return Err(Error::Unsupported);
        }
        self.tables(&self.original_sections()?)?
            .validate_observation(value)
    }

    pub(super) fn list_snippet(
        &self,
        control: &str,
        quip_user: &str,
        slack_user: &str,
    ) -> Result<serde_json::Value, Error> {
        if !self.list_layout {
            return Err(Error::Unsupported);
        }
        let sections = self
            .sections
            .iter()
            .map(|raw| Section::decode(raw).map_err(Into::into))
            .collect::<Result<Vec<_>, Error>>()?;
        let anchor = sections
            .iter()
            .find(|s| s.id() == self.anchor)
            .ok_or(Error::Changed)?;
        if self.column_layout {
            super::snippet::list_capture_columns(&sections, anchor, control, quip_user, slack_user)
        } else if self.quote_layout {
            super::snippet::list_capture_quotes(&sections, anchor, control, quip_user, slack_user)
        } else if self.table_bodies {
            super::snippet::list_capture_admitted(&sections, anchor, control, quip_user, slack_user)
        } else {
            super::snippet::list_capture(&sections, anchor, control, quip_user, slack_user)
        }
    }

    pub(super) fn batch(&self) -> Option<&ControlBatch> {
        self.batch.as_ref()
    }

    pub(super) fn body(&self) -> Option<&super::inline::Whole> {
        self.body.as_ref()
    }

    /// Existing entities retain the captured Document entitiesById iteration
    /// order. The caller appends the new control only after these updates.
    pub(super) fn requests(
        &self,
        session: &str,
        parent: Option<&RawMessage>,
    ) -> Result<Vec<RawMessage>, Error> {
        if self.body.as_ref().is_some_and(|b| b.changes_body()) != parent.is_some()
            || parent.is_some_and(|p| p.string(1).ok().flatten() != Some(self.anchor.as_str()))
        {
            return Err(Error::Unsupported);
        }
        let mut updates = self
            .batch
            .as_ref()
            .map(|b| b.requests(session))
            .transpose()?
            .unwrap_or_default();
        if let Some(parent) = parent {
            updates.push(parent.clone());
        }
        let mut ordered = Vec::with_capacity(updates.len());
        for raw in &self.sections {
            let section = Section::decode(raw)?;
            if let Some(index) = updates
                .iter()
                .position(|p| p.string(1).ok().flatten() == Some(section.id()))
            {
                ordered.push(updates.remove(index));
            }
        }
        if !updates.is_empty() {
            return Err(Error::Changed);
        }
        Ok(ordered)
    }

    pub(super) fn current(&self, snapshot: &CanvasCollabSnapshot) -> Result<(), Error> {
        if snapshot.init.workspace_id() != self.workspace
            || snapshot.init.options().string(33)? != Some(self.user.as_str())
            || snapshot.file_id() != self.file
            || snapshot.document.raw() != self.document
            || snapshot.sections.len() != self.sections.len()
            || snapshot
                .sections
                .iter()
                .zip(&self.sections)
                .any(|(s, raw)| s.raw().raw() != raw)
        {
            return Err(Error::Changed);
        }
        self.validate()
    }
}

fn registry_order(sections: &[Section], anchor: &str) -> Result<Vec<String>, Error> {
    let mut order = Vec::new();
    for section in sections {
        if section.deleted()? == Some(true) || section.section_type()? != Some(9) {
            continue;
        }
        if let Some(content) = section.raw().message(12)?
            && let Some(highlight) = content.message(7)?
            && super::strings(&highlight, 3)?.iter().any(|id| id == anchor)
        {
            order.push(section.id().to_owned());
        }
    }
    Ok(order)
}

/// Read-only existing root selection. It creates neither a new annotation nor
/// a saved operation; the desktop still resolves its actual numeric thread.
pub(super) fn existing(
    snapshot: &CanvasCollabSnapshot,
    anchor: &str,
    presentation: &AnnotationPresentation,
) -> Result<Option<String>, Error> {
    let inventory = SectionControls::capture(
        snapshot,
        anchor,
        &registry_order(&snapshot.sections, anchor)?,
        presentation.clone(),
    )?;
    Ok(inventory.existing().map(|control| control.id().to_owned()))
}
