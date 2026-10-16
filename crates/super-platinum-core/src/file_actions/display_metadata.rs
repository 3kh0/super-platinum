//! A deliberately small display-only patch; never media, access or content.
use super::*;

#[derive(Clone, Copy, Default)]
pub(crate) struct DisplayPatch<'a> {
    pub title: Option<&'a str>,
    pub name: Option<&'a str>,
    pub size: Option<u64>,
    pub pretty_type: Option<&'a str>,
}
impl<'a> DisplayPatch<'a> {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.name.is_none()
            && self.size.is_none()
            && self.pretty_type.is_none()
    }
    fn apply(&self, file: &mut File) {
        if let Some(title) = self.title {
            file.title = Some(title.into());
        }
        if let Some(name) = self.name {
            file.name = Some(name.into());
        }
        if let Some(size) = self.size {
            file.size = Some(size);
        }
        if let Some(kind) = self.pretty_type {
            file.pretty_type = Some(kind.into());
        }
    }
}
impl CoreAppState {
    /// A verified same-ID read may contribute independent display fields.
    /// Absent/null fields and empty type labels do not erase known metadata.
    pub fn merge_file_display_metadata(&mut self, team: &str, file: &File) -> bool {
        let Some(id) = file.id.as_deref().filter(|id| !id.is_empty()) else {
            return false;
        };
        if deleted(file)
            || self
                .workspaces
                .get(team)
                .is_some_and(|w| w.deleted_files.contains(id))
        {
            return false;
        }
        let title = file
            .title
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .or(file.name.as_deref().filter(|v| !v.trim().is_empty()))
            .map(|_| crate::state::file_title(file));
        let patch = DisplayPatch {
            title: title.as_deref(),
            name: title
                .as_ref()
                .and(file.name.as_deref().filter(|v| !v.trim().is_empty())),
            size: file.size,
            pretty_type: file.pretty_type.as_deref().filter(|v| !v.trim().is_empty()),
        };
        if patch.is_empty() {
            return false;
        }
        self.file_display_reads
            .record(self.account_epoch, team, id, patch);
        self.visit_files(team, |copy| {
            if copy.id.as_deref() == Some(id) && !deleted(copy) {
                patch.apply(copy);
            }
        });
        if self.later.matches_scope(self.account_epoch, team) {
            self.later.record_file_display_metadata(id, patch);
        }
        // Reuse the current rename policy: discard an index read already in flight.
        // This is not a guarantee about a later query against a lagging index.
        for files in [&mut self.files, &mut self.channel_files] {
            if files.matches_scope(self.account_epoch, team) && files.loading {
                files.generation = files.generation.wrapping_add(1);
                files.loading = false;
            }
        }
        true
    }
}
#[cfg(test)]
mod tests;
