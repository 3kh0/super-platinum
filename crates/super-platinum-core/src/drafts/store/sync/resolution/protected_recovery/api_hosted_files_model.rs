//! Full ordered source set and lossless composer caption. No source permission.
use super::*;
use crate::slack::{
    api::file_share::{FileShareNote, batch::ExistingFilesShare},
    file_share_policy,
    models::File,
};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    snapshot: crate::drafts::sync::Snapshot,
    sources: Vec<serde_json::Value>,
}
impl Input {
    /// Captured sources must be in precisely the draft's current attachment order.
    /// This admits complete existing hosted files only; no pending uploads or grants.
    pub fn new(
        snapshot: crate::drafts::sync::Snapshot,
        sources: Vec<serde_json::Value>,
    ) -> Result<Self, Error> {
        let value = Self { snapshot, sources };
        value.validate()?;
        Ok(value)
    }
    pub fn snapshot(&self) -> &crate::drafts::sync::Snapshot {
        &self.snapshot
    }
    pub fn sources(&self) -> &[serde_json::Value] {
        &self.sources
    }
    pub(super) fn validate(&self) -> Result<(), Error> {
        if self.sources.len() != self.snapshot.file_ids.len()
            || !bounded(
                &serde_json::to_value(self).map_err(|_| Error::Unsupported)?,
                2 * 1024 * 1024,
            )
        {
            return Err(Error::Unsupported);
        }
        self.request()?;
        for (id, raw) in self.snapshot.file_ids.iter().zip(&self.sources) {
            if !raw.is_object()
                || raw.get("id").and_then(serde_json::Value::as_str) != Some(id.as_str())
                || !bounded(raw, 256 * 1024)
            {
                return Err(Error::Unsupported);
            }
            let file: File = serde_json::from_value(raw.clone()).map_err(|_| Error::Unsupported)?;
            if file.id.as_deref() != Some(id.as_str())
                || file_share_policy::source(&file) != file_share_policy::Eligibility::Allowed
            {
                return Err(Error::Unsupported);
            }
        }
        Ok(())
    }
    /// A value, not a dispatch permission. Caller still needs owned admission
    /// and fresh per-source and destination policy at the final network fence.
    pub fn request(&self) -> Result<ExistingFilesShare, Error> {
        let [destination] = self.snapshot.destinations.as_slice() else {
            return Err(Error::Unsupported);
        };
        let object = destination.as_object().ok_or(Error::Unsupported)?;
        if object.len() != 1 {
            return Err(Error::Unsupported);
        }
        let channel = object
            .get("channel_id")
            .and_then(serde_json::Value::as_str)
            .ok_or(Error::Unsupported)?;
        let note = (!self.snapshot.blocks.is_empty())
            .then(|| FileShareNote::new(self.snapshot.blocks.clone()))
            .transpose()
            .map_err(|_| Error::Unsupported)?;
        ExistingFilesShare::new(self.snapshot.file_ids.clone(), channel.into(), note)
            .map_err(|_| Error::Unsupported)
    }
    pub(super) fn paired(&self, review: &Review) -> Result<(), Error> {
        self.validate()?;
        if review.scope().key.thread.is_some() {
            return Err(Error::Unsupported);
        }
        let Some(expected) = review.row().state().desired() else {
            // File-only input has its own exact empty-caption proof. It never
            // changes the ordinary text codec or admits remote file_ids.
            if review.local().attachments.is_empty()
                && review.local().composer == crate::ComposerState::default()
                && self.snapshot.blocks.is_empty()
                && self.snapshot.destinations
                    == vec![serde_json::json!({"channel_id":review.scope().key.channel})]
            {
                return Ok(());
            }
            return Err(Error::Unsupported);
        };
        // Existing text proof remains unchanged. A dedicated hosted composer
        // adds the exact ordered source set; it never coerces remote file_ids.
        if !expected.file_ids.is_empty() {
            return Err(Error::Unsupported);
        }
        let mut caption = self.snapshot.clone();
        caption.file_ids.clear();
        if &caption != expected
            || self.snapshot.destinations
                != vec![serde_json::json!({"channel_id":review.scope().key.channel})]
        {
            return Err(Error::Stale);
        }
        facade::original_pair(&review.inner).map_err(|_| Error::Unsupported)
    }
}
pub(super) fn bounded(value: &serde_json::Value, limit: usize) -> bool {
    let mut nodes = vec![(value, 0usize)];
    let mut seen = 0;
    while let Some((v, depth)) = nodes.pop() {
        seen += 1;
        if depth > 32 || seen > limit || nodes.len() > limit {
            return false;
        }
        match v {
            serde_json::Value::Array(a) => nodes.extend(a.iter().map(|v| (v, depth + 1))),
            serde_json::Value::Object(o) => nodes.extend(o.values().map(|v| (v, depth + 1))),
            _ => (),
        }
    }
    struct Budget(usize);
    impl std::io::Write for Budget {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_sub(b.len())
                .ok_or(std::io::ErrorKind::FileTooLarge)?;
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Budget(limit), value).is_ok()
}
