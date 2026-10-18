//! Bounded complete-load section choices. Projection creates no operation ID.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub struct SectionAnnotationPreview {
    pub id: String,
    pub blocks: Vec<serde_json::Value>,
    pub existing_control: Option<String>,
}

pub fn section_previews(
    snapshot: &CanvasCollabSnapshot,
    presentation: &AnnotationPresentation,
) -> Result<Vec<SectionAnnotationPreview>, Error> {
    comment_permission(snapshot)?;
    presentation.validate()?;
    let metadata = section_inventory::metadata(&snapshot.document)?;
    let mut ids = BTreeSet::new();
    let mut existing = BTreeMap::new();
    // One complete initial registry pass, in load order. Do not sort controls by
    // ID or repeatedly rescan a large document for every rendered choice.
    for section in &snapshot.sections {
        if !ids.insert(section.id()) {
            return Err(Error::Changed);
        }
        if section.deleted()? == Some(true) || section.section_type()? != Some(9) {
            continue;
        }
        let Some(content) = section.raw().message(12)? else {
            continue;
        };
        let Some(highlight) = content.message(7)? else {
            continue;
        };
        let anchors = strings(&highlight, 3)?;
        if anchors.is_empty() {
            continue;
        }
        if section.thread_id()? != Some(snapshot.thread_id())
            || section.document_id()? != Some(snapshot.document_id())
            || section.raw().uint(11)? != Some(1)
            || anchors.iter().collect::<BTreeSet<_>>().len() != anchors.len()
        {
            return Err(Error::Changed);
        }
        let visibility = visibility::inspect(section, metadata.get(section.id()), presentation)?;
        if visibility.hidden {
            continue;
        }
        let positioned = section_inventory::positioned(&highlight)?;
        if positioned {
            continue;
        }
        for anchor in anchors {
            existing.insert(anchor, section.id().to_owned());
        }
    }
    let mut result = Vec::new();
    let inspection = inline::Inspection::new(snapshot)?;
    let tables = table_layout::Tables::capture_columns(&snapshot.sections).ok();
    for section in &snapshot.sections {
        let shape = if section.raw().boolean(29)? == Some(true) {
            tables
                .as_ref()
                .ok_or(Error::Changed)
                .and_then(|tables| tables.admit_body(section))
                .and_then(|_| section_body_shape(section))
        } else {
            section_shape(section)
        };
        if shape.is_err() {
            continue;
        }
        let projected = if section.section_type()? == Some(1) {
            snippet::list_preview_columns(&snapshot.sections, section)
        } else {
            if section_policy::admit(snapshot, section).is_err()
                || inline::Whole::from_inspection(&inspection, section, presentation).is_err()
            {
                continue;
            }
            snippet::preview_whole(section)
        };
        let Ok(blocks) = projected else {
            continue;
        };
        result.push(SectionAnnotationPreview {
            id: section.id().into(),
            blocks,
            existing_control: existing.remove(section.id()),
        });
    }
    Ok(result)
}
