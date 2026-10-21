//! Whole-section hidden-wrapper removal. Unlike a native range, the retained
//! section command examines every referenced annotation in this exact body.
use super::{CanvasCollabSnapshot, Error, Existing, Preserved, range::RangeModel};
use crate::canvas::{edit::annotation::AnnotationPresentation, wire::Section};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::canvas::edit::annotation) struct Whole {
    // Exact original referenced controls and canonical metadata survive removal.
    preserved: Preserved,
    removed: Vec<String>,
}

impl Whole {
    pub(in crate::canvas::edit::annotation) fn capture(
        snapshot: &CanvasCollabSnapshot,
        anchor: &Section,
        presentation: &AnnotationPresentation,
    ) -> Result<Option<Self>, Error> {
        Self::from_parts(&snapshot.sections, &snapshot.document, anchor, presentation)
    }

    pub(in crate::canvas::edit::annotation) fn from_parts(
        sections: &[Section],
        document: &super::RawMessage,
        anchor: &Section,
        presentation: &AnnotationPresentation,
    ) -> Result<Option<Self>, Error> {
        Self::from_inspection(
            &super::Inspection::from_parts(sections, document)?,
            anchor,
            presentation,
        )
    }

    pub(in crate::canvas::edit::annotation) fn from_inspection(
        inspection: &super::Inspection<'_>,
        anchor: &Section,
        presentation: &AnnotationPresentation,
    ) -> Result<Option<Self>, Error> {
        super::super::section_body_shape(anchor)?;
        let original = super::existing::parse(anchor)?;
        let Some(preserved) =
            inspection.capture_context(anchor, &original, true, Some(presentation))?
        else {
            return Ok(None);
        };
        let removed = preserved.hidden(anchor)?;
        let value = Self { preserved, removed };
        value.validate(anchor, presentation)?;
        Ok(Some(value))
    }

    pub(in crate::canvas::edit::annotation) fn validate(
        &self,
        anchor: &Section,
        presentation: &AnnotationPresentation,
    ) -> Result<(), Error> {
        super::super::section_body_shape(anchor)?;
        let original = super::existing::parse(anchor)?;
        self.preserved.validate_mode(anchor, &original, true)?;
        if self.preserved.presentation() != Some(presentation)
            || self.preserved.hidden(anchor)? != self.removed
        {
            return Err(Error::Changed);
        }
        // Ordinary section ingress uses Content.allowsSectionHighlights: a
        // caret section may offer this command only when every referenced
        // annotation is hidden. Existing visible annotations use their own
        // selection/thread path, never a newly created section root here.
        if original
            .spans
            .iter()
            .any(|span| !self.removed.contains(&span.id))
        {
            return Err(Error::Unsupported);
        }
        // Actual Highlight.remove reports a user content change. That clears
        // default-content attributes and may replace SELECT_ON_FOCUS body data.
        // This removal-only transaction cannot impersonate that template edit.
        // section_shape already excludes inherited/nested default content.
        if self.changes_body()
            && anchor
                .raw()
                .message(16)?
                .map(|attributes| attributes.uint(13))
                .transpose()?
                .flatten()
                .is_some_and(|value| value != 0)
        {
            return Err(Error::Unsupported);
        }
        self.model(&original)?;
        Ok(())
    }

    pub(in crate::canvas::edit::annotation) fn changes_body(&self) -> bool {
        !self.removed.is_empty()
    }

    pub(in crate::canvas::edit::annotation) fn current(
        &self,
        snapshot: &CanvasCollabSnapshot,
    ) -> Result<(), Error> {
        self.preserved.current(snapshot)
    }

    pub(super) fn model(&self, original: &Existing) -> Result<RangeModel, Error> {
        if self
            .removed
            .iter()
            .any(|id| !original.spans.iter().any(|s| &s.id == id))
            || self
                .removed
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.removed.len()
        {
            return Err(Error::Changed);
        }
        let text = original.draft.model()?.text;
        let marks = original
            .marks
            .iter()
            .filter(|mark| {
                !(mark.name() == "annotation"
                    && mark
                        .value()
                        .to_str()
                        .is_some_and(|id| self.removed.iter().any(|old| old == id)))
            })
            .cloned()
            .collect::<Vec<_>>();
        let rtml = super::compact::render(&text, &marks)?;
        // These offsets are inert bookkeeping for the existing validated model.
        // No Selection is created and no native range authority is inferred.
        let escaped_end = text.encode_utf16().count();
        Ok(RangeModel {
            text,
            marks,
            rtml,
            selected: original.draft.clone(),
            escaped_start: 0,
            escaped_end,
        })
    }

    pub(in crate::canvas::edit::annotation) fn prepare(
        &self,
        anchor: &Section,
        session: &str,
        now: u64,
    ) -> Result<super::history::PreparedHistory, Error> {
        if !self.changes_body() {
            return Err(Error::Unchanged);
        }
        let original = super::existing::parse(anchor)?;
        super::history::prepare_removal(anchor, &original, &self.model(&original)?, session, now)
    }

    pub(in crate::canvas::edit::annotation) fn request(
        &self,
        anchor: &Section,
        session: &str,
        now: u64,
    ) -> Result<(super::RawMessage, super::Prepared), Error> {
        use super::{F, RawMessage};
        let history = self.prepare(anchor, session, now)?;
        let parent = RawMessage::from_fields(&[
            F::bytes(1, anchor.id().as_bytes())?,
            F::uint(2, anchor.sequence()?.ok_or(Error::Unsupported)?)?,
            F::uint(9, 0)?,
            F::message(12, &super::content(anchor, &history.rtml)?)?,
            F::message(13, &RawMessage::decode(&[])?)?,
            F::uint(19, 1)?,
            F::uint(20, 1)?,
            F::bytes(33, format!("{session}-0").as_bytes())?,
            F::message(38, &history.wire)?,
        ])?;
        Ok((
            parent,
            super::Prepared {
                expires: history.expires,
            },
        ))
    }

    pub(in crate::canvas::edit::annotation) fn checked(
        &self,
        anchor: &Section,
        parent: &super::RawMessage,
        session: &str,
    ) -> Result<super::record::Checked, Error> {
        if !self.changes_body() {
            return Err(Error::Unsupported);
        }
        let original = super::existing::parse(anchor)?;
        super::record::payload_model(anchor, parent, self.model(&original)?, session, None)
    }

    pub(in crate::canvas::edit::annotation) fn check_parent(
        &self,
        anchor: &Section,
        parent_request: &super::RawMessage,
        session: &str,
        received: &Section,
        direct_ack: bool,
    ) -> Result<(), Error> {
        let expected = self.checked(anchor, parent_request, session)?;
        super::record::parent_mode(anchor, &expected, received, direct_ack, true)
    }

    pub(in crate::canvas::edit::annotation) fn observe(
        &self,
        snapshot: &CanvasCollabSnapshot,
    ) -> Result<super::PriorObservation, Error> {
        self.preserved.observe(snapshot)
    }

    pub(in crate::canvas::edit::annotation) fn validate_observation(
        &self,
        observation: &super::PriorObservation,
    ) -> Result<(), Error> {
        self.preserved.validate_observation(observation)
    }
}
