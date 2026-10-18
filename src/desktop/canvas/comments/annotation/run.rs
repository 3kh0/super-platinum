use super::*;
fn millis() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| u64::try_from(d.as_millis()).ok())
}
fn failure(mut state: Signal<ShellState>, t: &Ticket, error: &Error) {
    if t.owner.scope.current(&state.peek()) {
        if let Error::RateLimited { retry_after_secs } = error {
            rate::remember(&state.peek(), *retry_after_secs);
        }
        state.write().report_failure(
            error,
            "You are offline. The comment remains retained.",
            || "This canvas request could not be completed.".into(),
        );
    }
}
pub(super) fn create(mut state: Signal<ShellState>, model: Signal<super::super::State>, t: Ticket) {
    use super_platinum_core::canvas::edit::annotation::{
        AnnotationPresentation, SectionAnnotationChoice,
    };
    let capture = {
        let shell = state.peek();
        if !t.current(&shell) {
            return;
        }
        let Some(e) = entry(&shell, &t.id) else {
            return;
        };
        if e.busy
            || e.pending.is_some()
            || e.accepted.is_some()
            || e.acknowledged.is_some()
            || e.verified.is_some()
        {
            return;
        }
        let Some(snapshot) = e.snapshot.as_ref() else {
            return;
        };
        if e.selected_range
            .as_ref()
            .is_some_and(|range| range.section != e.selected)
        {
            return;
        }
        if e.selected_image
            .as_ref()
            .is_some_and(|image| image.section != e.selected)
            || e.selected_image.is_some() && e.selected_range.is_some()
        {
            return;
        }
        (
            snapshot.clone(),
            e.selected.clone(),
            e.selected_range.clone(),
            e.selected_image.clone(),
        )
    };
    if !start(state, &t) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        // Complete canonical topology and snippet construction stay off the UI
        // event thread. Only original captured data crosses this worker boundary.
        let choice = tokio::task::spawn_blocking(move || {
            let (snapshot, selected, range, image) = capture;
            if let Some(range) = range {
                range::capture_intent(
                    &snapshot,
                    &selected,
                    range.start,
                    range.end,
                    range.removed.as_deref(),
                )
                .map(SectionAnnotationChoice::New)
            } else if let Some(image) = image {
                AnnotationPresentation::ordinary(None, None).and_then(|presentation| {
                    AnnotationIntent::for_image(&snapshot, &selected, &presentation, image.export)
                })
            } else {
                AnnotationPresentation::ordinary(None, None).and_then(|presentation| {
                    AnnotationIntent::for_section(&snapshot, &selected, &presentation)
                })
            }
        })
        .await;
        if !t.current(&state.peek()) {
            issue(state, &t, "The canvas view changed. Nothing was sent.");
            return;
        }
        let intent = match choice {
            Ok(Ok(SectionAnnotationChoice::New(intent))) => intent,
            Ok(Ok(SectionAnnotationChoice::Existing(control))) => {
                if let Some(e) = entry_mut(&mut state.write(), &t.id) {
                    e.busy = false;
                }
                // Actual ordinary reads retain the selected control binding.
                // No operation ID, anchor write, or Qpc bootstrap is created.
                super::super::selection::open(state, model, t.owner.clone(), control);
                return;
            }
            _ => {
                issue(
                    state,
                    &t,
                    "This paragraph or its comment anchors cannot be changed safely. Refresh the canvas before continuing.",
                );
                return;
            }
        };
        let Ok(snapshot) = fresh(state, &t).await else {
            issue(
                state,
                &t,
                "Could not check the selected paragraph. Nothing was sent.",
            );
            return;
        };
        let Some(now) = crate::canvas::checklist::save::now_usec() else {
            issue(
                state,
                &t,
                "The local clock is unavailable. Nothing was sent.",
            );
            return;
        };
        let prepared = tokio::task::spawn_blocking(move || {
            let prepared = intent.prepare_at(&snapshot, now)?;
            let record = prepared.record()?;
            Ok::<_, super_platinum_core::canvas::edit::Error>((snapshot, prepared, record))
        })
        .await;
        let Ok(Ok((snapshot, prepared, record))) = prepared else {
            issue(
                state,
                &t,
                "The paragraph or access changed. Refresh before creating this comment.",
            );
            return;
        };
        let scope = t.disk.scope.clone();
        let admitted = t
            .disk
            .work(move |j| {
                let result = j.begin_annotation(record)?;
                Ok((result, j.annotation(&scope)?))
            })
            .await;
        let pending = match admitted {
            Ok((AnnotationBegin::Started(_), Some(p))) => p,
            Ok((AnnotationBegin::Blocked(p), _)) => {
                if let Some(e) = entry_mut(&mut state.write(), &t.id) {
                    e.pending = Some(p);
                }
                issue(state, &t, UNKNOWN);
                return;
            }
            _ => {
                issue(state, &t, STORAGE);
                return;
            }
        };
        if let Some(e) = entry_mut(&mut state.write(), &t.id) {
            e.pending = Some(pending.clone());
        }
        let result = if crate::fixture::is_fixture() {
            let shell = state.peek();
            if !t.current(&shell) {
                Err(CanvasEditFailure::NotSent(Error::Api(
                    "annotation_owner_changed".into(),
                )))
            } else {
                fixture::anchor(&shell, &t.owner, &snapshot, prepared)
            }
        } else {
            let context = {
                let shell = state.peek();
                t.current(&shell)
                    .then(|| t.owner.scope.request(&shell))
                    .flatten()
            };
            let Some(ctx) = context else {
                not_sent(state, &t, pending).await;
                return;
            };
            api::execute_canvas_annotation(
                &ctx.transport,
                &ctx.client,
                &ctx.session,
                &snapshot,
                prepared,
            )
            .await
        };
        match result {
            Ok(ack) => {
                if let Some(e) = entry_mut(&mut state.write(), &t.id) {
                    // Evidence capture copies the already validated result; no
                    // dispatch/canonical authority can be reconstructed from it.
                    let evidence = ack.evidence().ok();
                    e.acknowledged = Some(RetainedAck {
                        pending,
                        proof: ack,
                        evidence,
                    });
                }
                continue_owned(state, t).await;
            }
            Err(CanvasEditFailure::NotSent(error)) => {
                failure(state, &t, &error);
                not_sent(state, &t, pending).await;
            }
            Err(CanvasEditFailure::Uncertain(error)) => {
                failure(state, &t, &error);
                issue(state, &t, UNKNOWN);
            }
        }
    });
}
async fn not_sent(mut state: Signal<ShellState>, t: &Ticket, pending: AnnotationPending) {
    let scope = t.disk.scope.clone();
    let result = t
        .disk
        .work(move |j| {
            let done = j.annotation_not_sent(&pending)?;
            Ok((done, j.annotation(&scope)?))
        })
        .await;
    if let Ok((true, pending)) = result {
        if let Some(e) = entry_mut(&mut state.write(), &t.id) {
            e.pending = pending;
        }
        issue(
            state,
            t,
            "This request was not sent. Review the current canvas before deliberately continuing.",
        );
    } else {
        issue(state, t, STORAGE);
    }
}
pub(super) fn resume(state: Signal<ShellState>, t: Ticket) {
    if start(state, &t) {
        dioxus::core::spawn_forever(continue_owned(state, t));
    }
}
async fn continue_owned(mut state: Signal<ShellState>, t: Ticket) {
    let accepted = entry(&state.peek(), &t.id).and_then(|e| e.accepted.clone());
    if let Some((pending, accepted)) = accepted {
        settle(state, &t, pending, accepted).await;
        return;
    }
    let acknowledgement = {
        let shell = state.peek();
        let ack = entry(&shell, &t.id).and_then(|e| e.acknowledged.as_ref());
        if ack.is_some_and(|a| a.evidence.is_none()) {
            drop(shell);
            issue(state, &t, STORAGE);
            return;
        }
        ack.and_then(|a| {
            a.evidence
                .as_ref()
                .map(|proof| (a.pending.clone(), proof.clone()))
        })
    };
    if let Some((pending, proof)) = acknowledgement {
        if t.disk
            .work(move |j| j.preserve_annotation_acknowledgement(&pending, &proof))
            .await
            != Ok(true)
        {
            issue(state, &t, STORAGE);
            return;
        }
    }
    // A fresh canonical read is necessary even when an ACK was retained. This
    // read alone cannot manufacture an ACK for an unknown AnchorStarted row.
    let Ok(snapshot) = fresh(state, &t).await else {
        issue(
            state,
            &t,
            "The accepted comment result is retained. Reopen the original canvas and retry its read.",
        );
        return;
    };
    let check = {
        let shell = state.peek();
        entry(&shell, &t.id)
            .and_then(|e| e.acknowledged.as_ref())
            .map(|a| a.proof.check_confirmation(&snapshot))
    };
    if check.as_ref().is_some_and(Result::is_err) {
        issue(
            state,
            &t,
            "Slack acknowledged the annotation, but the current anchor does not match yet. Its result is retained.",
        );
        return;
    }
    if check.is_some() {
        let ack = entry_mut(&mut state.write(), &t.id).and_then(|e| e.acknowledged.take());
        let Some(ack) = ack else {
            issue(state, &t, STORAGE);
            return;
        };
        // No await or mutation separates the successful borrowed check and move.
        let Ok(proof) = ack.proof.confirm(&snapshot) else {
            issue(state, &t, STORAGE);
            return;
        };
        if let Some(e) = entry_mut(&mut state.write(), &t.id) {
            e.verified = Some(Arc::new(proof));
            e.verified_receipt = Some(ack.pending.receipt().clone());
            e.retained_anchor = ack.evidence.map(|evidence| (ack.pending, evidence));
        }
    }
    let verified = {
        let shell = state.peek();
        entry(&shell, &t.id)
            .and_then(|e| e.verified.as_ref().zip(e.verified_receipt.as_ref()))
            .map(|(proof, receipt)| (proof.clone(), receipt.clone()))
    };
    if let Some((proof, receipt)) = verified {
        let scope = t.disk.scope.clone();
        let result = t
            .disk
            .work(move |j| {
                let done = j.annotation_anchor_confirmed(&receipt, &proof)?;
                Ok((done, j.annotation(&scope)?))
            })
            .await;
        match result {
            Ok((true, pending)) => {
                if let Some(e) = entry_mut(&mut state.write(), &t.id) {
                    e.pending = pending;
                    e.verified = None;
                    e.verified_receipt = None;
                    e.retained_anchor = None;
                }
            }
            _ => {
                issue(state, &t, STORAGE);
                return;
            }
        }
    }
    let pending = entry(&state.peek(), &t.id).and_then(|e| e.pending.clone());
    let Some(pending) = pending else {
        issue(state, &t, "No pending annotation needs continuation.");
        return;
    };
    if !matches!(
        pending.stage(),
        AnnotationStage::AnchorConfirmed | AnnotationStage::BootstrapNotSent
    ) {
        issue(state, &t, UNKNOWN);
        return;
    }
    if !t.current(&state.peek()) {
        issue(
            state,
            &t,
            "The original annotation is retained. Open its original canvas before continuing.",
        );
        return;
    }
    let Some(now) = millis() else {
        issue(
            state,
            &t,
            "Check the system clock before creating this thread.",
        );
        return;
    };
    let expected = pending.clone();
    let scope = t.disk.scope.clone();
    let admitted = t
        .disk
        .work(move |j| {
            let intent = j.continue_annotation_bootstrap(&expected, &snapshot, now)?;
            Ok((intent, j.annotation(&scope)?))
        })
        .await;
    let (intent, pending) = match admitted {
        Ok((Some(i), Some(p))) => (i, p),
        _ => {
            issue(state, &t, STORAGE);
            return;
        }
    };
    if let Some(e) = entry_mut(&mut state.write(), &t.id) {
        e.pending = Some(pending.clone());
    }
    let result = if crate::fixture::is_fixture() {
        let shell = state.peek();
        if !t.current(&shell) {
            Err(CanvasEditFailure::NotSent(Error::Api(
                "annotation_owner_changed".into(),
            )))
        } else {
            fixture::bootstrap(&shell, &t.owner, intent)
        }
    } else {
        let context = {
            let shell = state.peek();
            t.current(&shell)
                .then(|| t.owner.scope.request(&shell))
                .flatten()
        };
        let Some(ctx) = context else {
            not_sent(state, &t, pending).await;
            return;
        };
        api::execute_canvas_comment_bootstrap(&ctx.transport, &ctx.client, &ctx.session, intent)
            .await
    };
    match result {
        Ok(accepted) => {
            let accepted = Arc::new(accepted);
            if let Some(e) = entry_mut(&mut state.write(), &t.id) {
                e.accepted = Some((pending.clone(), accepted.clone()));
            }
            settle(state, &t, pending, accepted).await;
        }
        Err(CanvasEditFailure::NotSent(error)) => {
            failure(state, &t, &error);
            not_sent(state, &t, pending).await;
        }
        Err(CanvasEditFailure::Uncertain(error)) => {
            failure(state, &t, &error);
            issue(state, &t, UNKNOWN);
        }
    }
}
async fn settle(
    mut state: Signal<ShellState>,
    t: &Ticket,
    pending: AnnotationPending,
    accepted: Arc<BootstrapAccepted>,
) {
    let original = accepted.clone();
    let expected = pending.clone();
    let scope = t.disk.scope.clone();
    let result = t
        .disk
        .work(move |j| {
            let settled = j.annotation_bootstrap_accepted(&expected, &original)?;
            let preserved = if settled {
                true
            } else {
                j.preserve_annotation_root_result(&expected, &original)?
            };
            Ok((
                settled,
                preserved,
                j.annotation(&scope)?,
                j.annotation_history(&scope, None)?,
            ))
        })
        .await;
    if let Ok((settled, true, pending, history)) = result {
        if let Some(e) = entry_mut(&mut state.write(), &t.id) {
            e.created = Some(accepted.root().clone());
            e.accepted = None;
            e.pending = pending;
            e.history = history;
            e.history_after = None;
            e.history_back.clear();
            e.busy = false;
            e.error=(!settled).then_some("The original comment root arrived after local review. It is retained in history; another current attempt remains unchanged.");
        }
    } else {
        issue(state, t, STORAGE);
    }
}
pub(super) fn dispose(mut state: Signal<ShellState>, t: Ticket) {
    let expected = {
        let mut shell = state.write();
        if !t.current(&shell) {
            return;
        }
        let Some(e) = entry_mut(&mut shell, &t.id) else {
            return;
        };
        if !e.reviewed || e.busy || e.verified.is_some() || e.accepted.is_some() {
            return;
        }
        let evidence = e.acknowledged.as_ref().and_then(|ack| ack.evidence.clone());
        if e.acknowledged.is_some() && evidence.is_none() {
            return;
        }
        if e.acknowledged
            .as_ref()
            .is_some_and(|ack| e.pending.as_ref() != Some(&ack.pending))
        {
            return;
        }
        e.pending
            .clone()
            .zip(e.review.take())
            .map(|(pending, review)| (pending, review, evidence))
    };
    let Some((pending, observation, acknowledged)) = expected else {
        return;
    };
    if !start(state, &t) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let Ok(snapshot) = fresh(state, &t).await else {
            issue(
                state,
                &t,
                "The current anchor could not be reviewed. The original attempt remains locked.",
            );
            return;
        };
        if observation.matches_current(&snapshot) != Ok(true) {
            issue(
                state,
                &t,
                "The canvas changed after your review. Refresh and review the new state before removing the local lock.",
            );
            return;
        }
        let scope = t.disk.scope.clone();
        let result = t
            .disk
            .work(move |j| {
                let done = if let Some(ack) = acknowledged {
                    j.resolve_acknowledged_annotation_locally(
                        &pending,
                        observation,
                        ack,
                        AnnotationDecision::RemoveLocalLockOutcomeUnknown,
                    )?
                } else {
                    j.resolve_annotation_locally(
                        &pending,
                        observation,
                        AnnotationDecision::RemoveLocalLockOutcomeUnknown,
                    )?
                };
                Ok((
                    done,
                    j.annotation(&scope)?,
                    j.annotation_history(&scope, None)?,
                ))
            })
            .await;
        match result {
            Ok((true, pending, history)) => {
                if let Some(e) = entry_mut(&mut state.write(), &t.id) {
                    e.pending = pending;
                    e.acknowledged = None;
                    e.history = history;
                    e.history_after = None;
                    e.history_back.clear();
                    e.snapshot = Some(Arc::new(snapshot));
                    e.busy = false;
                    e.error = Some(
                        "The outcome is still unknown. Its original evidence is saved locally; no request was repeated.",
                    );
                }
            }
            _ => issue(state, &t, STORAGE),
        }
    });
}

/// Explicitly retire only in-memory proof after its durable evidence commits.
/// Any current journal lock remains unchanged and needs its own exact review.
pub(super) fn keep_ack_history(mut state: Signal<ShellState>, t: Ticket) {
    let value = {
        let shell = state.peek();
        if !t.current(&shell) {
            return;
        }
        entry(&shell, &t.id).filter(|e| !e.busy).and_then(|e| {
            e.acknowledged
                .as_ref()
                .and_then(|a| {
                    a.evidence
                        .as_ref()
                        .map(|proof| (a.pending.clone(), proof.clone()))
                })
                .or_else(|| e.retained_anchor.clone())
        })
    };
    let Some((pending, proof)) = value else {
        return;
    };
    if !start(state, &t) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let scope = t.disk.scope.clone();
        let result = t
            .disk
            .work(move |j| {
                Ok((
                    j.preserve_annotation_acknowledgement(&pending, &proof)?,
                    j.annotation(&scope)?,
                    j.annotation_history(&scope, None)?,
                ))
            })
            .await;
        match result {
            Ok((true, pending, history)) => {
                if let Some(e) = entry_mut(&mut state.write(), &t.id) {
                    e.acknowledged = None;
                    e.verified = None;
                    e.verified_receipt = None;
                    e.retained_anchor = None;
                    e.pending = pending;
                    e.history = history;
                    e.history_after = None;
                    e.history_back.clear();
                    e.busy = false;
                    e.error = Some(
                        "The accepted response is saved in local history. Any unresolved request remains locked until you review it; nothing was repeated.",
                    );
                }
            }
            _ => issue(state, &t, STORAGE),
        }
    });
}
