//! Pure admission rules shared by typing, review leases and native projection.
use super::*;

pub(super) fn clean(review: &durable::Review) -> bool {
    review.blocked().is_none() && review.uncommitted().is_none() && review.alternates().is_empty()
}

/// Receipt-only changes and fresh verification may advance the base within the
/// same enrollment. Different local revision, content or inventory never does.
/// A revoked stream is separately checked at each modifying/dispatch admission.
pub(super) fn can_rebase(
    before: &durable::Review,
    after: &durable::Review,
    writer: &durable::Writer,
) -> bool {
    writer.scope() == before.scope()
        && writer.enrollment() == before.row().lineage().enrollment()
        && before.row().same_enrollment(after.row())
        && clean(before)
        && clean(after)
        && before.origins() == after.origins()
        && same_local(before.local(), after.local())
        && before.row().state().local_revision() == after.row().state().local_revision()
}

/// This proof remains attached to its immutable registered writer. A later
/// verification basis/epoch change never manufactures a new writer origin.
#[derive(Clone)]
pub(super) struct Capture {
    pub identity: Arc<()>,
    pub writer: durable::Writer,
    pub serial: u64,
    pub review: durable::Review,
    pub pending: Option<LocalDraft>,
    pub visible: LocalDraft,
    pub visit: Visit,
}
impl Capture {
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.identity, &other.identity)
            && self.writer.id() == other.writer.id()
            && self.writer.scope() == other.writer.scope()
            && self.writer.enrollment() == other.writer.enrollment()
            && self.serial == other.serial
            && self.review.same(&other.review)
            && same_local(&self.visible, &other.visible)
            && self.visit.same(&other.visit)
            && self.visit.current()
            && match (&self.pending, &other.pending) {
                (None, None) => true,
                (Some(left), Some(right)) => same_local(left, right),
                _ => false,
            }
    }
}

/// Displayed decisions retain the exact reviewed inventory. This wrapper cannot
/// be constructed by a UI from Operation, revision or rendered text alone.
pub(crate) struct ReviewLease {
    capture: Capture,
}
impl ReviewLease {
    pub(super) fn capture(capture: Capture) -> Result<Self, durable::Error> {
        if !capture.visit.current()
            || capture.writer.scope() != capture.review.scope()
            || capture.writer.enrollment() != capture.review.row().lineage().enrollment()
        {
            return Err(durable::Error::Stale);
        }
        Ok(Self { capture })
    }
    pub fn review(&self) -> &durable::Review {
        &self.capture.review
    }
    pub(super) fn visit(&self) -> Visit {
        self.capture.visit.clone()
    }
    pub(super) fn visible(&self) -> &LocalDraft {
        &self.capture.visible
    }
    /// Caller has just compared native scope/text/selection and holds the Editor
    /// state lock. The returned review is synchronously enqueued before unlock.
    pub(super) fn claim(self, current: &Capture) -> Result<durable::Review, durable::Error> {
        if !self.capture.same(current) || !self.capture.visit.claim() {
            return Err(durable::Error::Stale);
        }
        Ok(self.capture.review)
    }
}
