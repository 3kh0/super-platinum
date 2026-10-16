//! File metadata requests share rename/deletion evidence with message previews,
//! but use a typed file cache and independent accepted-response revisions.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileHydrationFailure {
    Unavailable,
    Deleted,
}
impl LaterState {
    fn file_response_current(&self, id: &str, revision: u64) -> bool {
        !id.is_empty()
            && self
                .file_hydrated_revisions
                .get(id)
                .is_none_or(|accepted| *accepted < revision)
    }
    /// Account/transport/list guards belong to the adapter. An account scope
    /// initialized after deletion must also supply its workspace tombstone via
    /// file_unavailable_since before considering a file-info response.
    pub fn hydrate_file_since(
        &mut self,
        id: &str,
        mut file: File,
        requested_revision: u64,
    ) -> bool {
        if file.id.as_deref() != Some(id) || !self.file_response_current(id, requested_revision) {
            return false;
        }
        if crate::file_actions::deleted(&file) {
            self.record_file_deletion(id);
        }
        if self.deleted_files.contains(id) {
            file = crate::file_actions::tombstone(id);
            self.file_errors
                .insert(id.into(), FileHydrationFailure::Deleted);
        } else {
            if let Some(update) = self.file_updates.get(id) {
                update.apply_since(&mut file, requested_revision);
            }
            self.file_errors.remove(id);
        }
        self.file_hydrated_revisions
            .insert(id.into(), requested_revision);
        self.files.insert(id.into(), file);
        true
    }
    /// Missing permission/offline/inaccessible metadata does not remove the
    /// saved item or claim the file was deleted. Only known deletion does that.
    pub fn file_unavailable_since(
        &mut self,
        id: &str,
        requested_revision: u64,
        deleted: bool,
    ) -> bool {
        if !self.file_response_current(id, requested_revision) {
            return false;
        }
        if deleted {
            self.record_file_deletion(id);
            self.files
                .insert(id.into(), crate::file_actions::tombstone(id));
        }
        self.file_hydrated_revisions
            .insert(id.into(), requested_revision);
        self.file_errors.insert(
            id.into(),
            if self.deleted_files.contains(id) {
                FileHydrationFailure::Deleted
            } else {
                FileHydrationFailure::Unavailable
            },
        );
        true
    }
}

#[cfg(test)]
mod tests;
