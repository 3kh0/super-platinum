//! Read-only canonical annotation/body projections; no operation or permit.
use super::*;

/// Read-only paragraph choices. This creates no operation identity or permit;
/// exact comment permission and annotation conflicts are rechecked by capture.
pub fn paragraph_previews(
    snapshot: &CanvasCollabSnapshot,
) -> Result<Vec<(String, Vec<serde_json::Value>)>, Error> {
    Ok(paragraph_seeds(snapshot)?
        .into_iter()
        .map(|(id, draft)| (id, draft.rich_blocks()))
        .collect())
}
/// Renderer-neutral read-only seeds. These expose no edit or annotation permit.
pub fn paragraph_seeds(
    snapshot: &CanvasCollabSnapshot,
) -> Result<Vec<(String, super::super::text::TextDraft)>, Error> {
    comment_permission(snapshot)?;
    let mut annotated = std::collections::HashSet::new();
    for section in &snapshot.sections {
        if section.deleted()? == Some(true) || section.section_type()? != Some(9) {
            continue;
        }
        if let Some(content) = section.raw().message(12)? {
            if let Some(highlight) = content.message(7)? {
                annotated.extend(strings(&highlight, 3)?);
            }
        }
    }
    let mut out = Vec::new();
    for section in &snapshot.sections {
        if annotated.contains(section.id())
            || section.deleted()? == Some(true)
            || section.section_type()?.unwrap_or(0) != 0
            || section.style()?.unwrap_or(0) != 0
            || section.raw().uint(11)?.unwrap_or(0) != 0
            || section.read_only()? == Some(true)
            || section.parents()?.is_some_and(|p| !p.fields().is_empty())
        {
            continue;
        }
        if let Ok(draft) = super::super::text::canonical_draft(section) {
            out.push((section.id().into(), draft));
        }
    }
    Ok(out)
}

/// Read-only typed text and existing comment boundaries. These values cannot
/// mint either a body-edit permit or an annotation request.
pub struct AnnotationParagraph {
    pub draft: super::super::text::TextDraft,
    pub runs: Vec<AnnotationRun>,
    pub archived: Vec<String>,
    pub hidden: Vec<String>,
}
pub struct AnnotationRun {
    pub text: String,
    pub style: super::super::text::TextStyle,
    pub control: Option<String>,
}
pub fn range_seeds(
    snapshot: &CanvasCollabSnapshot,
) -> Result<Vec<(String, AnnotationParagraph)>, Error> {
    range_seeds_context(snapshot, None)
}
/// Ordinary reader with the same explicit context used by original-node capture.
pub fn range_seeds_with_presentation(
    snapshot: &CanvasCollabSnapshot,
    presentation: &AnnotationPresentation,
) -> Result<Vec<(String, AnnotationParagraph)>, Error> {
    presentation.validate()?;
    range_seeds_context(snapshot, Some(presentation))
}
fn range_seeds_context(
    snapshot: &CanvasCollabSnapshot,
    presentation: Option<&AnnotationPresentation>,
) -> Result<Vec<(String, AnnotationParagraph)>, Error> {
    comment_permission(snapshot)?;
    let mut section_highlights = std::collections::HashSet::new();
    for section in &snapshot.sections {
        if section.deleted()? == Some(true) || section.section_type()? != Some(9) {
            continue;
        }
        if let Some(content) = section.raw().message(12)? {
            if let Some(highlight) = content.message(7)? {
                section_highlights.extend(strings(&highlight, 3)?);
            }
        }
    }
    let inspection = inline::Inspection::new(snapshot).ok();
    let mut out = Vec::new();
    for section in &snapshot.sections {
        if target_shape(section).is_err() || section_highlights.contains(section.id()) {
            continue;
        }
        let Ok(original) = inline::original(section) else {
            continue;
        };
        if !original.spans.is_empty()
            && inspection.as_ref().is_none_or(|index| {
                index
                    .capture_context(section, &original, true, presentation)
                    .is_err()
            })
        {
            continue;
        }
        let archived = inspection
            .as_ref()
            .map(|index| index.capture_context(section, &original, true, presentation))
            .transpose()?
            .flatten()
            .map(|p| p.archived(section))
            .transpose()?
            .unwrap_or_default();
        let hidden = inspection
            .as_ref()
            .map(|index| index.capture_context(section, &original, true, presentation))
            .transpose()?
            .flatten()
            .map(|p| p.hidden(section))
            .transpose()?
            .unwrap_or_default();
        let mut runs: Vec<AnnotationRun> = Vec::new();
        let mut offset = 0;
        let mut span_index = 0;
        for run in &original.draft.runs {
            for ch in run.text.chars() {
                let end = offset + ch.len_utf16();
                while original
                    .spans
                    .get(span_index)
                    .is_some_and(|span| span.end <= offset)
                {
                    span_index += 1;
                }
                let control = original
                    .spans
                    .get(span_index)
                    .filter(|span| span.start <= offset && end <= span.end)
                    .map(|s| s.id.as_str());
                if let Some(last) = runs
                    .last_mut()
                    .filter(|r| r.style == run.style && r.control.as_deref() == control)
                {
                    last.text.push(ch);
                } else {
                    if runs.len() >= 4096 {
                        return Err(Error::Unsupported);
                    }
                    runs.push(AnnotationRun {
                        text: ch.to_string(),
                        style: run.style.clone(),
                        control: control.map(str::to_owned),
                    });
                }
                offset = end;
            }
        }
        out.push((
            section.id().into(),
            AnnotationParagraph {
                draft: original.draft,
                runs,
                archived,
                hidden,
            },
        ));
    }
    Ok(out)
}
