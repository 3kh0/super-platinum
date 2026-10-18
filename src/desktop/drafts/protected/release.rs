//! One terminal handoff; lost replies require explicit fresh local renewal.
use super::*;
use super::{editor::Editor, local_writer::LocalWriter};

enum HandoffReply {
    Legacy(durable::Reply<durable::Release>),
    Owned(Arc<super::owned_release::Release>, operation::Operation),
}
impl Editor {
    pub(super) async fn retry_owned_handoff(
        self: &Arc<Self>,
        parent: operation::Operation,
    ) -> Result<(), durable::Error> {
        if self.owner.account.owned.is_none() || !parent.belongs(self) {
            return Err(durable::Error::Stale);
        }
        let editor = self.clone();
        self.runtime
            .spawn(async move {
                let _gate = editor.gate.lock().await;
                let held = parent
                    .record()
                    .evidence
                    .lock()
                    .map_err(|_| durable::Error::Storage)?
                    .owned_release
                    .clone()
                    .ok_or(durable::Error::Pending)?;
                let expected = held
                    .held
                    .job()
                    .map_err(super::owned_route::ui)?
                    .input()
                    .expected
                    .clone();
                {
                    let state = editor.state.lock().map_err(|_| durable::Error::Storage)?;
                    if state.busy
                        || state.running
                        || !state.handoff_lost
                        || state.released.is_some()
                        || state
                            .handoff_expected
                            .as_ref()
                            .is_none_or(|v| !v.same(&expected))
                    {
                        return Err(durable::Error::Stale);
                    }
                }
                // Storage completion can finish after retirement; installation cannot.
                let lease = held.retry_storage(&parent).await?;
                if !parent.current()
                    || editor
                        .state
                        .lock()
                        .map_err(|_| durable::Error::Storage)?
                        .retired
                {
                    return Err(durable::Error::Stale);
                }
                let local = LocalWriter::from_handoff(
                    editor.owner.account.clone(),
                    lease,
                    expected.local().clone(),
                )?;
                let inventory = local.refresh_inventory().await;
                let installed = editor.install_local(local, Some((&parent, &expected)));
                let result = installed.and(inventory);
                parent.retain(|s| s.handoff = Some(result))?;
                editor.notify();
                result
            })
            .await
            .map_err(|_| durable::Error::Storage)?
    }

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
            let routed = self.owner.account.owned.clone();
            let reply = if routed.is_some() {
                {
                    let route = routed.as_ref().expect("checked route");
                    let parent = operation.as_ref().ok_or(durable::Error::Unsupported)?;
                    let held = super::owned_release::Release::new(
                        route,
                        super::owned_route::o::ReleaseInput {
                            expected: fresh.clone(),
                            writer: state.writer.clone(),
                        },
                    );
                    parent.retain(|s| s.owned_release = Some(held.clone()))?;
                    held.held.submit().map_err(super::owned_route::ui)?;
                    HandoffReply::Owned(held, parent.clone())
                }
            } else {
                HandoffReply::Legacy(self.owner.release(fresh.clone())?)
            };
            state.busy = true;
            // From this enqueue onward a missing reply may mean committed
            // release. A second release call is never a recovery mechanism.
            state.handoff_lost = true;
            state.handoff_expected = Some(fresh.clone());
            (fresh, state.writer.clone(), reply)
        };
        let result = async {
            let lease = match reply {
                HandoffReply::Legacy(reply) => {
                    let release = account::wait(reply).await?;
                    if let Some(operation) = &operation {
                        operation.retain(|s| s.release = Some(release.clone()))?;
                        operation.phase(operation::Phase::StartLocal)?;
                    }
                    let lease =
                        account::wait(self.owner.client()?.start_local(release, writer)).await?;
                    if let Some(operation) = &operation {
                        operation.retain(|s| s.local_lease = Some(lease.clone()))?;
                    }

                    lease
                }
                HandoffReply::Owned(held, parent) => {
                    let lease = held.finish(&parent).await?;
                    if !parent.current() {
                        return Err(durable::Error::Stale);
                    }
                    lease
                }
            };
            let local = LocalWriter::from_handoff(
                self.owner.account.clone(),
                lease,
                expected.local().clone(),
            )?;
            // Retain the lease before a fallible inventory read. No retry of
            // start_local is possible through this branch.
            let inventory = local.refresh_inventory().await;
            self.install_local(local, operation.as_ref().map(|p| (p, &expected)))?;
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
    fn install_local(
        self: &Arc<Self>,
        writer: Arc<LocalWriter>,
        authority: Option<(&operation::Operation, &durable::Review)>,
    ) -> Result<(), durable::Error> {
        let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if self.owner.account.owned.is_some() {
            let (parent, expected) = authority.ok_or(durable::Error::Unsupported)?;
            if !parent.current()
                || !parent.belongs(self)
                || state.retired
                || state
                    .handoff_expected
                    .as_ref()
                    .is_none_or(|v| !v.same(expected))
            {
                return Err(durable::Error::Stale);
            }
            // Retain the actual issued lease before a pending-input submission
            // can fail. Never discard this LocalWriter and renew another one.
            state.released = Some(writer.clone());
            state.handoff_lost = false;
        }
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
                    editor.install_local(writer, None)?;
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
