//! Renderer custody is stricter than durable attachment descriptors: live
//! previews and all upload capabilities remain guarded across each native await.
use super_platinum_core::drafts::LocalDraft;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::view::composer) enum Policy {
    Plain,
    Baseline,
}
impl Policy {
    pub(in crate::view::composer) fn admits(self, local: &LocalDraft) -> bool {
        match self {
            Self::Plain => local.attachments.is_empty(),
            Self::Baseline => local.attachments.iter().all(|a| {
                !a.uploading
                    && a.upload_started.is_none()
                    && a.upload_cancel.is_none()
                    && a.upload_progress.is_none()
            }),
        }
    }
    pub(in crate::view::composer) fn same(self, a: &LocalDraft, b: &LocalDraft) -> bool {
        self.same_saved(a, b)
            && a.attachments
                .iter()
                .zip(&b.attachments)
                .all(|(a, b)| a.preview_path == b.preview_path)
    }
    /// Compare against an independently persisted review. preview_path is not
    /// persisted. The original live preview remains separately fenced by same().
    pub(in crate::view::composer) fn same_saved(self, a: &LocalDraft, b: &LocalDraft) -> bool {
        self.admits(a)
            && self.admits(b)
            && a.composer == b.composer
            && a.attachments.len() == b.attachments.len()
            && a.attachments.iter().zip(&b.attachments).all(|(a, b)| {
                a.id == b.id && a.path == b.path && a.name == b.name && a.bytes == b.bytes
            })
    }
    pub(in crate::view::composer) fn permits_action(
        self,
        action: &crate::drafts::protected::Action,
    ) -> bool {
        self == Self::Plain || matches!(action, crate::drafts::protected::Action::Baseline { .. })
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
