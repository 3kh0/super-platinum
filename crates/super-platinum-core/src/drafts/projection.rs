//! Conditional in-memory installation of an already authorized projection.
use super::{DraftBook, DraftKey, LocalDraft};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CleanProjectionError {
    #[error("the target has an unconsumed local change")]
    DirtyTarget,
    #[error("the exact stored predecessor changed")]
    PredecessorMismatch,
    #[error("an attachment has active upload state or capabilities")]
    ActiveUpload,
}

fn active_upload(draft: &LocalDraft) -> bool {
    draft.attachments.iter().any(|file| {
        file.uploading
            || file.upload_started.is_some()
            || file.upload_cancel.is_some()
            || file.upload_progress.is_some()
    })
}

fn exact_idle(left: &LocalDraft, right: &LocalDraft) -> bool {
    left.composer == right.composer
        && left.attachments.len() == right.attachments.len()
        && left
            .attachments
            .iter()
            .zip(&right.attachments)
            .all(|(a, b)| {
                a.id == b.id
                    && a.path == b.path
                    && a.name == b.name
                    && a.bytes == b.bytes
                    && a.preview_path == b.preview_path
            })
}

impl DraftBook {
    /// Install only this key without recording local authorship or rebinding a
    /// composer. `None` requires absence; it does not mean a present empty draft.
    /// Empty replacements are removed using the existing Book empty semantics.
    ///
    /// This is not native acceptance or durable authorization. The caller must
    /// guard the live composer, account/visit, Editor serial and storage ticket,
    /// then install the paired live state in the same synchronous turn. Drained
    /// changes are not tracked here; caller authority must cover in-flight work.
    pub fn project_clean(
        &mut self,
        key: &DraftKey,
        expected: Option<&LocalDraft>,
        replacement: &LocalDraft,
    ) -> Result<(), CleanProjectionError> {
        if self.changed.contains_key(key) {
            return Err(CleanProjectionError::DirtyTarget);
        }
        let current = self.items.get(key);
        // Mutable upload handles cannot be certified by cloned LocalDrafts.
        // Refuse them even if both snapshots happen to share the same Arc.
        if current.is_some_and(active_upload)
            || expected.is_some_and(active_upload)
            || active_upload(replacement)
        {
            return Err(CleanProjectionError::ActiveUpload);
        }
        if !match (current, expected) {
            (None, None) => true,
            (Some(current), Some(expected)) => exact_idle(current, expected),
            _ => false,
        } {
            return Err(CleanProjectionError::PredecessorMismatch);
        }
        if replacement.empty() {
            self.items.remove(key);
        } else {
            self.items.insert(key.clone(), replacement.clone());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
