//! One terminal handoff; lost replies require explicit fresh local renewal.
use super::*;
use super::{editor::Editor, local_writer::LocalWriter};

impl Editor {
    pub(super) async fn release_completion_owned(
        self: &Arc<Self>,
        accepted: durable::Review,
        intent: super_platinum_core::drafts::sync::ConsumeIntent,
        operation: operation::Operation,
    ) -> Result<(), durable::Error> {
        if !operation.belongs(self) {
            return Err(durable::Error::Stale);
        }
        operation.phase(operation::Phase::Release)?;
        self.clone()
            .release_inner(Some((accepted, intent)), Some(operation))
            .await
    }
    /// Explicit read-only repair when the local lease was retained but its
    /// first inventory read failed. This cannot repeat release/start_local.
    pub async fn refresh_local_handoff(self: &Arc<Self>) -> Result<(), durable::Error> {
        let editor = self.clone();
        self.runtime
            .spawn(async move {
                let _gate = editor.gate.lock().await;
                let (writer, serial) = {
                    let state = editor.state.lock().map_err(|_| durable::Error::Storage)?;
                    if state.retired || state.busy || state.handoff_lost {
                        return Err(durable::Error::Stale);
                    }
                    (
                        state.released.clone().ok_or(durable::Error::Stale)?,
                        state.serial,
                    )
                };
                let snapshot = writer.snapshot();
                if snapshot.running
                    || snapshot.pending.is_some()
                    || !snapshot.failed_inputs.is_empty()
                {
                    return Err(durable::Error::Pending);
                }
                writer.refresh_inventory().await?;
                let mut state = editor.state.lock().map_err(|_| durable::Error::Storage)?;
                if state.serial != serial
                    || state
                        .released
                        .as_ref()
                        .is_none_or(|current| !Arc::ptr_eq(current, &writer))
                {
                    return Err(durable::Error::Stale);
                }
                state.error = None;
                state.parked = false;
                drop(state);
                editor.notify();
                Ok(())
            })
            .await
            .map_err(|_| durable::Error::Storage)?
    }
    pub fn retry_local_storage(self: &Arc<Self>) -> Result<(), durable::Error> {
        let writer = self
            .state
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .released
            .clone()
            .ok_or(durable::Error::Stale)?;
        writer.retry_oldest_storage()
    }
    pub async fn release(self: &Arc<Self>) -> Result<(), durable::Error> {
        let editor = self.clone();
        self.runtime
            .spawn(async move { editor.release_inner(None, None).await })
            .await
            .map_err(|_| durable::Error::Storage)?
    }
    pub(super) async fn release_completion(
        self: &Arc<Self>,
        accepted: durable::Review,
        intent: super_platinum_core::drafts::sync::ConsumeIntent,
    ) -> Result<(), durable::Error> {
        let editor = self.clone();
        self.runtime
            .spawn(async move { editor.release_inner(Some((accepted, intent)), None).await })
            .await
            .map_err(|_| durable::Error::Storage)?
    }
    async fn release_inner(
        self: Arc<Self>,
        completion: Option<(
            durable::Review,
            super_platinum_core::drafts::sync::ConsumeIntent,
        )>,
        operation: Option<operation::Operation>,
    ) -> Result<(), durable::Error> {
        self.flush().await?;
        let _gate = self.gate.lock().await;
        let fresh = self.owner.review().await?;
        if completion.as_ref().is_some_and(|(accepted, intent)| {
            !accepted.row().same_enrollment(fresh.row())
                || accepted.row().state().local_preservation_intent() != Some(intent)
                || fresh.row().state().local_preservation_intent() != Some(intent)
                || fresh.row().state().consumption().is_some()
        }) {
            return Err(durable::Error::Stale);
        }
        let (expected, writer, reply) = {
            let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if state.retired
                || state.running
                || state.busy
                || state.pending.is_some()
                || state.parked
                || state.released.is_some()
                || state.handoff_lost
                || !editor_proof::can_rebase(&state.review, &fresh, &state.writer)
            {
                return Err(durable::Error::Stale);
            }
            if operation.as_ref().is_some_and(|op| !op.current()) {
                return Err(durable::Error::Stale);
            }
            let reply = self.owner.release(fresh.clone())?;
            state.busy = true;
            // From this enqueue onward a missing reply may mean committed
            // release. A second release call is never a recovery mechanism.
            state.handoff_lost = true;
            state.handoff_expected = Some(fresh.clone());
            (fresh, state.writer.clone(), reply)
        };
        let result = async {
            let release = account::wait(reply).await?;
            if let Some(operation) = &operation {
                operation.retain(|s| s.release = Some(release.clone()))?;
                operation.phase(operation::Phase::StartLocal)?;
            }
            let lease = account::wait(self.owner.client()?.start_local(release, writer)).await?;
            if let Some(operation) = &operation {
                operation.retain(|s| s.local_lease = Some(lease.clone()))?;
            }
            let local = LocalWriter::from_handoff(
                self.owner.account.clone(),
                lease,
                expected.local().clone(),
            )?;
            // Retain the lease before a fallible inventory read. No retry of
            // start_local is possible through this branch.
            let inventory = local.refresh_inventory().await;
            self.install_local(local)?;
            inventory
        }
        .await;
        if let Some(operation) = &operation {
            operation.retain(|s| s.handoff = Some(result))?;
        }
        {
            let mut state = self.state.lock().unwrap();
            state.busy = false;
            if let Err(error) = result {
                state.error = Some(error);
                state.parked = true;
            }
        }
        self.notify();
        result
    }
    fn install_local(&self, writer: Arc<LocalWriter>) -> Result<(), durable::Error> {
        let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if let Some((_, pending)) = &state.pending {
            writer.submit(pending)?;
        }
        state.pending = None;
        state.released = Some(writer);
        state.running = false;
        state.handoff_lost = false;
        state.handoff_expected = None;
        state.error = None;
        state.parked = false;
        state.barrier = None;
        Ok(())
    }
    /// A user-requested local-only repair after lost release/start reply. It
    /// cannot recreate managed ownership or regain a remote verification basis.
    pub async fn renew_lost_handoff(self: &Arc<Self>) -> Result<(), durable::Error> {
        let editor = self.clone();
        self.runtime
            .spawn(async move {
                let _gate = editor.gate.lock().await;
                let expected = {
                    let mut state = editor.state.lock().map_err(|_| durable::Error::Storage)?;
                    if !state.handoff_lost || state.busy || state.released.is_some() {
                        return Err(durable::Error::Stale);
                    }
                    let expected = state
                        .handoff_expected
                        .clone()
                        .ok_or(durable::Error::Stale)?;
                    state.busy = true;
                    expected
                };
                let result = async {
                    let inventory = editor.owner.local_inventory().await?;
                    let writer = if inventory.row().is_none() {
                        // Release committed: renew from exact current inventory.
                        // Never reuse the previously consumed Release token.
                        LocalWriter::renew(editor.owner.account.clone(), inventory).await?
                    } else {
                        // The original release can have rolled back. Only its
                        // exact still-active full proof permits a new release CAS.
                        let current = editor.owner.review().await?;
                        if !current.same(&expected) {
                            return Err(durable::Error::ReviewRequired);
                        }
                        let registered = editor
                            .state
                            .lock()
                            .map_err(|_| durable::Error::Storage)?
                            .writer
                            .clone();
                        let release = account::wait(editor.owner.release(current.clone())?).await?;
                        let lease =
                            account::wait(editor.owner.client()?.start_local(release, registered))
                                .await?;
                        let writer = LocalWriter::from_handoff(
                            editor.owner.account.clone(),
                            lease,
                            current.local().clone(),
                        )?;
                        writer.refresh_inventory().await?;
                        writer
                    };
                    editor.install_local(writer)?;
                    Ok(())
                }
                .await;
                {
                    let mut state = editor.state.lock().unwrap();
                    state.busy = false;
                    if let Err(error) = result {
                        state.error = Some(error);
                        state.parked = true;
                    }
                }
                editor.notify();
                result
            })
            .await
            .map_err(|_| durable::Error::Storage)?
    }
}
