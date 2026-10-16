//! Request-owned protection for accepted display changes during history reads.
//! No override survives the request, and a later history read starts empty.
use super::{deleted, display_metadata::DisplayPatch, visit_message};
use crate::{
    CoreAppState,
    slack::models::{File, Message},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, Weak},
};

const MAX_READS: usize = 32;
const MAX_FILES: usize = 256;
const MAX_BYTES: usize = 64 * 1024;
const RETIRED: &str = "history_file_display_read_superseded";

#[derive(Default)]
struct Update {
    title: Option<String>,
    name: Option<String>,
    size: Option<u64>,
    pretty_type: Option<String>,
}
impl Update {
    fn bytes_with(&self, patch: DisplayPatch<'_>) -> usize {
        patch
            .title
            .or(self.title.as_deref())
            .map_or(0, str::len)
            .saturating_add(patch.name.or(self.name.as_deref()).map_or(0, str::len))
            .saturating_add(
                patch
                    .pretty_type
                    .or(self.pretty_type.as_deref())
                    .map_or(0, str::len),
            )
    }
    fn merge(&mut self, patch: DisplayPatch<'_>) {
        if let Some(value) = patch.title {
            self.title = Some(value.into());
        }
        if let Some(value) = patch.name {
            self.name = Some(value.into());
        }
        if let Some(value) = patch.size {
            self.size = Some(value);
        }
        if let Some(value) = patch.pretty_type {
            self.pretty_type = Some(value.into());
        }
    }
    fn apply(&self, file: &mut File) {
        if let Some(value) = &self.title {
            file.title = Some(value.clone());
        }
        if let Some(value) = &self.name {
            file.name = Some(value.clone());
        }
        if let Some(value) = self.size {
            file.size = Some(value);
        }
        if let Some(value) = &self.pretty_type {
            file.pretty_type = Some(value.clone());
        }
    }
}

#[derive(Default)]
struct Pending {
    updates: BTreeMap<String, Update>,
    bytes: usize,
    failed: bool,
}
impl Pending {
    fn record(&mut self, id: &str, patch: DisplayPatch<'_>) {
        if self.failed {
            return;
        }
        let empty = Update::default();
        let previous = self.updates.get(id);
        let old = previous.map_or(0, |p| id.len() + p.bytes_with(Default::default()));
        let new = id
            .len()
            .saturating_add(previous.unwrap_or(&empty).bytes_with(patch));
        let bytes = self.bytes.saturating_sub(old).saturating_add(new);
        if bytes > MAX_BYTES || (previous.is_none() && self.updates.len() >= MAX_FILES) {
            // Never silently evict an intervening change then admit stale data.
            self.failed = true;
            self.updates.clear();
            self.bytes = 0;
            return;
        }
        self.updates.entry(id.into()).or_default().merge(patch);
        self.bytes = bytes;
    }
}
struct Read {
    epoch: u64,
    team: String,
    pending: Mutex<Pending>,
}

/// One history request's lifetime. Dropping it retires its display evidence.
/// Account/transport/navigation admission remains the adapter's responsibility.
pub struct FileDisplayRead(Arc<Read>);

/// Consumed request evidence; applies after the existing visible-message unwrap.
pub struct FileDisplayOverlay(BTreeMap<String, Update>);
impl FileDisplayOverlay {
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn message(&self, message: Message) -> Message {
        let mut message = crate::state::visible_message(message);
        if self.0.is_empty() {
            return message;
        }
        visit_message(&mut message, &mut |file| self.apply_file(file));
        message
    }

    /// In-place application for non-history ingress; never unwraps messages.
    pub(crate) fn apply_file(&self, file: &mut File) {
        if !self.0.is_empty()
            && !deleted(file)
            && let Some(update) = file.id.as_ref().and_then(|id| self.0.get(id))
        {
            update.apply(file);
        }
    }
}

#[derive(Default)]
pub(crate) struct DisplayReads {
    pending: Vec<Weak<Read>>,
}
impl DisplayReads {
    fn prune(&mut self, epoch: u64) {
        self.pending
            .retain(|read| read.upgrade().is_some_and(|r| r.epoch == epoch));
    }
    fn begin(&mut self, epoch: u64, team: &str) -> FileDisplayRead {
        self.prune(epoch);
        let read = Arc::new(Read {
            epoch,
            team: team.into(),
            pending: Mutex::new(Pending {
                failed: self.pending.len() >= MAX_READS,
                ..Default::default()
            }),
        });
        if self.pending.len() < MAX_READS {
            self.pending.push(Arc::downgrade(&read));
        }
        FileDisplayRead(read)
    }
    pub(super) fn record(&mut self, epoch: u64, team: &str, id: &str, patch: DisplayPatch<'_>) {
        self.prune(epoch);
        if id.is_empty() || patch.is_empty() {
            return;
        }
        for read in self.pending.iter().filter_map(Weak::upgrade) {
            if read.team == team
                && let Ok(mut pending) = read.pending.lock()
            {
                pending.record(id, patch);
            }
        }
    }
}

impl CoreAppState {
    /// Capture immediately before a bounded history/replies request, not after it.
    pub fn begin_file_display_read(&mut self, team: &str) -> FileDisplayRead {
        self.file_display_reads.begin(self.account_epoch, team)
    }
    /// Call only after the existing account/transport/navigation guards pass.
    /// Pressure/poison/foreign ownership refuses the entire response, not fields.
    pub fn finish_file_display_read(
        &self,
        team: &str,
        read: FileDisplayRead,
    ) -> Result<FileDisplayOverlay, &'static str> {
        if read.0.epoch != self.account_epoch || read.0.team != team {
            return Err(RETIRED);
        }
        let mut pending = read.0.pending.lock().map_err(|_| RETIRED)?;
        if pending.failed {
            return Err(RETIRED);
        }
        Ok(FileDisplayOverlay(std::mem::take(&mut pending.updates)))
    }
}

#[cfg(test)]
mod tests;
