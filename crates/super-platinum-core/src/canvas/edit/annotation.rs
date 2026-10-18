//! Canonical Canvas annotations, independent from body edit permission.
//! Paragraph and inline selections retain distinct payload and ACK contracts.
use super::structure::position;
use super::{CanonicalTitle, CanvasCollabSnapshot, Error, F, RawMessage, Section};
use std::sync::Arc;

mod blockquote_layout;
mod column_layout;
mod control_batch;
#[cfg(test)]
mod control_tests;
mod control_topology;
mod control_update;
mod image_body;
mod inline;
mod list_body;
mod table_layout;
pub use image_body::{ImageAnnotationSeed, ImageExport, image_seed};
mod projection;
mod record;
pub use projection::{
    AnnotationParagraph, AnnotationRun, paragraph_previews, paragraph_seeds, range_seeds,
    range_seeds_with_presentation,
};
mod section_choices;
mod section_inventory;
mod section_plan;
mod section_policy;
#[cfg(test)]
mod section_tests;
mod selection;
mod snippet;
mod visibility;
pub use record::{AnnotationAcknowledgementEvidence, AnnotationObservation, AnnotationRecord};
pub use section_choices::{SectionAnnotationPreview, section_previews};
pub use visibility::AnnotationPresentation;

pub struct AnnotationIntent {
    snapshot: Arc<CanvasCollabSnapshot>,
    section: Section,
    title: CanonicalTitle,
    id: String,
    session: String,
    path: String,
    snippet: serde_json::Value,
    inline: Option<inline::Selection>,
    preserved: Option<inline::Preserved>,
    removal: Option<inline::Removal>,
    section_plan: Option<section_plan::SectionPlan>,
}
pub struct PreparedAnnotation {
    intent: AnnotationIntent,
    data: RawMessage,
    inline: Option<inline::Prepared>,
}
/// A direct ACK is retained separately from the fresh canonical observation.
pub struct AcknowledgedAnnotation {
    prepared: PreparedAnnotation,
    section: Section,
    anchor: Option<Section>,
    changed_controls: Option<control_batch::AcknowledgedControls>,
}
/// Only this proof admits the next, separately journaled Qpc bootstrap stage.
pub struct VerifiedAnnotation {
    prepared: PreparedAnnotation,
    section: Section,
    anchor: Option<Section>,
    changed_controls: Vec<Vec<u8>>,
}

fn comment_permission(snapshot: &CanvasCollabSnapshot) -> Result<(), Error> {
    let file = &snapshot.file;
    if !matches!(
        file.extra.get("access").and_then(serde_json::Value::as_str),
        Some("comment" | "write" | "owner")
    ) || ["is_locked", "is_deleted"].iter().any(|k| {
        file.extra
            .get(*k)
            .is_some_and(|v| v != &serde_json::Value::Bool(false))
    }) || snapshot.thread.boolean(119)? != Some(true)
        || snapshot.thread.boolean(303)? == Some(true)
        || snapshot.thread.boolean(105)? == Some(true)
        || !snapshot
            .thread
            .uints(23)?
            .iter()
            .any(|p| matches!(p, 0 | 5))
        || snapshot.document.boolean(3)? == Some(true)
        || snapshot.document.boolean(41)? == Some(true)
    {
        return Err(Error::ReadOnly);
    }
    // A registered Slack/Quip identity is required; guest/anonymous annotation
    // policy is not inferred from a body-edit grant or a rendered toolbar.
    if snapshot.init.quip_user_id().is_empty()
        || snapshot
            .init
            .options()
            .string(33)?
            .is_none_or(str::is_empty)
    {
        return Err(Error::ReadOnly);
    }
    Ok(())
}

fn target_shape(section: &Section) -> Result<(), Error> {
    if section.section_type()?.unwrap_or(0) != 0 || section.style()?.unwrap_or(0) != 0 {
        return Err(Error::Unsupported);
    }
    section_shape(section)
}
fn section_shape(section: &Section) -> Result<(), Error> {
    section_shape_with_layout(section, false)
}
// Inert text/body validation only. The versioned SectionPlan and live ingress
// separately require complete table membership before using this shape.
fn section_body_shape(section: &Section) -> Result<(), Error> {
    section_shape_with_layout(section, true)
}
fn admitted_section_shape(snapshot: &CanvasCollabSnapshot, section: &Section) -> Result<(), Error> {
    if section.raw().boolean(29)? == Some(true) {
        table_layout::Tables::capture_columns(&snapshot.sections)?.admit_body(section)?;
        section_body_shape(section)
    } else {
        section_shape(section)
    }
}
fn section_shape_with_layout(section: &Section, allow_layout: bool) -> Result<(), Error> {
    if section.section_type()? == Some(2) {
        return image_body::shape_with_layout(section, allow_layout);
    }
    if section.section_type()? == Some(1) {
        return list_body::shape_with_layout(section, allow_layout);
    }
    if section.section_type()?.unwrap_or(0) != 0
        || !matches!(
            section.style()?.unwrap_or(0),
            0 | 1 | 2 | 3 | 4 | 16 | 17 | 39 | 40 | 41 | 47
        )
        || section.raw().uint(11)?.unwrap_or(0) != 0
        || section.deleted()? == Some(true)
        || section.read_only()? == Some(true)
        || section.parents()?.is_some_and(|p| !p.fields().is_empty())
        || (!allow_layout && section.raw().boolean(29)? == Some(true))
        || section.raw().has(15)
        || section
            .sequence()?
            .is_none_or(|s| s == 0 || s > i64::MAX as u64)
    {
        return Err(Error::Unsupported);
    }
    position::base(section.raw().string(8)?.ok_or(Error::Unsupported)?)?;
    Ok(())
}
fn target<'a>(
    snapshot: &'a CanvasCollabSnapshot,
    id: &str,
    ranged: bool,
    archived: bool,
    presentation: Option<&AnnotationPresentation>,
    whole_section: bool,
) -> Result<&'a Section, Error> {
    comment_permission(snapshot)?;
    let section = snapshot.section(id).ok_or(Error::Changed)?;
    if whole_section {
        admitted_section_shape(snapshot, section)?;
    } else {
        target_shape(section)?;
    }
    // The existing canonical text parser refuses unsupported RTML, never strips
    // it to create an apparently equivalent anchor/snippet.
    if whole_section {
        // The separately captured SectionPlan retains every original wrapper,
        // control and metadata row. This parser alone grants no body edit.
        if section.section_type()? == Some(2) {
            admitted_section_shape(snapshot, section)?;
        } else if section.section_type()? == Some(1) {
            list_body::ListBody::capture_columns(&snapshot.sections, section)?;
        } else {
            inline::original(section)?;
        }
    } else if ranged {
        let original = inline::original(section)?;
        inline::Inspection::new(snapshot)?.capture_context(
            section,
            &original,
            archived,
            presentation,
        )?;
    } else {
        super::text::canonical_draft(section)?;
    }
    position::base(section.raw().string(8)?.ok_or(Error::Unsupported)?)?;
    for other in snapshot.sections.iter().filter(|_| !whole_section) {
        if other.deleted()? == Some(true) || other.section_type()? != Some(9) {
            continue;
        }
        if let Some(content) = other.raw().message(12)?
            && let Some(highlight) = content.message(7)?
            && strings(&highlight, 3)?.iter().any(|anchor| anchor == id)
        {
            // Existing and archived annotations need their own canonical root;
            // this operation must not silently duplicate or delete either one.
            return Err(Error::Unchanged);
        }
    }
    Ok(section)
}

/// Existing is a read-only control identity. New remains a captured live intent;
/// neither variant itself dispatches or reconstructs an operation from storage.
pub enum SectionAnnotationChoice {
    Existing(String),
    New(AnnotationIntent),
}
impl AnnotationIntent {
    /// The native caller captures export from the original image node before
    /// replacing the body with Comments. This does not accept exported pixels,
    /// caller-provided snippet JSON, a control ID, or a stored replay ticket.
    pub fn for_image(
        snapshot: &CanvasCollabSnapshot,
        id: &str,
        presentation: &AnnotationPresentation,
        export: ImageExport,
    ) -> Result<SectionAnnotationChoice, Error> {
        comment_permission(snapshot)?;
        let anchor = snapshot.section(id).ok_or(Error::Changed)?;
        if anchor.section_type()? != Some(2) {
            return Err(Error::Unsupported);
        }
        admitted_section_shape(snapshot, anchor)?;
        if let Some(control) = section_plan::existing(snapshot, id, presentation)? {
            return Ok(SectionAnnotationChoice::Existing(control));
        }
        let plan = section_plan::SectionPlan::capture_image(snapshot, id, presentation, export)?;
        Self::capture_inner(snapshot, id, None, None, None, Some(plan))
            .map(SectionAnnotationChoice::New)
    }

    pub fn for_section(
        snapshot: &CanvasCollabSnapshot,
        id: &str,
        presentation: &AnnotationPresentation,
    ) -> Result<SectionAnnotationChoice, Error> {
        comment_permission(snapshot)?;
        let anchor = snapshot.section(id).ok_or(Error::Changed)?;
        admitted_section_shape(snapshot, anchor)?;
        if anchor.section_type()? == Some(2) {
            return Err(Error::Unsupported);
        }
        if anchor.section_type()? == Some(1) {
            list_body::ListBody::capture_columns(&snapshot.sections, anchor)?;
        } else {
            section_policy::admit(snapshot, anchor)?;
            inline::Whole::capture(snapshot, anchor, presentation)?;
        }
        if let Some(control) = section_plan::existing(snapshot, id, presentation)? {
            return Ok(SectionAnnotationChoice::Existing(control));
        }
        let plan = section_plan::SectionPlan::capture(snapshot, id, presentation)?;
        Self::capture_inner(snapshot, id, None, None, None, Some(plan))
            .map(SectionAnnotationChoice::New)
    }

    pub fn capture(snapshot: &CanvasCollabSnapshot, id: &str) -> Result<Self, Error> {
        Self::capture_inner(snapshot, id, None, None, None, None)
    }
    fn capture_inner(
        snapshot: &CanvasCollabSnapshot,
        id: &str,
        selection: Option<inline::Selection>,
        removed: Option<Vec<String>>,
        presentation: Option<&AnnotationPresentation>,
        section_plan: Option<section_plan::SectionPlan>,
    ) -> Result<Self, Error> {
        let section = target(
            snapshot,
            id,
            selection.is_some(),
            removed.is_some() || presentation.is_some(),
            presentation,
            section_plan.is_some(),
        )?
        .clone();
        let target_position = position::base(section.raw().string(8)?.ok_or(Error::Unsupported)?)?;
        let mut previous: Option<&str> = None;
        let mut positions = std::collections::HashSet::new();
        let tables = section_plan
            .as_ref()
            .filter(|p| p.layout_custody())
            .map(|plan| plan.tables(&snapshot.sections))
            .transpose()?;
        let layout_owner = tables.as_ref().and_then(|t| t.layout_owner(section.id()));
        for item in &snapshot.sections {
            if (item.deleted()? == Some(true)
                && !section_plan
                    .as_ref()
                    .is_some_and(|plan| plan.loaded_layout()))
                || item.raw().uint(11)?.unwrap_or(0) != 0
                || item.parents()?.is_some_and(|p| !p.fields().is_empty())
            {
                continue;
            }
            if let Some(tables) = &tables
                && tables.layout_owner(item.id()) != layout_owner
            {
                continue;
            }
            let value = position::base(item.raw().string(8)?.ok_or(Error::Unsupported)?)?;
            if !positions.insert(value) {
                return Err(Error::Unsupported);
            }
            if value < target_position && previous.is_none_or(|p| p < value) {
                previous = Some(value);
            }
        }
        let layout_previous = if layout_owner.is_some() {
            tables
                .as_ref()
                .ok_or(Error::Changed)?
                .previous_position(section.id())?
        } else {
            None
        };
        if layout_owner.is_some() {
            previous = layout_previous.as_deref();
        }
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let new_id = position::section_id(snapshot, &suffix[..25])?;
        if snapshot.section(&new_id).is_some() {
            return Err(Error::Changed);
        }
        let mut removal = None;
        let (snippet, preserved) = if let Some(selection) = selection {
            let original = inline::original(&section)?;
            let preserved = inline::Inspection::new(snapshot)?.capture_context(
                &section,
                &original,
                removed.is_some() || presentation.is_some(),
                presentation,
            )?;
            if let Some(ids) = removed {
                removal = Some(inline::Removal::capture(
                    &section,
                    &original,
                    preserved.as_ref().ok_or(Error::Unsupported)?,
                    ids,
                )?);
            }
            inline::capture(&original, selection, &new_id, removal.as_ref())?;
            let model = inline::captured_rtml(&original, selection, &new_id, removal.as_ref())?;
            (
                snippet::capture_inline(
                    &section,
                    &new_id,
                    snapshot.init.quip_user_id(),
                    snapshot.init.options().string(33)?.ok_or(Error::ReadOnly)?,
                    &model,
                )?,
                preserved,
            )
        } else {
            (
                if section.section_type()? == Some(2) {
                    section_plan
                        .as_ref()
                        .ok_or(Error::Unsupported)?
                        .image_snippet(
                            &new_id,
                            snapshot.init.quip_user_id(),
                            snapshot.init.options().string(33)?.ok_or(Error::ReadOnly)?,
                        )?
                } else if section.section_type()? == Some(1) {
                    section_plan
                        .as_ref()
                        .ok_or(Error::Unsupported)?
                        .list_snippet(
                            &new_id,
                            snapshot.init.quip_user_id(),
                            snapshot.init.options().string(33)?.ok_or(Error::ReadOnly)?,
                        )?
                } else if section_plan.as_ref().is_some_and(|p| p.body().is_some()) {
                    snippet::capture_whole(
                        &section,
                        &new_id,
                        snapshot.init.quip_user_id(),
                        snapshot.init.options().string(33)?.ok_or(Error::ReadOnly)?,
                    )?
                } else if section_plan.is_some() {
                    snippet::capture_styled(
                        &section,
                        &new_id,
                        snapshot.init.quip_user_id(),
                        snapshot.init.options().string(33)?.ok_or(Error::ReadOnly)?,
                    )?
                } else {
                    snippet::capture(
                        &section,
                        &new_id,
                        snapshot.init.quip_user_id(),
                        snapshot.init.options().string(33)?.ok_or(Error::ReadOnly)?,
                        snapshot.init.workspace_id(),
                    )?
                },
                None,
            )
        };
        let path = position::between(previous, Some(target_position))?;
        Ok(Self {
            snapshot: Arc::new(snapshot.clone()),
            section,
            title: CanonicalTitle::capture(snapshot)?,
            id: new_id,
            session: uuid::Uuid::new_v4().simple().to_string()[..12].into(),
            path,
            snippet,
            inline: selection,
            preserved,
            removal,
            section_plan,
        })
    }
    pub fn anchor_section(&self) -> &str {
        self.section.id()
    }
    /// Native callers must bind these UTF-16 offsets to the exact original
    /// paragraph node and canonical bytes; this capture creates no request.
    pub fn capture_range(
        snapshot: &CanvasCollabSnapshot,
        id: &str,
        start: usize,
        end: usize,
    ) -> Result<Self, Error> {
        Self::capture_inner(
            snapshot,
            id,
            Some(inline::Selection { start, end }),
            None,
            None,
            None,
        )
    }
    /// Renderer-owned original-node hidden-removal observation. Only canonical
    /// archived IDs may be removed; this never deletes their controls/threads.
    pub fn capture_range_with_archived(
        snapshot: &CanvasCollabSnapshot,
        id: &str,
        start: usize,
        end: usize,
        removed: Vec<String>,
    ) -> Result<Self, Error> {
        Self::capture_inner(
            snapshot,
            id,
            Some(inline::Selection { start, end }),
            Some(removed),
            None,
            None,
        )
    }
    /// Original native capture supplies the removal set under this exact
    /// presentation. Deserialized presentation never creates this live intent.
    pub fn capture_range_with_presentation(
        snapshot: &CanvasCollabSnapshot,
        id: &str,
        start: usize,
        end: usize,
        removed: Vec<String>,
        presentation: &AnnotationPresentation,
    ) -> Result<Self, Error> {
        presentation.validate()?;
        let original = inline::original(snapshot.section(id).ok_or(Error::Changed)?)?;
        let preserved = inline::Inspection::new(snapshot)?.capture_context(
            snapshot.section(id).ok_or(Error::Changed)?,
            &original,
            true,
            Some(presentation),
        )?;
        let hidden = preserved
            .as_ref()
            .map(|p| p.hidden(snapshot.section(id).unwrap()))
            .transpose()?
            .unwrap_or_default();
        if hidden.is_empty() && !removed.is_empty() {
            return Err(Error::Changed);
        }
        Self::capture_inner(
            snapshot,
            id,
            Some(inline::Selection { start, end }),
            (!hidden.is_empty()).then_some(removed),
            Some(presentation),
            None,
        )
    }
    pub fn prepare(self, fresh: &CanvasCollabSnapshot) -> Result<PreparedAnnotation, Error> {
        if self.inline.is_some()
            || self
                .section_plan
                .as_ref()
                .and_then(|p| p.body())
                .is_some_and(|b| b.changes_body())
        {
            return Err(Error::Unsupported);
        }
        self.prepare_inner(fresh, None)
    }
    pub fn prepare_at(
        self,
        fresh: &CanvasCollabSnapshot,
        now_usec: u64,
    ) -> Result<PreparedAnnotation, Error> {
        self.prepare_inner(fresh, Some(now_usec))
    }
    fn prepare_inner(
        self,
        fresh: &CanvasCollabSnapshot,
        now: Option<u64>,
    ) -> Result<PreparedAnnotation, Error> {
        self.revalidate(fresh)?;
        let empty = RawMessage::decode(&[])?;
        // Exact retained Highlight.renderToDom defaults, followed by creator and
        // addHighlightSection. Color's descriptor default is YELLOW (2), not 0.
        let highlight = RawMessage::from_fields(&[
            F::bytes(1, fresh.init.quip_user_id().as_bytes())?,
            F::bytes(3, self.section.id().as_bytes())?,
            F::uint(4, 0)?,
            F::uint(5, 0)?,
            F::uint(9, 2)?,
            F::bytes(10, b"")?,
            F::message(11, &empty)?,
            F::uint(13, 0)?,
        ])?;
        let content = RawMessage::from_fields(&[F::message(7, &highlight)?])?;
        // Entity.toUpdateBundle for a new Control: no parents, position, style,
        // CRDT or content-origin field is invented when toPb does not emit it.
        let section = RawMessage::from_fields(&[
            F::bytes(1, self.id.as_bytes())?,
            F::uint(2, 0)?,
            F::uint(3, 0)?,
            F::uint(9, 9)?,
            F::uint(11, 1)?,
            F::message(12, &content)?,
            F::bytes(14, b"")?,
            F::uint(19, 1)?,
            F::uint(20, 1)?,
            F::bytes(21, self.path.as_bytes())?,
            F::bytes(33, format!("{}-0", self.session).as_bytes())?,
            F::uint(35, 0)?,
        ])?;
        let body_request = self
            .section_plan
            .as_ref()
            .and_then(|p| p.body())
            .filter(|b| b.changes_body())
            .map(|b| b.request(&self.section, &self.session, now.ok_or(Error::Unsupported)?))
            .transpose()?;
        let mut updates = Vec::new();
        if let Some(plan) = &self.section_plan {
            for update in plan.requests(&self.session, body_request.as_ref().map(|p| &p.0))? {
                updates.push(F::message(1, &update)?);
            }
        }
        updates.push(F::message(1, &section)?);
        updates.push(F::message(14, self.title.rich_text())?);
        let data = RawMessage::from_fields(&updates)?;
        let selection = self.inline;
        let mut prepared = PreparedAnnotation {
            intent: self,
            data,
            inline: body_request.map(|p| p.1),
        };
        if let Some(selection) = selection {
            let inline = inline::prepare(
                &mut prepared,
                fresh,
                selection,
                now.ok_or(Error::Unsupported)?,
            )?;
            prepared.inline = Some(inline);
        }
        Ok(prepared)
    }
    fn revalidate(&self, fresh: &CanvasCollabSnapshot) -> Result<(), Error> {
        if fresh.file_id() != self.snapshot.file_id()
            || fresh.thread_id() != self.snapshot.thread_id()
            || fresh.document_id() != self.snapshot.document_id()
            || fresh.init.workspace_id() != self.snapshot.init.workspace_id()
            || fresh.init.quip_user_id() != self.snapshot.init.quip_user_id()
            || fresh.init.options().string(33)? != self.snapshot.init.options().string(33)?
            || fresh.thread != self.snapshot.thread
            || fresh.document != self.snapshot.document
            || fresh.sections != self.snapshot.sections
            || target(
                fresh,
                self.section.id(),
                self.inline.is_some(),
                self.removal.is_some()
                    || self
                        .preserved
                        .as_ref()
                        .and_then(|p| p.presentation())
                        .is_some(),
                self.preserved.as_ref().and_then(|p| p.presentation()),
                self.section_plan.is_some(),
            )? != &self.section
        {
            return Err(Error::Changed);
        }
        if let Some(plan) = &self.section_plan {
            plan.current(fresh)?;
        }
        self.title.revalidate(fresh)
    }
}

impl PreparedAnnotation {
    pub fn workspace_id(&self) -> &str {
        self.intent.snapshot.init.workspace_id()
    }
    pub fn user_id(&self) -> &str {
        self.intent
            .snapshot
            .init
            .options()
            .string(33)
            .ok()
            .flatten()
            .unwrap()
    }
    pub fn file_id(&self) -> &str {
        self.intent.snapshot.file_id()
    }
    pub fn thread_id(&self) -> &str {
        self.intent.snapshot.thread_id()
    }
    pub fn document_id(&self) -> &str {
        self.intent.snapshot.document_id()
    }
    pub fn control_id(&self) -> &str {
        &self.intent.id
    }
    pub fn anchor_section(&self) -> &str {
        self.intent.section.id()
    }
    pub fn session_id(&self) -> &str {
        &self.intent.session
    }
    pub fn document_data(&self) -> &RawMessage {
        &self.data
    }
    pub fn form_title(&self) -> &str {
        self.intent.title.form_title()
    }
    pub(crate) fn validate_snapshot(
        &self,
        fresh: &CanvasCollabSnapshot,
        now_usec: u64,
    ) -> Result<(), Error> {
        self.intent.revalidate(fresh)?;
        if self
            .inline
            .as_ref()
            .and_then(|p| p.expires)
            .is_some_and(|expiry| expiry <= now_usec)
        {
            return Err(Error::Expired);
        }
        Ok(())
    }
    pub fn validate_ack(self, ack: &super::EditAck) -> Result<AcknowledgedAnnotation, Error> {
        if ack.changed != Some(true)
            || ack.is_new_edit_session == Some(true)
            || ack.cell_data.is_some()
            || ack.import_sections.is_some()
            || ack.mount_points.is_some()
        {
            return Err(Error::Unconfirmed);
        }
        let data = ack.final_data.as_ref().ok_or(Error::Unconfirmed)?;
        if data.document_id()?.is_some_and(|v| v != self.document_id()) {
            return Err(Error::Unconfirmed);
        }
        let sections = data.sections();
        let batch = self.intent.section_plan.as_ref().and_then(|p| p.batch());
        let old_count = batch.map_or(0, |b| b.len());
        if sections.len() != old_count + 1 + usize::from(self.inline.is_some()) {
            return Err(Error::Unconfirmed);
        }
        let old = sections
            .iter()
            .filter(|s| {
                s.id() != self.control_id()
                    && (self.inline.is_none() || s.id() != self.anchor_section())
            })
            .cloned()
            .collect::<Vec<_>>();
        let changed_controls = batch.map(|b| b.acknowledge(&old)).transpose()?;
        if batch.is_none() && !old.is_empty() {
            return Err(Error::Unconfirmed);
        }
        let sections = sections
            .iter()
            .filter(|s| {
                s.id() == self.control_id()
                    || (self.inline.is_some() && s.id() == self.anchor_section())
            })
            .cloned()
            .collect::<Vec<_>>();
        let sections = sections.as_slice();
        let record = self.record()?;
        let (section, anchor) = if self.inline.is_some() {
            if sections.len() != 2 {
                return Err(Error::Unconfirmed);
            }
            let section = sections
                .iter()
                .find(|s| s.id() == self.control_id())
                .ok_or(Error::Unconfirmed)?;
            let anchor = sections
                .iter()
                .find(|s| s.id() == self.anchor_section())
                .ok_or(Error::Unconfirmed)?;
            record.check_acknowledged_bytes(section.raw().raw(), Some(anchor.raw().raw()))?;
            (section.clone(), Some(anchor.clone()))
        } else {
            let [section] = sections else {
                return Err(Error::Unconfirmed);
            };
            record.check_acknowledged_bytes(section.raw().raw(), None)?;
            (section.clone(), None)
        };
        Ok(AcknowledgedAnnotation {
            prepared: self,
            section,
            anchor,
            changed_controls,
        })
    }

    fn check_control(&self, section: &Section) -> Result<(), Error> {
        self.record()?.check_control(section)
    }
}

// Read each repeated occurrence through the bounded wire decoder. A singular
// accessor intentionally rejects duplicate fields and cannot read this list.
fn strings(value: &RawMessage, tag: u32) -> Result<Vec<String>, Error> {
    value
        .fields()
        .iter()
        .filter(|field| field.tag() == tag)
        .map(|field| {
            let one = RawMessage::decode(value.encoded_field(field)?)?;
            Ok(one.string(tag)?.ok_or(Error::Unsupported)?.to_owned())
        })
        .collect()
}
impl AcknowledgedAnnotation {
    pub fn control_id(&self) -> &str {
        self.prepared.control_id()
    }
    pub fn section(&self) -> &Section {
        &self.section
    }
    /// Observe without consuming the accepted result. A failed/stale canonical
    /// read must not destroy the original ACK held by the desktop root.
    pub fn check_confirmation(&self, fresh: &CanvasCollabSnapshot) -> Result<(), Error> {
        comment_permission(fresh)?;
        let p = &self.prepared;
        if let Some(old) = &self.changed_controls {
            old.check_snapshot(fresh)?;
        }
        if fresh.file_id() != p.file_id()
            || fresh.thread_id() != p.thread_id()
            || fresh.document_id() != p.document_id()
            || fresh.init.workspace_id() != p.workspace_id()
            || fresh.init.options().string(33)? != Some(p.user_id())
            || fresh.init.quip_user_id() != p.intent.snapshot.init.quip_user_id()
        {
            return Err(Error::Changed);
        }
        let anchor = fresh
            .section(p.anchor_section())
            .ok_or(Error::Unconfirmed)?;
        p.record()?.check_anchor(anchor, false)?;
        if let Some(plan) = &p.intent.section_plan {
            plan.children_current(fresh)?;
        }
        if let Some(preserved) = &p.intent.preserved {
            preserved.current(fresh)?;
        }
        if let Some(body) = p.intent.section_plan.as_ref().and_then(|p| p.body()) {
            body.current(fresh)?;
        }
        if self.anchor.as_ref().is_some_and(|accepted| {
            anchor.sequence().ok().flatten() < accepted.sequence().ok().flatten()
        }) {
            return Err(Error::Unconfirmed);
        }
        let current = fresh.section(p.control_id()).ok_or(Error::Unconfirmed)?;
        p.check_control(current)?;
        if current.sequence()? < self.section.sequence()? {
            return Err(Error::Unconfirmed);
        }
        Ok(())
    }
    pub fn confirm(self, fresh: &CanvasCollabSnapshot) -> Result<VerifiedAnnotation, Error> {
        self.check_confirmation(fresh)?;
        let section = fresh
            .section(self.control_id())
            .ok_or(Error::Unconfirmed)?
            .clone();
        let anchor = self
            .anchor
            .as_ref()
            .map(|_| {
                fresh
                    .section(self.prepared.anchor_section())
                    .cloned()
                    .ok_or(Error::Unconfirmed)
            })
            .transpose()?;
        let changed_controls = self
            .changed_controls
            .as_ref()
            .map(|p| p.snapshot_bytes(fresh))
            .transpose()?
            .unwrap_or_default();
        Ok(VerifiedAnnotation {
            section,
            prepared: self.prepared,
            anchor,
            changed_controls,
        })
    }
}
impl VerifiedAnnotation {
    pub(in crate::canvas) fn changed_controls(&self) -> &[Vec<u8>] {
        &self.changed_controls
    }
    pub(in crate::canvas) fn anchor(&self) -> Option<&Section> {
        self.anchor.as_ref()
    }
    pub fn control_id(&self) -> &str {
        self.prepared.control_id()
    }
    pub fn file_id(&self) -> &str {
        self.prepared.file_id()
    }
    pub fn workspace_id(&self) -> &str {
        self.prepared.workspace_id()
    }
    pub fn user_id(&self) -> &str {
        self.prepared.user_id()
    }
    pub fn section(&self) -> &Section {
        &self.section
    }
}

#[cfg(test)]
mod tests;
