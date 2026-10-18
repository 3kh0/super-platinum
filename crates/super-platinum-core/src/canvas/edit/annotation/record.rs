//! Inert annotation evidence. Deserialization never recreates PreparedAnnotation
//! or the live ACK/readback proof needed for the next write stage.
use super::*;
use crate::canvas::edit::structure::recovery::Scope;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationRecord {
    version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    inline: Option<inline::Selection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    preserved: Option<inline::Preserved>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    removal: Option<inline::Removal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    section_plan: Option<section_plan::SectionPlan>,
    scope: Scope,
    thread: String,
    document: String,
    quip_user: String,
    anchor: Vec<u8>,
    control: String,
    session: String,
    request: Vec<u8>,
    snippet: serde_json::Value,
}
impl PreparedAnnotation {
    pub fn record(&self) -> Result<AnnotationRecord, Error> {
        let value = AnnotationRecord {
            version: if self
                .intent
                .section_plan
                .as_ref()
                .is_some_and(|p| p.column_layout())
            {
                13
            } else if self
                .intent
                .section_plan
                .as_ref()
                .is_some_and(|p| p.quote_layout())
            {
                12
            } else if self
                .intent
                .section_plan
                .as_ref()
                .is_some_and(|p| p.table_bodies())
            {
                11
            } else if self
                .intent
                .section_plan
                .as_ref()
                .is_some_and(|p| p.table_layout())
            {
                10
            } else if self
                .intent
                .section_plan
                .as_ref()
                .is_some_and(|p| p.media_layout())
            {
                9
            } else if self
                .intent
                .section_plan
                .as_ref()
                .is_some_and(|p| p.list_layout())
            {
                8
            } else if self
                .intent
                .section_plan
                .as_ref()
                .is_some_and(|p| p.body().is_some())
            {
                7
            } else if self.intent.section_plan.is_some() {
                6
            } else if self
                .intent
                .preserved
                .as_ref()
                .and_then(|p| p.presentation())
                .is_some()
            {
                5
            } else if self.intent.removal.is_some() {
                4
            } else if self.intent.preserved.is_some() {
                3
            } else if self.intent.inline.is_some() {
                2
            } else {
                1
            },
            inline: self.intent.inline,
            preserved: self.intent.preserved.clone(),
            removal: self.intent.removal.clone(),
            section_plan: self.intent.section_plan.clone(),
            scope: Scope {
                workspace: self.workspace_id().into(),
                user: self.user_id().into(),
                file: self.file_id().into(),
            },
            thread: self.thread_id().into(),
            document: self.document_id().into(),
            quip_user: self.intent.snapshot.init.quip_user_id().into(),
            anchor: self.intent.section.raw().raw().to_vec(),
            control: self.control_id().into(),
            session: self.session_id().into(),
            request: self.data.raw().to_vec(),
            snippet: self.intent.snippet.clone(),
        };
        value.validate()?;
        Ok(value)
    }
}
/// Copy of an actual accepted response for local preservation only. It cannot
/// be deserialized or converted into a canonical proof or another request.
#[derive(Clone)]
pub struct AnnotationAcknowledgementEvidence {
    pub(in crate::canvas) id: String,
    pub(in crate::canvas) record: AnnotationRecord,
    pub(in crate::canvas) section: Vec<u8>,
    pub(in crate::canvas) anchor: Option<Vec<u8>>,
    pub(in crate::canvas) changed_controls: Vec<Vec<u8>>,
}
impl AcknowledgedAnnotation {
    pub fn evidence(&self) -> Result<AnnotationAcknowledgementEvidence, Error> {
        Ok(AnnotationAcknowledgementEvidence {
            id: uuid::Uuid::new_v4().to_string(),
            record: self.record()?,
            section: self.section().raw().raw().to_vec(),
            anchor: self.anchor.as_ref().map(|s| s.raw().raw().to_vec()),
            changed_controls: self
                .changed_controls
                .as_ref()
                .map(|p| p.actual_bytes())
                .unwrap_or_default(),
        })
    }
    pub fn record(&self) -> Result<AnnotationRecord, Error> {
        self.prepared.record()
    }
}
impl VerifiedAnnotation {
    pub fn record(&self) -> Result<AnnotationRecord, Error> {
        self.prepared.record()
    }
}
impl AnnotationRecord {
    fn body(&self) -> Option<&inline::Whole> {
        self.section_plan.as_ref().and_then(|p| p.body())
    }
    pub(in crate::canvas) fn check_changed_controls(
        &self,
        raw: &[Vec<u8>],
        direct_ack: bool,
    ) -> Result<(), Error> {
        self.validate()?;
        match self.section_plan.as_ref().and_then(|p| p.batch()) {
            Some(batch) if direct_ack => batch.check_ack_bytes(raw),
            Some(batch) => batch.check_canonical_bytes(raw),
            None if raw.is_empty() => Ok(()),
            _ => Err(Error::Unconfirmed),
        }
    }
    pub(in crate::canvas) fn changed_controls_current(
        &self,
        raw: &[Vec<u8>],
        fresh: &CanvasCollabSnapshot,
    ) -> Result<(), Error> {
        self.check_changed_controls(raw, false)?;
        if let Some(batch) = self.section_plan.as_ref().and_then(|p| p.batch()) {
            batch.current_metadata(fresh)?;
            let now = batch.sections(fresh)?;
            if now
                .iter()
                .map(|s| s.raw().raw())
                .ne(raw.iter().map(Vec::as_slice))
            {
                return Err(Error::Changed);
            }
        }
        Ok(())
    }
    pub(in crate::canvas) fn has_changed_controls(&self) -> bool {
        self.section_plan.as_ref().and_then(|p| p.batch()).is_some()
    }
    pub(in crate::canvas) fn has_prior_controls(&self) -> bool {
        self.preserved.is_some()
            || self.has_changed_controls()
            || self.body().is_some()
            || self.section_plan.as_ref().is_some_and(|p| p.list_anchor())
            || self
                .section_plan
                .as_ref()
                .is_some_and(|p| p.layout_custody())
    }
    pub(in crate::canvas) fn validate_prior_evidence(
        &self,
        value: Option<&serde_json::Value>,
    ) -> Result<(), Error> {
        if let Some(plan) = self.section_plan.as_ref().filter(|p| p.layout_custody()) {
            let observed: TableObservation =
                serde_json::from_value(value.ok_or(Error::Unsupported)?.clone())
                    .map_err(|_| Error::Unsupported)?;
            plan.validate_table_observation(&observed.table)?;
            return self.validate_annotation_prior(observed.annotation.as_ref());
        }
        self.validate_annotation_prior(value)
    }
    fn validate_annotation_prior(&self, value: Option<&serde_json::Value>) -> Result<(), Error> {
        if let Some(plan) = self.section_plan.as_ref().filter(|p| p.list_anchor()) {
            let observed: ListSectionObservation =
                serde_json::from_value(value.ok_or(Error::Unsupported)?.clone())
                    .map_err(|_| Error::Unsupported)?;
            plan.validate_children_observation(&observed.children)?;
            return match (plan.batch(), observed.section) {
                (None, None) => Ok(()),
                (Some(batch), Some(value)) => batch.validate_observation(&value),
                _ => Err(Error::Unsupported),
            };
        }
        if let Some(body) = self.body() {
            let value: WholeObservation =
                serde_json::from_value(value.ok_or(Error::Unsupported)?.clone())
                    .map_err(|_| Error::Unsupported)?;
            body.validate_observation(&value.inline)?;
            return match (
                self.section_plan.as_ref().and_then(|p| p.batch()),
                value.section,
            ) {
                (None, None) => Ok(()),
                (Some(batch), Some(observed)) => batch.validate_observation(&observed),
                _ => Err(Error::Unsupported),
            };
        }
        match (&self.preserved, value) {
            (None, None) if !self.has_changed_controls() => Ok(()),
            (None, Some(value)) if self.has_changed_controls() => {
                let observed: control_batch::Observation =
                    serde_json::from_value(value.clone()).map_err(|_| Error::Unsupported)?;
                self.section_plan
                    .as_ref()
                    .and_then(|p| p.batch())
                    .ok_or(Error::Unsupported)?
                    .validate_observation(&observed)
            }
            (Some(preserved), Some(value)) => {
                let observed: inline::PriorObservation =
                    serde_json::from_value(value.clone()).map_err(|_| Error::Unsupported)?;
                preserved.validate_observation(&observed)
            }
            _ => Err(Error::Unsupported),
        }
    }
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn control_id(&self) -> &str {
        &self.control
    }
    pub fn anchor(&self) -> Result<Section, Error> {
        Ok(Section::decode(&self.anchor)?)
    }
    pub fn request_bytes(&self) -> &[u8] {
        &self.request
    }
    pub fn snippet(&self) -> &serde_json::Value {
        &self.snippet
    }
    pub fn validate(&self) -> Result<(), Error> {
        if !matches!(
            (
                self.version,
                self.inline,
                self.preserved.is_some(),
                self.removal.is_some()
            ),
            (1, None, false, false)
                | (2, Some(_), false, false)
                | (3, Some(_), true, false)
                | (4, Some(_), true, true)
                | (5, Some(_), true, _)
                | (6, None, false, false)
                | (7, None, false, false)
                | (8, None, false, false)
                | (9, None, false, false)
                | (10, None, false, false)
                | (11, None, false, false)
                | (12, None, false, false)
                | (13, None, false, false)
        ) || self.session.len() != 12
            || !self.session.bytes().all(|b| b.is_ascii_hexdigit())
            || self
                .anchor
                .len()
                .checked_add(self.request.len())
                .is_none_or(|n| n > crate::canvas::wire::MAX_BYTES)
            || !super::super::structure::position::modern_id(&self.control)
        {
            return Err(Error::Unsupported);
        }
        for id in [
            &self.scope.workspace,
            &self.scope.user,
            &self.scope.file,
            &self.thread,
            &self.document,
            &self.quip_user,
        ] {
            if id.is_empty() || id.len() > 256 || id.chars().any(char::is_control) {
                return Err(Error::Unsupported);
            }
        }
        if (self.version == 5)
            != self
                .preserved
                .as_ref()
                .and_then(|p| p.presentation())
                .is_some()
        {
            return Err(Error::Unsupported);
        }
        if matches!(self.version, 6..=13) != self.section_plan.is_some()
            || (self.version == 13)
                != self
                    .section_plan
                    .as_ref()
                    .is_some_and(|p| p.column_layout())
            || (self.version != 13
                && (self.version == 12)
                    != self.section_plan.as_ref().is_some_and(|p| p.quote_layout()))
            || (!matches!(self.version, 12 | 13)
                && (self.version == 11)
                    != self.section_plan.as_ref().is_some_and(|p| p.table_bodies()))
            || (!matches!(self.version, 12 | 13)
                && (matches!(self.version, 10 | 11))
                    != self.section_plan.as_ref().is_some_and(|p| p.table_layout()))
            || (!matches!(self.version, 10..=13)
                && (self.version == 9)
                    != self.section_plan.as_ref().is_some_and(|p| p.media_layout()))
            || (!matches!(self.version, 9..=13)
                && (self.version == 8)
                    != self.section_plan.as_ref().is_some_and(|p| p.list_layout()))
            || (!matches!(self.version, 8..=13) && (self.version == 7) != self.body().is_some())
        {
            return Err(Error::Unsupported);
        }
        if let Some(plan) = &self.section_plan {
            plan.validate_for(
                &self.scope.workspace,
                &self.scope.user,
                &self.scope.file,
                &self.anchor()?,
            )?;
        }
        let anchor = self.anchor()?;
        if anchor.thread_id()? != Some(self.thread.as_str())
            || anchor.document_id()? != Some(self.document.as_str())
            || anchor.id() == self.control
            || anchor.deleted()? == Some(true)
            || !(anchor.section_type()?.unwrap_or(0) == 0
                || (matches!(self.version, 8..=13) && anchor.section_type()? == Some(1))
                || (matches!(self.version, 9..=13) && anchor.section_type()? == Some(2)))
            || anchor.sequence()?.is_none_or(|n| n == 0)
        {
            return Err(Error::Unsupported);
        }
        if self.inline.is_some() {
            let original = inline::original(&anchor)?;
            match (&self.preserved, original.spans.is_empty()) {
                (None, true) => {}
                (Some(preserved), false) => {
                    if let Some(removal) = &self.removal {
                        removal.validate(&anchor, &original, preserved)?;
                    } else {
                        preserved.validate_mode(&anchor, &original, self.version == 5)?;
                    }
                }
                _ => return Err(Error::Unsupported),
            }
        }
        let data = RawMessage::decode(&self.request)?;
        if data.fields().iter().any(|f| !matches!(f.tag(), 1 | 14)) || data.message(14)?.is_none() {
            return Err(Error::Unsupported);
        }
        let updates = data.messages(1)?;
        let update = if let Some(plan) = &self.section_plan {
            let parent = updates
                .iter()
                .find(|p| p.string(1).ok().flatten() == Some(anchor.id()));
            let old = plan.requests(&self.session, parent)?;
            if updates.len() != old.len() + 1 || updates[..old.len()] != old {
                return Err(Error::Unsupported);
            }
            if let Some(parent) = parent {
                plan.body()
                    .ok_or(Error::Unsupported)?
                    .checked(&anchor, parent, &self.session)?;
            }
            updates.last().ok_or(Error::Unsupported)?
        } else {
            match (self.inline, updates.as_slice()) {
                (None, [control]) | (Some(_), [control, _]) => control,
                _ => return Err(Error::Unsupported),
            }
        };
        let snippet = if let Some(selection) = self.inline {
            let checked = inline::checked_record_payload(
                &anchor,
                &data,
                selection,
                &self.control,
                &self.session,
                self.removal.as_ref(),
            )?;
            super::snippet::capture_inline(
                &anchor,
                &self.control,
                &self.quip_user,
                &self.scope.user,
                &checked.rtml,
            )?
        } else if matches!(self.version, 9..=13) && anchor.section_type()? == Some(2) {
            self.section_plan
                .as_ref()
                .ok_or(Error::Unsupported)?
                .image_snippet(&self.control, &self.quip_user, &self.scope.user)?
        } else if matches!(self.version, 8..=13) && anchor.section_type()? == Some(1) {
            self.section_plan
                .as_ref()
                .ok_or(Error::Unsupported)?
                .list_snippet(&self.control, &self.quip_user, &self.scope.user)?
        } else if self.version == 7 || matches!(self.version, 8..=13) && self.body().is_some() {
            super::snippet::capture_whole(
                &anchor,
                &self.control,
                &self.quip_user,
                &self.scope.user,
            )?
        } else if matches!(self.version, 6 | 8 | 9 | 10 | 11 | 12 | 13) {
            super::snippet::capture_styled(
                &anchor,
                &self.control,
                &self.quip_user,
                &self.scope.user,
            )?
        } else {
            super::snippet::capture(
                &anchor,
                &self.control,
                &self.quip_user,
                &self.scope.user,
                &self.scope.workspace,
            )?
        };
        if self.snippet != snippet {
            return Err(Error::Unsupported);
        }
        let mut tags = update.fields().iter().map(|f| f.tag()).collect::<Vec<_>>();
        tags.sort_unstable();
        if tags != [1, 2, 3, 9, 11, 12, 14, 19, 20, 21, 33, 35]
            || update.string(1)? != Some(self.control.as_str())
            || update.uint(2)? != Some(0)
            || update.boolean(3)? != Some(false)
            || update.uint(9)? != Some(9)
            || update.uint(11)? != Some(1)
            || update.string(14)? != Some("")
            || update.boolean(19)? != Some(true)
            || update.uint(20)? != Some(1)
            || update.string(33)? != Some(format!("{}-0", self.session).as_str())
            || update.boolean(35)? != Some(false)
        {
            return Err(Error::Unsupported);
        }
        let path = update.string(21)?.ok_or(Error::Unsupported)?;
        if !path.ends_with(":temp") {
            return Err(Error::Unsupported);
        }
        position::base(path)?;
        let content = update.message(12)?.ok_or(Error::Unsupported)?;
        if content.fields().len() != 1 || content.fields()[0].tag() != 7 {
            return Err(Error::Unsupported);
        }
        let highlight = content.message(7)?.ok_or(Error::Unsupported)?;
        let expected = RawMessage::from_fields(&[
            F::bytes(1, self.quip_user.as_bytes())?,
            F::bytes(3, anchor.id().as_bytes())?,
            F::uint(4, 0)?,
            F::uint(5, 0)?,
            F::uint(9, 2)?,
            F::bytes(10, b"")?,
            F::message(11, &RawMessage::decode(&[])?)?,
            F::uint(13, 0)?,
        ])?;
        let expected = if self.inline.is_some() {
            expected.remove(3)?
        } else {
            expected
        };
        if highlight != expected {
            return Err(Error::Unsupported);
        }
        Ok(())
    }
    pub(in crate::canvas) fn check_control(&self, section: &Section) -> Result<(), Error> {
        self.validate()?;
        let request = RawMessage::decode(&self.request)?;
        let request = request
            .messages(1)?
            .into_iter()
            .find(|u| u.string(1).ok().flatten() == Some(self.control.as_str()))
            .ok_or(Error::Unsupported)?;
        let path = request.string(21)?.ok_or(Error::Unsupported)?;
        if section.id() != self.control_id()
            || section.document_id()? != Some(self.document.as_str())
            || section.thread_id()? != Some(self.thread.as_str())
            || section.section_type()? != Some(9)
            || section.raw().uint(11)? != Some(1)
            || section.deleted()? == Some(true)
            || section.read_only()? == Some(true)
            || section.raw().has(17)
            || section
                .sequence()?
                .is_none_or(|n| n == 0 || n > i64::MAX as u64)
            || section.parents()?.is_some_and(|p| !p.fields().is_empty())
            || section.crdt()?.is_some()
            || section.raw().uint(10)?.unwrap_or(0) != 0
            || section.raw().string(14)?.is_some_and(|v| !v.is_empty())
            || section.raw().boolean(29)? == Some(true)
            || position::base(section.raw().string(21)?.ok_or(Error::Unconfirmed)?)?
                != position::base(path)?
        {
            return Err(Error::Unconfirmed);
        }
        let content = section.raw().message(12)?.ok_or(Error::Unconfirmed)?;
        if content.fields().iter().any(|f| f.tag() != 7) {
            return Err(Error::Unconfirmed);
        }
        let highlight = content.message(7)?.ok_or(Error::Unconfirmed)?;
        if highlight
            .fields()
            .iter()
            .any(|f| !matches!(f.tag(), 1 | 3 | 4 | 5 | 9 | 10 | 11 | 13))
            || highlight.string(1)? != Some(self.quip_user.as_str())
            || strings(&highlight, 3)? != self.highlight_sections()?
            || highlight.boolean(4)?.unwrap_or(false)
            || highlight.uint(5)?.unwrap_or(0) != 0
            || highlight.uint(9)?.unwrap_or(2) != 2
            || highlight.string(10)?.is_some_and(|v| !v.is_empty())
            || highlight
                .message(11)?
                .is_some_and(|p| !p.fields().is_empty())
            || highlight.uint(13)?.unwrap_or(0) != 0
        {
            return Err(Error::Unconfirmed);
        }
        Ok(())
    }
    fn highlight_sections(&self) -> Result<Vec<String>, Error> {
        Ok(if self.inline.is_some() {
            vec![]
        } else {
            vec![self.anchor()?.id().to_owned()]
        })
    }
    pub(in crate::canvas) fn has_parent_update(&self) -> bool {
        self.inline.is_some() || self.body().is_some_and(|b| b.changes_body())
    }
    pub(in crate::canvas) fn check_anchor(
        &self,
        anchor: &Section,
        direct_ack: bool,
    ) -> Result<(), Error> {
        self.validate()?;
        if let Some(selection) = self.inline {
            let original = self.anchor()?;
            let checked = inline::checked_record_payload(
                &original,
                &RawMessage::decode(&self.request)?,
                selection,
                &self.control,
                &self.session,
                self.removal.as_ref(),
            )?;
            inline::check_record_parent(&original, &checked, anchor, direct_ack)
        } else if let Some(body) = self.body().filter(|b| b.changes_body()) {
            let original = self.anchor()?;
            let request = RawMessage::decode(&self.request)?;
            let parent = request
                .messages(1)?
                .into_iter()
                .find(|p| p.string(1).ok().flatten() == Some(original.id()))
                .ok_or(Error::Unsupported)?;
            body.check_parent(&original, &parent, &self.session, anchor, direct_ack)
        } else if !direct_ack && anchor == &self.anchor()? {
            Ok(())
        } else {
            Err(Error::Unconfirmed)
        }
    }
    /// Inert ACK validation preserves both actual sections for inline edits.
    /// It does not recreate the live acknowledgement or a bootstrap permit.
    pub(in crate::canvas) fn check_acknowledged_bytes(
        &self,
        control: &[u8],
        anchor: Option<&[u8]>,
    ) -> Result<(), Error> {
        let control = Section::decode(control)?;
        self.check_control(&control)?;
        if control.raw().uint(20)? != Some(1) {
            return Err(Error::Unconfirmed);
        }
        match (self.has_parent_update(), anchor) {
            (false, None) => Ok(()),
            (true, Some(anchor)) => self.check_anchor(&Section::decode(anchor)?, true),
            _ => Err(Error::Unconfirmed),
        }
    }
    /// Read-only evidence for an explicit local review. This never recreates the
    /// original ACK, a send permit, or a claim that an uncertain operation failed.
    pub fn review_current(
        &self,
        fresh: &CanvasCollabSnapshot,
    ) -> Result<AnnotationObservation, Error> {
        self.validate()?;
        if fresh.file_id() != self.scope.file
            || fresh.thread_id() != self.thread
            || fresh.document_id() != self.document
            || fresh.init.workspace_id() != self.scope.workspace
            || fresh.init.options().string(33)? != Some(self.scope.user.as_str())
            || fresh.init.quip_user_id() != self.quip_user
        {
            return Err(Error::Changed);
        }
        let anchor = fresh.section(self.anchor()?.id()).ok_or(Error::Changed)?;
        let control = fresh.section(&self.control);
        Ok(AnnotationObservation {
            record: self.clone(),
            table: self
                .section_plan
                .as_ref()
                .filter(|p| p.layout_custody())
                .map(|plan| {
                    plan.observe_table(fresh)
                        .and_then(|v| serde_json::to_value(v).map_err(|_| Error::Unsupported))
                })
                .transpose()?,
            prior: if let Some(plan) = self.section_plan.as_ref().filter(|p| p.list_anchor()) {
                Some(
                    serde_json::to_value(ListSectionObservation {
                        children: plan.observe_children(fresh)?,
                        section: plan.batch().map(|b| b.observe(fresh)).transpose()?,
                    })
                    .map_err(|_| Error::Unsupported)?,
                )
            } else if let Some(body) = self.body() {
                Some(
                    serde_json::to_value(WholeObservation {
                        inline: body.observe(fresh)?,
                        section: self
                            .section_plan
                            .as_ref()
                            .and_then(|p| p.batch())
                            .map(|b| b.observe(fresh))
                            .transpose()?,
                    })
                    .map_err(|_| Error::Unsupported)?,
                )
            } else if let Some(p) = &self.preserved {
                Some(serde_json::to_value(p.observe(fresh)?).map_err(|_| Error::Unsupported)?)
            } else if let Some(batch) = self.section_plan.as_ref().and_then(|p| p.batch()) {
                Some(serde_json::to_value(batch.observe(fresh)?).map_err(|_| Error::Unsupported)?)
            } else {
                None
            },
            anchor: anchor.raw().raw().to_vec(),
            control: control.map(|s| s.raw().raw().to_vec()),
        })
    }
    /// Validate inert historical observations without creating a live review.
    /// Changed/deleted content may be evidence, but it must name the same objects.
    pub(in crate::canvas) fn validate_observed_bytes(
        &self,
        anchor: &[u8],
        control: Option<&[u8]>,
    ) -> Result<(), Error> {
        self.validate()?;
        let a = Section::decode(anchor)?;
        if a.id() != self.anchor()?.id()
            || a.thread_id()? != Some(self.thread.as_str())
            || a.document_id()? != Some(self.document.as_str())
        {
            return Err(Error::Changed);
        }
        if let Some(raw) = control {
            let c = Section::decode(raw)?;
            if c.id() != self.control_id()
                || c.thread_id()? != Some(self.thread.as_str())
                || c.document_id()? != Some(self.document.as_str())
            {
                return Err(Error::Changed);
            }
        }
        Ok(())
    }
    pub(in crate::canvas) fn bootstrap_current_with_controls(
        &self,
        fresh: &CanvasCollabSnapshot,
        canonical: &[u8],
        canonical_anchor: Option<&[u8]>,
        canonical_changed_controls: &[Vec<u8>],
    ) -> Result<(), Error> {
        super::comment_permission(fresh)?;
        self.changed_controls_current(canonical_changed_controls, fresh)?;
        if let Some(plan) = &self.section_plan {
            plan.children_current(fresh)?;
        }
        let observation = self.review_current(fresh)?;
        if let Some(preserved) = &self.preserved {
            preserved.current(fresh)?;
        }
        if let Some(body) = self.body() {
            body.current(fresh)?;
        }
        self.check_anchor(&Section::decode(&observation.anchor)?, false)?;
        match (self.has_parent_update(), canonical_anchor) {
            (false, None) => {}
            (true, Some(expected)) if expected == observation.anchor.as_slice() => {}
            _ => return Err(Error::Changed),
        }
        if observation.control.as_deref() != Some(canonical) {
            return Err(Error::Changed);
        }
        self.check_control(&Section::decode(canonical)?)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WholeObservation {
    inline: inline::PriorObservation,
    section: Option<control_batch::Observation>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListSectionObservation {
    children: super::list_body::Observation,
    section: Option<control_batch::Observation>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TableObservation {
    table: super::table_layout::Observation,
    annotation: Option<serde_json::Value>,
}
pub struct AnnotationObservation {
    prior: Option<serde_json::Value>,
    table: Option<serde_json::Value>,
    pub(super) record: AnnotationRecord,
    pub(super) anchor: Vec<u8>,
    pub(super) control: Option<Vec<u8>>,
}
impl AnnotationObservation {
    pub fn matches_current(&self, fresh: &CanvasCollabSnapshot) -> Result<bool, Error> {
        let current = self.record.review_current(fresh)?;
        Ok(current.prior == self.prior
            && current.table == self.table
            && current.anchor == self.anchor
            && current.control == self.control)
    }
    pub fn preview_blocks(&self) -> Result<Vec<serde_json::Value>, Error> {
        let observed = Section::decode(&self.anchor)?;
        if observed.section_type()? == Some(2) {
            // A current review describes canonical metadata only; it is not a
            // new native export or permission to replace the retained outcome.
            let body = super::image_body::ImageBody::metadata(&observed)?;
            return Ok(vec![serde_json::json!({"type":"rich_text","elements":[{
                "type":"rich_text_section","elements":[{"type":"text",
                "text":format!("Image description: {}",body.alt)}]
            }]})]);
        }
        if self
            .record
            .section_plan
            .as_ref()
            .is_some_and(|p| p.list_anchor())
        {
            let prior: ListSectionObservation =
                serde_json::from_value(self.prior.clone().ok_or(Error::Unsupported)?)
                    .map_err(|_| Error::Unsupported)?;
            let sections = prior.children.sections(&observed)?;
            if self
                .record
                .section_plan
                .as_ref()
                .is_some_and(|p| p.table_bodies())
            {
                return super::snippet::list_preview_observed(&sections, &observed);
            }
            return super::snippet::list_preview(&sections, &observed);
        }
        if (self.record.inline.is_some() || self.record.body().is_some())
            && self.record.check_anchor(&observed, false).is_ok()
        {
            // The exact admitted annotation changes no text/style; render its
            // original typed text while retaining the full marked raw evidence.
            return inline::original(&self.record.anchor()?).map(|d| d.draft.rich_blocks());
        }
        super::super::text::canonical_draft(&observed).map(|d| d.rich_blocks())
    }
    pub fn record(&self) -> &AnnotationRecord {
        &self.record
    }
    pub(in crate::canvas) fn prior_evidence(&self) -> Option<serde_json::Value> {
        self.table
            .as_ref()
            .map(|table| serde_json::json!({"table":table,"annotation":self.prior}))
            .or_else(|| self.prior.clone())
    }
    pub fn anchor_bytes(&self) -> &[u8] {
        &self.anchor
    }
    pub fn control_bytes(&self) -> Option<&[u8]> {
        self.control.as_deref()
    }
}
