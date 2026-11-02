//! Display-only occurrence; never a mutation, file or navigation capability.
use super::*;
use super_platinum_core::{MediaAssetId, MediaAssetKind};
#[derive(Clone)]
pub(crate) struct ImageDisplay {
    owner: Arc<Picker>,
    ready: Arc<Ready>,
    frame: u64,
    index: usize,
    label: String,
}
impl PartialEq for ImageDisplay {
    fn eq(&self, b: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &b.owner)
            && Arc::ptr_eq(&self.ready, &b.ready)
            && self.frame == b.frame
            && self.index == b.index
    }
}
impl ImageDisplay {
    pub(super) fn new(
        owner: Arc<Picker>,
        ready: Arc<Ready>,
        frame: u64,
        index: usize,
    ) -> Option<Self> {
        let label = format!(":{}:", ready.choices.get(index)?.name());
        Some(Self {
            owner,
            ready,
            frame,
            index,
            label,
        })
    }
    pub(crate) fn label(&self) -> &str {
        &self.label
    }
    pub(crate) fn asset(&self, s: &ShellState) -> Option<MediaAssetId> {
        if !self.owner.current(s) || !self.ready.current(s) {
            return None;
        }
        let session = s
            .thread_feed_reads
            .reaction_toggle
            .first_add
            .session
            .as_ref()?;
        if session.frame != self.frame
            || !session.custom.mode
            || !session
                .custom
                .ready
                .as_ref()
                .is_some_and(|r| Arc::ptr_eq(r, &self.ready))
            || !session
                .matches
                .iter()
                .take(session.limit)
                .any(|&i| i == self.index)
        {
            return None;
        }
        self.register()
    }
    fn register(&self) -> Option<MediaAssetId> {
        let choice = self.ready.choices.get(self.index)?;
        // Only visible, current, unslotted assets. No persistent-cache read,
        // provisional prior-URL pixels, raw URL DOM attribute or unseen sweep.
        Some(self.ready.receipt.media.register_image(
            MediaAssetKind::Emoji,
            choice.url(),
            "image/png",
        ))
    }
}

// One immutable render borrow, one complete original-root/currentness check.
// The returned IDs are display data only; ImageDisplay::asset remains fully
// guarded for retained callers, and action/prewire guards are unchanged.
pub(crate) fn assets(
    s: &ShellState,
    rows: &[super::super::picker::PresentedChoice],
) -> Vec<Option<MediaAssetId>> {
    let Some(first) = rows.iter().find_map(|r| r.image.as_ref()) else {
        return vec![None; rows.len()];
    };
    let Some(first_asset) = first.asset(s) else {
        return vec![None; rows.len()];
    };
    rows.iter()
        .map(|r| {
            let image = r.image.as_ref()?;
            if !Arc::ptr_eq(&image.owner, &first.owner)
                || !Arc::ptr_eq(&image.ready, &first.ready)
                || image.frame != first.frame
            {
                return None;
            }
            let session = s
                .thread_feed_reads
                .reaction_toggle
                .first_add
                .session
                .as_ref()?;
            if !session
                .matches
                .iter()
                .take(session.limit)
                .any(|&i| i == image.index)
            {
                return None;
            }
            if image.index == first.index {
                Some(first_asset.clone())
            } else {
                image.register()
            }
        })
        .collect()
}
