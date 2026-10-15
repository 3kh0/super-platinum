//! Reuse its exact single-use visit/lease; never use ordinary clear consumption.
use super::*;
use super_platinum_core::drafts::{
    store::sync::RejectedScheduleCommit, sync::RejectedScheduleChoice,
};

fn rejected(view: &View) -> bool {
    view.stored
        .as_ref()
        .is_some_and(|row| row.state.rejected_schedule().is_some())
}
fn valid_review(
    editor: &Arc<Editor>,
    state: &State,
    lease: &ScheduleLease,
    visit: &ScheduleVisit,
) -> Result<(), Fault> {
    if !Arc::ptr_eq(editor, &lease.editor)
        || !Arc::ptr_eq(&visit.0, &lease.visit.0)
        || !visit.active()
        || !available(state)
        || !rejected(&state.base)
        || state.serial != lease.serial
        || state.local_writer.id() != lease.writer
        || !same_schedule_view(&state.base, &lease.base)
    {
        return Err(Fault::Stale);
    }
    Ok(())
}
impl Editor {
    pub fn rejected_schedule_review_current(
        self: &Arc<Self>,
        lease: &ScheduleLease,
        visit: &ScheduleVisit,
    ) -> bool {
        valid_review(self, &self.state.lock().unwrap(), lease, visit).is_ok()
    }
    /// No native composer is replaced. Flush its current input, then review the
    /// frozen rejected intent independently of the complete paired LocalDraft.
    pub async fn prepare_rejected_schedule_review(
        self: &Arc<Self>,
        visit: ScheduleVisit,
    ) -> Result<ScheduleLease, Fault> {
        loop {
            self.flush().await?;
            let gate = self.gate.lock().await;
            if self.state.lock().unwrap().running {
                drop(gate);
                continue;
            }
            let (serial, writer, base) = {
                let state = self.state.lock().unwrap();
                if !available(&state) || !rejected(&state.base) || !visit.active() {
                    return Err(Fault::Stale);
                }
                (
                    state.serial,
                    state.local_writer.id().to_owned(),
                    state.base.clone(),
                )
            };
            let inventory = self.owner.review_local().await?;
            if inventory.scope != self.owner.scope
                || !clean_schedule_review(&inventory)
                || !same_schedule_view(&base, &schedule_view(&inventory))
            {
                return Err(Fault::Stale);
            }
            let lease = ScheduleLease {
                editor: self.clone(),
                visit: visit.clone(),
                serial,
                writer,
                base,
                inventory,
            };
            valid_review(self, &self.state.lock().unwrap(), &lease, &visit)?;
            return Ok(lease);
        }
    }
    pub async fn apply_rejected_schedule_review(
        self: &Arc<Self>,
        lease: ScheduleLease,
        visit: ScheduleVisit,
        choice: RejectedScheduleChoice,
        now_secs: i64,
    ) -> Result<Stored, Fault> {
        self.apply_rejected_schedule_using(
            lease,
            visit,
            choice,
            now_secs,
            |backend, expected, choice, now| async move {
                backend
                    .review_rejected_schedule(expected, choice, now)
                    .await
            },
        )
        .await
    }
    /// Native-thread caller guard runs after the final owner read and before
    /// claiming this exact review. Detached completion survives dialog removal.
    pub async fn apply_rejected_schedule_guarded<G>(
        self: &Arc<Self>,
        lease: ScheduleLease,
        visit: ScheduleVisit,
        choice: RejectedScheduleChoice,
        now_secs: i64,
        guard: G,
    ) -> Result<Stored, Fault>
    where
        G: FnOnce() -> Result<(), Fault> + 'static,
    {
        let editor = self.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        dioxus::core::spawn_forever(async move {
            let result = editor
                .apply_rejected_schedule_inner(
                    lease,
                    visit,
                    choice,
                    now_secs,
                    guard,
                    |backend, expected, choice, now| async move {
                        backend
                            .review_rejected_schedule(expected, choice, now)
                            .await
                    },
                )
                .await;
            let _ = send.send(result);
        });
        receive.await.map_err(|_| Fault::Storage)?
    }
    async fn apply_rejected_schedule_using<C, Fut>(
        self: &Arc<Self>,
        lease: ScheduleLease,
        visit: ScheduleVisit,
        choice: RejectedScheduleChoice,
        now_secs: i64,
        commit: C,
    ) -> Result<Stored, Fault>
    where
        C: FnOnce(Backend, LocalReview, RejectedScheduleChoice, i64) -> Fut + Send + 'static,
        Fut: Future<Output = Result<RejectedScheduleCommit, Fault>> + Send,
    {
        let editor = self.clone();
        self.runtime
            .spawn(async move {
                editor
                    .apply_rejected_schedule_inner(
                        lease,
                        visit,
                        choice,
                        now_secs,
                        || Ok(()),
                        commit,
                    )
                    .await
            })
            .await
            .map_err(|_| Fault::Storage)?
    }
    async fn apply_rejected_schedule_inner<G, C, Fut>(
        self: &Arc<Self>,
        lease: ScheduleLease,
        visit: ScheduleVisit,
        choice: RejectedScheduleChoice,
        now_secs: i64,
        caller_guard: G,
        commit: C,
    ) -> Result<Stored, Fault>
    where
        G: FnOnce() -> Result<(), Fault>,
        C: FnOnce(Backend, LocalReview, RejectedScheduleChoice, i64) -> Fut,
        Fut: Future<Output = Result<RejectedScheduleCommit, Fault>>,
    {
        loop {
            self.flush().await?;
            let gate = self.gate.lock().await;
            if self.state.lock().unwrap().running {
                drop(gate);
                continue;
            }
            valid_review(self, &self.state.lock().unwrap(), &lease, &visit)?;
            let mut admitted = false;
            let result = self
                .owner
                .review_rejected_schedule_with(
                    lease.inventory.clone(),
                    choice,
                    now_secs,
                    || {
                        let mut state = self.state.lock().unwrap();
                        valid_review(self, &state, &lease, &visit)?;
                        caller_guard()?;
                        visit.claim()?;
                        state.settling = true;
                        admitted = true;
                        Ok(())
                    },
                    commit,
                )
                .await;
            {
                let mut state = self.state.lock().unwrap();
                if admitted {
                    state.settling = false;
                    match &result {
                        Ok(view) => {
                            state.direct_local =
                                view.stored.as_ref().and_then(local::direct_intent);
                            state.base = view.clone();
                            // Pending input after admission still belongs to this
                            // same editor, now under the new or retried intent.
                        }
                        Err(error) => state.error = Some(*error),
                    }
                }
            }
            drop(gate);
            self.changed.notify_waiters();
            return result.map(|view| view.stored.expect("paired reviewed schedule result"));
        }
    }
}

#[cfg(test)]
#[path = "checkpoint_rejected_schedule_tests.rs"]
mod tests;
