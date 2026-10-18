//! Bounded multi-control custody shared by live validation and inert records.
use super::control_update::ControlUpdate;
use super::{Error, RawMessage, Section};
use serde::{Deserialize, Serialize};

/// Stored only inside a new annotation record version. The legacy inline
/// preserved-control evidence must continue requiring byte-identical originals.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ControlBatch {
    updates: Vec<ControlUpdate>,
}

/// Actual accepted bytes, deliberately not Deserialize and never send authority.
#[derive(Clone)]
pub(super) struct AcknowledgedControls {
    batch: ControlBatch,
    sections: Vec<Section>,
}

/// Actual fresh canonical bytes. This is distinct from merely matching an ACK.
pub(super) struct ConfirmedControls {
    batch: ControlBatch,
    sections: Vec<Section>,
}

impl ControlBatch {
    pub(super) fn capture(updates: Vec<ControlUpdate>) -> Result<Self, Error> {
        let result = Self { updates };
        result.validate()?;
        Ok(result)
    }
    pub(super) fn validate(&self) -> Result<(), Error> {
        if self.updates.is_empty() || self.updates.len() > 4096 {
            return Err(Error::Unsupported);
        }
        let bytes = serde_json::to_vec(self).map_err(|_| Error::Unsupported)?;
        if bytes.len() > 512 * 1024 {
            return Err(Error::Unsupported);
        }
        let mut ids = std::collections::HashSet::new();
        let mut owner: Option<(String, String)> = None;
        for update in &self.updates {
            update.validate()?;
            let original = update.original()?;
            if !ids.insert(original.id().to_owned()) {
                return Err(Error::Unsupported);
            }
            let scope = (
                original
                    .document_id()?
                    .ok_or(Error::Unsupported)?
                    .to_owned(),
                original.thread_id()?.ok_or(Error::Unsupported)?.to_owned(),
            );
            if owner.as_ref().is_some_and(|expected| expected != &scope) {
                return Err(Error::Unsupported);
            }
            owner = Some(scope);
        }
        Ok(())
    }
    pub(super) fn len(&self) -> usize {
        self.updates.len()
    }
    pub(super) fn sections(
        &self,
        fresh: &super::CanvasCollabSnapshot,
    ) -> Result<Vec<Section>, Error> {
        let mut actual = Vec::new();
        for update in &self.updates {
            let original = update.original()?;
            let mut matches = fresh.sections.iter().filter(|s| s.id() == original.id());
            let found = matches.next().ok_or(Error::Unconfirmed)?;
            if matches.next().is_some() {
                return Err(Error::Unconfirmed);
            }
            actual.push(found.clone());
        }
        Ok(actual)
    }
    pub(super) fn current_metadata(
        &self,
        fresh: &super::CanvasCollabSnapshot,
    ) -> Result<(), Error> {
        let metadata = super::section_inventory::metadata(&fresh.document)?;
        for update in &self.updates {
            update.current_metadata(metadata.get(update.original()?.id()))?;
        }
        Ok(())
    }
    pub(super) fn check_ack_bytes(&self, raw: &[Vec<u8>]) -> Result<(), Error> {
        let sections = raw
            .iter()
            .map(|r| Section::decode(r).map_err(Into::into))
            .collect::<Result<Vec<_>, Error>>()?;
        self.acknowledge(&sections).map(|_| ())
    }
    pub(super) fn requests(&self, session: &str) -> Result<Vec<RawMessage>, Error> {
        self.validate()?;
        self.updates.iter().map(|u| u.request(session)).collect()
    }
    /// `received` must already be the exact old-control subset of the complete
    /// response. The annotation validator must also bind its new control/parent
    /// and total response count before calling this method.
    pub(super) fn acknowledge(&self, received: &[Section]) -> Result<AcknowledgedControls, Error> {
        self.validate()?;
        let sections = self.check_sections(received, true)?;
        Ok(AcknowledgedControls {
            batch: self.clone(),
            sections,
        })
    }
    fn check_sections(&self, received: &[Section], direct: bool) -> Result<Vec<Section>, Error> {
        if received.len() != self.updates.len() {
            return Err(Error::Unconfirmed);
        }
        let mut by_id = std::collections::HashMap::new();
        for section in received {
            if by_id.insert(section.id(), section).is_some() {
                return Err(Error::Unconfirmed);
            }
        }
        let mut result = Vec::with_capacity(self.updates.len());
        for update in &self.updates {
            let original = update.original()?;
            let actual = by_id.remove(original.id()).ok_or(Error::Unconfirmed)?;
            update.check_received(actual, direct)?;
            result.push(actual.clone());
        }
        if !by_id.is_empty() {
            return Err(Error::Unconfirmed);
        }
        Ok(result)
    }
    /// Read-only validation for persisted actual canonical bytes. The caller
    /// still checks complete record version/scope/anchor/new control and binds
    /// these exact bytes to a fresh complete load before a separate bootstrap.
    pub(super) fn check_canonical_bytes(&self, raw: &[Vec<u8>]) -> Result<(), Error> {
        self.validate()?;
        let sections = raw
            .iter()
            .map(|r| Section::decode(r).map_err(Into::into))
            .collect::<Result<Vec<_>, Error>>()?;
        self.check_sections(&sections, false)?;
        Ok(())
    }
}

impl AcknowledgedControls {
    /// A failed read borrows and preserves this result. No partial confirmation
    /// can consume the other successful controls or authorize Qpc bootstrap.
    pub(super) fn check_confirmation(
        &self,
        actual: &[Section],
        metadata: &std::collections::HashMap<String, RawMessage>,
    ) -> Result<(), Error> {
        let current = self.batch.check_sections(actual, false)?;
        for ((update, received), now) in self.batch.updates.iter().zip(&self.sections).zip(&current)
        {
            if now.sequence()? < received.sequence()? {
                return Err(Error::Unconfirmed);
            }
            update.current_metadata(metadata.get(now.id()))?;
        }
        Ok(())
    }
    pub(super) fn confirm(
        &self,
        actual: &[Section],
        metadata: &std::collections::HashMap<String, RawMessage>,
    ) -> Result<ConfirmedControls, Error> {
        self.check_confirmation(actual, metadata)?;
        Ok(ConfirmedControls {
            batch: self.batch.clone(),
            sections: self.batch.check_sections(actual, false)?,
        })
    }
    pub(super) fn check_snapshot(&self, fresh: &super::CanvasCollabSnapshot) -> Result<(), Error> {
        let actual = self.batch.sections(fresh)?;
        let metadata = super::section_inventory::metadata(&fresh.document)?
            .into_iter()
            .collect();
        self.check_confirmation(&actual, &metadata)
    }
    pub(super) fn snapshot_bytes(
        &self,
        fresh: &super::CanvasCollabSnapshot,
    ) -> Result<Vec<Vec<u8>>, Error> {
        self.check_snapshot(fresh)?;
        Ok(self
            .batch
            .sections(fresh)?
            .iter()
            .map(|s| s.raw().raw().to_vec())
            .collect())
    }
    pub(super) fn actual_bytes(&self) -> Vec<Vec<u8>> {
        self.sections
            .iter()
            .map(|s| s.raw().raw().to_vec())
            .collect()
    }
}
impl ConfirmedControls {
    pub(super) fn actual_bytes(&self) -> Vec<Vec<u8>> {
        self.sections
            .iter()
            .map(|s| s.raw().raw().to_vec())
            .collect()
    }
    pub(super) fn batch(&self) -> &ControlBatch {
        &self.batch
    }
}

/// Full actual state of every changed original control for explicit review.
/// Missing controls/metadata remain absence evidence, never success inference.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Observation {
    controls: Vec<Option<Vec<u8>>>,
    metadata: Vec<Option<Vec<u8>>>,
}
impl ControlBatch {
    pub(super) fn observe(
        &self,
        fresh: &super::CanvasCollabSnapshot,
    ) -> Result<Observation, Error> {
        let metadata = super::section_inventory::metadata(&fresh.document)?;
        let mut value = Observation {
            controls: vec![],
            metadata: vec![],
        };
        for update in &self.updates {
            let original = update.original()?;
            let mut matches = fresh.sections.iter().filter(|s| s.id() == original.id());
            let now = matches.next();
            if matches.next().is_some() {
                return Err(Error::Changed);
            }
            value.controls.push(now.map(|s| s.raw().raw().to_vec()));
            value
                .metadata
                .push(metadata.get(original.id()).map(|m| m.raw().to_vec()));
        }
        self.validate_observation(&value)?;
        Ok(value)
    }
    pub(super) fn validate_observation(&self, value: &Observation) -> Result<(), Error> {
        self.validate()?;
        if value.controls.len() != self.updates.len()
            || value.metadata.len() != self.updates.len()
            || value
                .controls
                .iter()
                .chain(&value.metadata)
                .flatten()
                .try_fold(0usize, |n, r| n.checked_add(r.len()))
                .is_none_or(|n| n > 512 * 1024)
        {
            return Err(Error::Unsupported);
        }
        for ((update, control), metadata) in self
            .updates
            .iter()
            .zip(&value.controls)
            .zip(&value.metadata)
        {
            let original = update.original()?;
            if let Some(raw) = control {
                let now = Section::decode(raw)?;
                if now.id() != original.id()
                    || now.thread_id()? != original.thread_id()?
                    || now.document_id()? != original.document_id()?
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
