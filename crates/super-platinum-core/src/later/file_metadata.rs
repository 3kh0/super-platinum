//! Each changed field keeps its own revision; omission never renews evidence.
use super::*;
use crate::file_actions::display_metadata::DisplayPatch;

#[derive(Debug, Default)]
pub(super) struct FileUpdate {
    title: Option<(u64, String)>,
    name: Option<(u64, String)>,
    size: Option<(u64, u64)>,
    pretty_type: Option<(u64, String)>,
}
impl FileUpdate {
    pub(super) fn apply_since(&self, file: &mut File, requested: u64) {
        if let Some((at, value)) = &self.title
            && *at > requested
        {
            file.title = Some(value.clone());
        }
        if let Some((at, value)) = &self.name
            && *at > requested
        {
            file.name = Some(value.clone());
        }
        if let Some((at, value)) = self.size
            && at > requested
        {
            file.size = Some(value);
        }
        if let Some((at, value)) = &self.pretty_type
            && *at > requested
        {
            file.pretty_type = Some(value.clone());
        }
    }
}
impl LaterState {
    pub(crate) fn record_file_display_metadata(&mut self, id: &str, patch: DisplayPatch<'_>) {
        if id.is_empty() || patch.is_empty() || self.deleted_files.contains(id) {
            return;
        }
        let revision = self.next_hydration_revision();
        let entry = self.file_updates.entry(id.into()).or_default();
        if let Some(value) = patch.title {
            entry.title = Some((revision, value.into()));
        }
        if let Some(value) = patch.name {
            entry.name = Some((revision, value.into()));
        }
        if let Some(value) = patch.size {
            entry.size = Some((revision, value));
        }
        if let Some(value) = patch.pretty_type {
            entry.pretty_type = Some((revision, value.into()));
        }
    }
}
#[cfg(test)]
mod tests;
