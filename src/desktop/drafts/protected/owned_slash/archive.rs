//! Retained later checkpoint and archive, followed by current/native adoption.
use super::super::owned_route::Held;
use super::*;

pub(super) struct Archive {
    checkpoint: Arc<Held<o::CheckpointInput, durable::WriteResult>>,
    state: Mutex<ArchiveState>,
}
#[derive(Default)]
struct ArchiveState {
    job: Option<o::SlashArchiveJob>,
    accepted: Option<durable::Review>,
    historical: Option<durable::Review>,
    repaired: Option<o::SlashArchiveRepair>,
    adopted: Option<durable::Review>,
    archive_acknowledged: bool,
    checkpoint_acknowledged: bool,
    family_acknowledged: bool,
}
impl Archive {
    pub(super) fn ownership_stamp(&self) -> Result<ownership::Stamp, ownership::Error> {
        self.state.observe(|_| ()).map(|(s, ())| s)
    }
    pub(super) fn validate_ownership(
        &self,
        stamp: &ownership::Stamp,
    ) -> Result<(), ownership::Error> {
        self.state.validate(stamp)
    }
}

impl Slash {
    /// A terminal response alone never grants native clearing authority.
    pub(crate) fn acknowledged_without_clear(&self) -> bool {
        self.state.lock().is_ok_and(|s| {
            s.storage_state == Some(o::JobState::Ready)
                && s.fresh.is_none()
                && !s.fresh_consumed
                && matches!(
                    s.terminal.as_ref().map(Record::phase),
                    Some(
                        super_platinum_core::slash_commands::controller::Phase::Completed(
                            super_platinum_core::slash_commands::api::Reply::Acknowledged {
                                keep_input: true,
                                ..
                            }
                        )
                    )
                )
                && s.completion
                    .as_ref()
                    .is_some_and(|c| c.observe_acknowledged().is_ok())
        })
    }

    pub(crate) fn retired_success(&self) -> bool {
        self.state.lock().is_ok_and(|s| {
            s.archive
                .as_ref()
                .is_some_and(|a| a.state.lock().is_ok_and(|s| s.family_acknowledged))
        })
    }
}

struct Busy<'a>(&'a editor::Editor);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.state.lock() {
            state.busy = false;
        }
        self.0.notify();
    }
}

impl editor::Editor {
    /// Called only after the actual mounted clear/keep disposition has completed.
    /// A repeated same-value checkpoint gives the archive the current writer
    /// receipt. It is retained before submit and never sent through auto-ack.
    pub(crate) async fn archive_slash(
        self: &Arc<Self>,
        parent: &operation::Operation,
    ) -> Result<durable::Review, durable::Error> {
        if !parent.belongs(self) || !parent.current() {
            return Err(durable::Error::Stale);
        }
        let child = {
            let e = parent
                .record()
                .evidence
                .lock()
                .map_err(|_| durable::Error::Storage)?;
            if e.status != operation::Status::Finished {
                return Err(durable::Error::Pending);
            }
            e.owned_slash.clone().ok_or(durable::Error::Pending)?
        };
        let retained = child
            .state
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .archive
            .clone();
        if let Some(archive) = retained {
            let _gate = self.gate.lock().await;
            return self.progress_slash_archive(&child, &archive, true).await;
        }
        self.flush().await?;
        let _gate = self.gate.lock().await;
        let before = self.capture(Visit::new())?;
        let actual = self.owner.review().await?;
        let route = self
            .owner
            .account
            .owned
            .as_ref()
            .ok_or(durable::Error::Unsupported)?;
        let archive = {
            let mut s = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if s.retired
                || s.running
                || s.busy
                || s.parked
                || s.pending.is_some()
                || s.error.is_some()
                || s.retry.is_some()
                || s.released.is_some()
                || s.serial != before.serial
                || !s.review.same(&before.review)
                || !actual.same(&before.review)
                || !Arc::ptr_eq(&self.identity, &child.capture.identity)
                || s.writer.id() != child.capture.writer.id()
                || s.writer.enrollment() != child.capture.writer.enrollment()
            {
                return Err(durable::Error::Stale);
            }
            let mut held = child.state.lock().map_err(|_| durable::Error::Storage)?;
            if !held.reconciled || !held.canonical.as_ref().is_some_and(|r| r.same(&actual)) {
                return Err(durable::Error::ReviewRequired);
            }
            // A failed/held attempt is retained, never replaced by a new serial.
            if held.archive.is_some() {
                return Err(durable::Error::ReviewRequired);
            }
            child
                .completion_from(&held)?
                .observe_acknowledged()
                .map_err(ui)?;
            let serial = s.serial.checked_add(1).ok_or(durable::Error::Unsupported)?;
            let checkpoint = Held::new(route.operations.checkpoint(o::CheckpointInput {
                writer: s.writer.clone(),
                serial,
                expected: actual.clone(),
                local: actual.local().clone(),
                direct: s.direct.clone(),
            }));
            let archive = Arc::new(Archive {
                checkpoint,
                state: Mutex::default(),
            });
            held.archive = Some(archive.clone());
            s.serial = serial;
            archive
        };
        self.progress_slash_archive(&child, &archive, false).await
    }

    async fn progress_slash_archive(
        self: &Arc<Self>,
        child: &Arc<Slash>,
        archive: &Arc<Archive>,
        explicit_resume: bool,
    ) -> Result<durable::Review, durable::Error> {
        let checkpoint = archive.checkpoint.job().map_err(ui)?;
        let input = checkpoint.input();
        {
            let mut s = self.state.lock().map_err(|_| durable::Error::Storage)?;
            let held = archive.state.lock().map_err(|_| durable::Error::Storage)?;
            if s.retired
                || s.running
                || s.parked
                || s.pending.is_some()
                || s.error.is_some()
                || s.retry.is_some()
                || s.released.is_some()
                || (explicit_resume && s.busy)
                || s.serial != input.serial
                || s.writer.id() != input.writer.id()
                || s.writer.enrollment() != input.writer.enrollment()
                || !(s.review.same(&input.expected)
                    || held.accepted.as_ref().is_some_and(|r| r.same(&s.review))
                    || held.adopted.as_ref().is_some_and(|r| r.same(&s.review)))
            {
                return Err(durable::Error::Stale);
            }
            s.busy = true;
        }
        let _busy = Busy(self);
        let prior = archive
            .state
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .accepted
            .clone();
        let accepted = if let Some(prior) = prior {
            prior
        } else {
            if explicit_resume
                && matches!(
                    checkpoint.state(),
                    o::JobState::Failed(durable::Error::Storage)
                )
            {
                checkpoint.retry_storage().map_err(ui)?;
            } else {
                archive.checkpoint.submit().map_err(ui)?;
            }
            ready(archive.checkpoint.wait().await.map_err(ui)?)?;
            let (accepted, _) = checkpoint.observe_checkpoint().map_err(ui)?;
            archive
                .state
                .lock()
                .map_err(|_| durable::Error::Storage)?
                .accepted = Some(accepted.clone());
            accepted
        };
        let historical = archive
            .state
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .historical
            .clone();
        let historical = if let Some(historical) = historical {
            historical
        } else {
            let retained = archive
                .state
                .lock()
                .map_err(|_| durable::Error::Storage)?
                .job
                .clone();
            let job = if let Some(job) = retained {
                if explicit_resume
                    && matches!(job.state(), o::JobState::Failed(durable::Error::Storage))
                {
                    job.retry_storage().map_err(ui)?;
                } else {
                    job.submit().map_err(ui)?;
                }
                job
            } else {
                let actual = self.owner.review().await?;
                if !actual.same(&accepted) {
                    return Err(durable::Error::Stale);
                }
                let mut s = self.state.lock().map_err(|_| durable::Error::Storage)?;
                if s.serial != input.serial
                    || s.pending.is_some()
                    || s.running
                    || !(s.review.same(&input.expected) || s.review.same(&accepted))
                    || !same_local(accepted.local(), input.expected.local())
                {
                    return Err(durable::Error::Stale);
                }
                s.review = accepted.clone();
                let job = child
                    .completion()?
                    .archive_after_checkpoint(checkpoint)
                    .map_err(ui)?;
                archive
                    .state
                    .lock()
                    .map_err(|_| durable::Error::Storage)?
                    .job = Some(job.clone());
                job.submit().map_err(ui)?;
                job
            };
            let wait = job.clone();
            ready(
                tokio::task::spawn_blocking(move || wait.wait())
                    .await
                    .map_err(|_| durable::Error::Storage)?
                    .map_err(ui)?,
            )?;
            let (result, repaired) = match job.observe_success() {
                Ok((result, _)) => (result, None),
                Err(_) => {
                    let (result, reports, _) = job.observe_repaired_success().map_err(ui)?;
                    (result, Some(reports))
                }
            };
            let historical = result.historical_review().clone();
            {
                let mut held = archive.state.lock().map_err(|_| durable::Error::Storage)?;
                held.repaired = repaired;
                held.historical = Some(historical.clone());
            }
            historical
        };
        // Even an already-adopted result must pass a new current storage read.
        // A failed late read never reverts to the cached historical certificate.
        let current = self.owner.review().await?;
        if !current.same(&historical)
            || !editor_proof::can_rebase(&accepted, &current, &input.writer)
        {
            return Err(durable::Error::Stale);
        }
        let mut s = self.state.lock().map_err(|_| durable::Error::Storage)?;
        let held = archive.state.lock().map_err(|_| durable::Error::Storage)?;
        if s.retired
            || s.serial != input.serial
            || s.pending.is_some()
            || s.running
            || s.parked
            || s.error.is_some()
            || !(s.review.same(&accepted)
                || held.adopted.as_ref().is_some_and(|r| r.same(&s.review)))
        {
            return Err(durable::Error::Stale);
        }
        drop(held);
        s.review = current.clone();
        s.direct = current.row().state().local_preservation_intent().cloned();
        archive
            .state
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .adopted = Some(current.clone());
        Ok(current)
    }

    /// Caller holds a fresh native lease for the exact current input. No await,
    /// UI mutation, old archive result adoption, or new dispatch occurs here.
    pub(crate) fn acknowledge_slash<G: FnOnce() -> bool>(
        self: &Arc<Self>,
        parent: &operation::Operation,
        expected: &durable::Review,
        native_current: G,
    ) -> Result<(), durable::Error> {
        if !parent.belongs(self) || !parent.current() {
            return Err(durable::Error::Stale);
        }
        let evidence = parent
            .record()
            .evidence
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        if evidence.status != operation::Status::Finished {
            return Err(durable::Error::Pending);
        }
        let child = evidence
            .owned_slash
            .clone()
            .ok_or(durable::Error::Pending)?;
        drop(evidence);
        let editor = self.state.lock().map_err(|_| durable::Error::Storage)?;
        let child_state = child.state.lock().map_err(|_| durable::Error::Storage)?;
        let archive = child_state
            .archive
            .as_ref()
            .ok_or(durable::Error::Pending)?;
        let checkpoint = archive.checkpoint.job().map_err(ui)?;
        let mut s = archive.state.lock().map_err(|_| durable::Error::Storage)?;
        if editor.retired
            || editor.running
            || editor.busy
            || editor.parked
            || editor.pending.is_some()
            || editor.error.is_some()
            || editor.retry.is_some()
            || editor.serial != checkpoint.input().serial
            || !editor.review.same(expected)
            || !s.adopted.as_ref().is_some_and(|r| r.same(expected))
            || !native_current()
        {
            return Err(durable::Error::Stale);
        }
        if !s.archive_acknowledged {
            let job = s.job.as_ref().ok_or(durable::Error::Pending)?;
            if let Some(observed) = &s.repaired {
                let (historical, exact, ack) = job.observe_repaired_success().map_err(ui)?;
                if exact.failed_submissions() != observed.failed_submissions()
                    || exact.successful_submission() != observed.successful_submission()
                    || !s
                        .historical
                        .as_ref()
                        .is_some_and(|r| r.same(historical.historical_review()))
                {
                    return Err(durable::Error::Stale);
                }
                ack.acknowledge().map_err(|(e, _)| ui(e))?;
            } else {
                let (_, ack) = job.observe_success().map_err(ui)?;
                ack.acknowledge().map_err(|(e, _)| ui(e))?;
            }
            s.archive_acknowledged = true;
        }
        if !s.checkpoint_acknowledged {
            let (_, ack) = checkpoint.observe_checkpoint().map_err(ui)?;
            ack.acknowledge().map_err(|(e, _)| ui(e))?;
            s.checkpoint_acknowledged = true;
        }
        if !s.family_acknowledged {
            let (_, ack) = child
                .completion_from(&child_state)?
                .observe_acknowledged()
                .map_err(ui)?;
            ack.acknowledge().map_err(|(e, _)| ui(e))?;
            s.family_acknowledged = true;
        }
        Ok(())
    }
}
impl Slash {
    fn completion_from(&self, s: &State) -> Result<o::SlashCompletion, durable::Error> {
        s.completion.clone().ok_or(durable::Error::Pending)
    }
}
