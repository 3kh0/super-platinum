//! No non-test constructor exists. A future constructor must consume real cold
//! process/Owned/C3 drain evidence, not bools, IDs, or an inspected SQL receipt.
use super::*;

pub(super) struct Authority {
    owner: Arc<Owned>,
    review: Arc<Review>,
}
impl Authority {
    pub(super) fn owner(&self) -> &Arc<Owned> {
        &self.owner
    }
    pub(super) fn expected(&self) -> &Image {
        self.review.image.as_ref().unwrap()
    }
    pub(super) fn identity_current(&self) -> Result<(), AppError> {
        if !Arc::ptr_eq(&self.owner, &self.review.owner)
            || self.review.class != Class::Protected7
            || self.review.image.is_none()
        {
            return Err(invalid("rich authority owner changed"));
        }
        self.owner.check()?;
        if self.owner.retain_db()?.as_ref().map(|(_, id)| id) != self.review.identity.as_ref() {
            return Err(invalid("rich reviewed file changed"));
        }
        Ok(())
    }
    // Closed-file preflight only. Never call while our own SQLite transaction
    // owns a rollback journal or SQLite byte-range locks.
    pub(super) fn preflight(&self) -> Result<(), AppError> {
        self.identity_current()?;
        self.owner.readonly_preflight(
            self.review
                ._file
                .as_ref()
                .ok_or_else(|| invalid("rich retained file missing"))?,
        )?;
        Ok(())
    }
    /// Isolated tests only. This is deliberately not a proof of deployment-wide
    /// process quiescence; no non-test path can manufacture this value.
    #[cfg(test)]
    pub(super) fn fixture(owner: Arc<Owned>, review: Arc<Review>) -> Result<Self, AppError> {
        let value = Self { owner, review };
        value.preflight()?;
        Ok(value)
    }
}
