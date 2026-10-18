//! Exact existing controls/metadata retained with an inert range record.
//! Controls and metadata stay exact even when a v4 operation unwraps archived marks.
use super::{CanvasCollabSnapshot, Error, Existing, RawMessage};
use crate::canvas::wire::Section;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::canvas::edit::annotation) struct Preserved {
    controls: Vec<Vec<u8>>,
    metadata: Vec<Option<Vec<u8>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    presentation: Option<super::super::AnnotationPresentation>,
}

fn metadata(document: &RawMessage) -> Result<BTreeMap<String, RawMessage>, Error> {
    let mut result = BTreeMap::new();
    if let Some(container) = document.message(16)? {
        if container.fields().iter().any(|f| f.tag() != 1) {
            return Err(Error::Unsupported);
        }
        for item in container.messages(1)? {
            let id = item
                .string(1)?
                .filter(|id| super::existing::safe_id(id))
                .ok_or(Error::Unsupported)?;
            if result.len() >= crate::canvas::wire::MAX_SECTIONS
                || result.insert(id.into(), item.clone()).is_some()
            {
                return Err(Error::Unsupported);
            }
        }
    }
    Ok(result)
}

fn status(
    control: &Section,
    meta: Option<&RawMessage>,
    anchor: &Section,
    presentation: Option<&super::super::AnnotationPresentation>,
) -> Result<bool, Error> {
    let highlight = control
        .raw()
        .message(12)?
        .ok_or(Error::Unsupported)?
        .message(7)?
        .ok_or(Error::Unsupported)?;
    let archived = meta
        .map(|m| m.boolean(8))
        .transpose()?
        .flatten()
        .unwrap_or(highlight.boolean(4)?.unwrap_or(false));
    // Actual isHidden: archived always hides; positive message count is visible
    // regardless of the stub/reaction-bar/currently-open presentation options.
    // Zero-count/reaction-only controls need that separate presentation policy.
    if control.id() == anchor.id()
        || control.document_id()? != anchor.document_id()?
        || control.thread_id()? != anchor.thread_id()?
        || control.section_type()? != Some(9)
        || control.raw().uint(11)? != Some(1)
        || control.deleted()? == Some(true)
        || control
            .sequence()?
            .is_none_or(|v| v == 0 || v > i64::MAX as u64)
        || control.parents()?.is_some_and(|p| !p.fields().is_empty())
        || control.crdt()?.is_some()
        || highlight.has(3)
        || meta.is_some_and(|m| m.string(1).ok().flatten() != Some(control.id()))
        || (presentation.is_none()
            && !archived
            && meta
                .map(|m| m.uint(2))
                .transpose()?
                .flatten()
                .is_none_or(|v| v == 0 || v > i32::MAX as u64))
    {
        return Err(Error::Unsupported);
    }
    for tag in [4, 7] {
        if meta
            .map(|m| m.uint(tag))
            .transpose()?
            .flatten()
            .is_some_and(|v| v > i64::MAX as u64)
        {
            return Err(Error::Unsupported);
        }
    }
    for tag in [2, 6] {
        if meta
            .map(|m| m.uint(tag))
            .transpose()?
            .flatten()
            .is_some_and(|v| v > i32::MAX as u64)
        {
            return Err(Error::Unsupported);
        }
    }
    if let Some(presentation) = presentation {
        super::super::visibility::inspect(control, meta, presentation)?;
    }
    Ok(archived)
}

pub(in crate::canvas::edit::annotation) struct Inspection<'a> {
    controls: BTreeMap<&'a str, &'a Section>,
    metadata: BTreeMap<String, RawMessage>,
}
impl<'a> Inspection<'a> {
    pub(in crate::canvas::edit::annotation) fn new(
        snapshot: &'a CanvasCollabSnapshot,
    ) -> Result<Self, Error> {
        Self::from_parts(&snapshot.sections, &snapshot.document)
    }
    pub(in crate::canvas::edit::annotation) fn from_parts(
        sections: &'a [Section],
        document: &RawMessage,
    ) -> Result<Self, Error> {
        let mut controls = BTreeMap::new();
        for section in sections {
            if controls.insert(section.id(), section).is_some() {
                return Err(Error::Unsupported);
            }
        }
        Ok(Self {
            controls,
            metadata: metadata(document)?,
        })
    }
    pub(in crate::canvas::edit::annotation) fn capture(
        &self,
        anchor: &Section,
        model: &Existing,
    ) -> Result<Option<Preserved>, Error> {
        self.capture_mode(anchor, model, false)
    }
    pub(in crate::canvas::edit::annotation) fn capture_mode(
        &self,
        anchor: &Section,
        model: &Existing,
        allow_archived: bool,
    ) -> Result<Option<Preserved>, Error> {
        self.capture_context(anchor, model, allow_archived, None)
    }
    pub(in crate::canvas::edit::annotation) fn capture_context(
        &self,
        anchor: &Section,
        model: &Existing,
        allow_archived: bool,
        presentation: Option<&super::super::AnnotationPresentation>,
    ) -> Result<Option<Preserved>, Error> {
        if let Some(presentation) = presentation {
            presentation.validate()?;
        }
        if model.spans.is_empty() {
            return Ok(None);
        }
        let mut value = Preserved {
            controls: Vec::new(),
            metadata: Vec::new(),
            presentation: presentation.cloned(),
        };
        for span in &model.spans {
            let control = self
                .controls
                .get(span.id.as_str())
                .ok_or(Error::Unsupported)?;
            let meta = self.metadata.get(&span.id);
            if status(control, meta, anchor, presentation)? && !allow_archived {
                return Err(Error::Unsupported);
            }
            value.controls.push(control.raw().raw().to_vec());
            value.metadata.push(meta.map(|m| m.raw().to_vec()));
        }
        value.validate_mode(anchor, model, allow_archived)?;
        Ok(Some(value))
    }
}

impl Preserved {
    pub(in crate::canvas::edit::annotation) fn capture(
        snapshot: &CanvasCollabSnapshot,
        anchor: &Section,
        model: &Existing,
    ) -> Result<Option<Self>, Error> {
        if model.spans.is_empty() {
            return Ok(None);
        }
        Inspection::new(snapshot)?.capture(anchor, model)
    }
    pub(in crate::canvas::edit::annotation) fn validate(
        &self,
        anchor: &Section,
        model: &Existing,
    ) -> Result<(), Error> {
        self.validate_mode(anchor, model, false)
    }
    pub(in crate::canvas::edit::annotation) fn validate_mode(
        &self,
        anchor: &Section,
        model: &Existing,
        allow_archived: bool,
    ) -> Result<(), Error> {
        if model.spans.is_empty()
            || self.controls.len() != model.spans.len()
            || self.metadata.len() != model.spans.len()
            || self.controls.len() > 4096
            || self
                .controls
                .iter()
                .chain(self.metadata.iter().flatten())
                .map(Vec::len)
                .sum::<usize>()
                > 512 * 1024
        {
            return Err(Error::Unsupported);
        }
        for ((span, control), meta) in model.spans.iter().zip(&self.controls).zip(&self.metadata) {
            let control = Section::decode(control)?;
            let meta = meta.as_deref().map(RawMessage::decode).transpose()?;
            if control.id() != span.id {
                return Err(Error::Unsupported);
            }
            if status(&control, meta.as_ref(), anchor, self.presentation.as_ref())?
                && !allow_archived
            {
                return Err(Error::Unsupported);
            }
        }
        Ok(())
    }
    pub(in crate::canvas::edit::annotation) fn archived(
        &self,
        anchor: &Section,
    ) -> Result<Vec<String>, Error> {
        self.controls
            .iter()
            .zip(&self.metadata)
            .filter_map(|(raw, meta)| {
                let result = (|| {
                    let control = Section::decode(raw)?;
                    let meta = meta.as_deref().map(RawMessage::decode).transpose()?;
                    Ok(
                        status(&control, meta.as_ref(), anchor, self.presentation.as_ref())?
                            .then(|| control.id().to_owned()),
                    )
                })();
                match result {
                    Ok(None) => None,
                    Ok(Some(id)) => Some(Ok(id)),
                    Err(e) => Some(Err(e)),
                }
            })
            .collect()
    }
    pub(in crate::canvas::edit::annotation) fn presentation(
        &self,
    ) -> Option<&super::super::AnnotationPresentation> {
        self.presentation.as_ref()
    }
    pub(in crate::canvas::edit::annotation) fn hidden(
        &self,
        anchor: &Section,
    ) -> Result<Vec<String>, Error> {
        let Some(presentation) = &self.presentation else {
            return self.archived(anchor);
        };
        self.controls
            .iter()
            .zip(&self.metadata)
            .filter_map(|(raw, meta)| {
                let result = (|| {
                    let control = Section::decode(raw)?;
                    let meta = meta.as_deref().map(RawMessage::decode).transpose()?;
                    status(&control, meta.as_ref(), anchor, Some(presentation))?;
                    Ok(
                        super::super::visibility::inspect(&control, meta.as_ref(), presentation)?
                            .hidden
                            .then(|| control.id().to_owned()),
                    )
                })();
                match result {
                    Ok(None) => None,
                    Ok(Some(id)) => Some(Ok(id)),
                    Err(e) => Some(Err(e)),
                }
            })
            .collect()
    }
    pub(in crate::canvas::edit::annotation) fn current(
        &self,
        snapshot: &CanvasCollabSnapshot,
    ) -> Result<(), Error> {
        let current = metadata(&snapshot.document)?;
        for (control, meta) in self.controls.iter().zip(&self.metadata) {
            let control = Section::decode(control)?;
            if snapshot.section(control.id()) != Some(&control)
                || current.get(control.id()).map(|m| m.raw()) != meta.as_deref()
            {
                return Err(Error::Changed);
            }
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::canvas::edit::annotation) struct Observation {
    controls: Vec<Option<Vec<u8>>>,
    metadata: Vec<Option<Vec<u8>>>,
}
impl Preserved {
    pub(in crate::canvas::edit::annotation) fn observe(
        &self,
        snapshot: &CanvasCollabSnapshot,
    ) -> Result<Observation, Error> {
        let current = metadata(&snapshot.document)?;
        let mut value = Observation {
            controls: Vec::new(),
            metadata: Vec::new(),
        };
        for raw in &self.controls {
            let original = Section::decode(raw)?;
            value.controls.push(
                snapshot
                    .section(original.id())
                    .map(|s| s.raw().raw().to_vec()),
            );
            value
                .metadata
                .push(current.get(original.id()).map(|m| m.raw().to_vec()));
        }
        self.validate_observation(&value)?;
        Ok(value)
    }
    pub(in crate::canvas::edit::annotation) fn validate_observation(
        &self,
        value: &Observation,
    ) -> Result<(), Error> {
        if value.controls.len() != self.controls.len()
            || value.metadata.len() != self.controls.len()
            || value
                .controls
                .iter()
                .chain(&value.metadata)
                .flatten()
                .map(Vec::len)
                .sum::<usize>()
                > 512 * 1024
        {
            return Err(Error::Unsupported);
        }
        for ((original, control), metadata) in self
            .controls
            .iter()
            .zip(&value.controls)
            .zip(&value.metadata)
        {
            let original = Section::decode(original)?;
            if let Some(raw) = control {
                let current = Section::decode(raw)?;
                if current.id() != original.id()
                    || current.thread_id()? != original.thread_id()?
                    || current.document_id()? != original.document_id()?
                {
                    return Err(Error::Changed);
                }
            }
            if let Some(raw) = metadata {
                if RawMessage::decode(raw)?.string(1)? != Some(original.id()) {
                    return Err(Error::Changed);
                }
            }
        }
        Ok(())
    }
}
