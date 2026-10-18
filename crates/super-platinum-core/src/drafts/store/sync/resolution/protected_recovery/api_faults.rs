//! Explicit isolated-route fault injection. Default workers cannot arm it.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaultPoint {
    InventoryRead,
    WriterCommit,
    LocalCommit,
    MutationReceipt,
    CheckCompletion,
    BaselineRetention,
    BaselineChoice,
    GapCommit,
    ReleaseCommit,
    StartLocalCommit,
}
#[cfg(any(test, feature = "protected-test-support"))]
std::thread_local! { static NEXT: std::cell::Cell<Option<FaultPoint>> = const { std::cell::Cell::new(None) }; }
#[cfg(any(test, feature = "protected-test-support"))]
pub(super) fn arm(point: FaultPoint) -> Result<(), Error> {
    NEXT.with(|next| {
        if next.get().is_some() {
            return Err(Error::Pending);
        }
        next.set(Some(point));
        Ok(())
    })
}
pub(super) fn before_commit(point: FaultPoint) -> Result<(), AppError> {
    #[cfg(any(test, feature = "protected-test-support"))]
    if NEXT.with(|next| {
        if next.get() == Some(point) {
            next.set(None);
            true
        } else {
            false
        }
    }) {
        return Err(invalid("isolated final-commit fault"));
    }
    let _ = point;
    Ok(())
}

// Core-only deterministic race probe, absent from dependency fixture features.
#[cfg(test)]
std::thread_local! { static PREPARED: std::cell::RefCell<Option<(std::sync::mpsc::Sender<()>,std::sync::mpsc::Receiver<()>)>> = const { std::cell::RefCell::new(None) }; }
#[cfg(test)]
pub(super) fn arm_prepared(
    entered: std::sync::mpsc::Sender<()>,
    resume: std::sync::mpsc::Receiver<()>,
) -> Result<(), Error> {
    PREPARED.with(|cell| {
        let mut value = cell.borrow_mut();
        if value.is_some() {
            return Err(Error::Pending);
        }
        *value = Some((entered, resume));
        Ok(())
    })
}
#[cfg(test)]
pub(super) fn pause_prepared() -> Result<(), AppError> {
    if let Some((entered, resume)) = PREPARED.with(|cell| cell.borrow_mut().take()) {
        entered
            .send(())
            .map_err(|_| invalid("race observer disappeared"))?;
        resume
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| invalid("race observer did not release"))?;
    }
    Ok(())
}
