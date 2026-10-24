//! Ordinary structural edits in saved split inventories keep pure proofs off the renderer.
use super::*;
use super_platinum_core::{
    canvas::edit::Error,
    slack::{api, transport::CanvasEditFailure},
};
pub(super) fn applies(shell: &ShellState) -> bool {
    checklist::presentation(shell)
        .and_then(|v| v.snapshot)
        .is_some_and(|s| {
            s.sections
                .iter()
                .any(|row| row.crdt().ok().flatten().is_some_and(|w| w.has(10)))
        })
}
async fn prepare(
    attempt: &checklist::Attempt,
    fresh: Arc<api::CanvasCollabSnapshot>,
    action: Action,
    now: u64,
) -> Result<checklist::structure_ops::PreparedTarget, Error> {
    let attempt = attempt.clone();
    tokio::task::spawn_blocking(move || {
        checklist::structure_ops::PreparedTarget::capture(attempt, &fresh, action, now)
    })
    .await
    .map_err(|_| Error::Unsupported)?
}
pub(super) fn submit(
    mut state: Signal<ShellState>,
    reader: Signal<Reader>,
    mut draft: Signal<Option<Draft>>,
    generation: u64,
    value: super_platinum_core::canvas::edit::text::TextDraft,
) {
    let captured = {
        let mut d = draft.write();
        let Some(d) = d
            .as_mut()
            .filter(|d| d.generation == generation && d.pending.is_none())
        else {
            return;
        };
        if !current(&state.peek(), d) || reader.peek().loading || d.kind.action().is_none() {
            d.error = Some("This canvas changed. Cancel and refresh before saving.");
            return;
        }
        d.reading = false;
        (d.visit, d.read, d.section.clone(), d.kind.action().unwrap())
    };
    let mut attempt = match checklist::structure_ops::begin_owner_delete(
        &mut state.write(),
        captured.0,
        captured.1,
        &captured.2,
        Arc::new(value),
    ) {
        Ok(a) => a,
        Err(reason) => {
            if let Some(d) = draft
                .write()
                .as_mut()
                .filter(|d| d.generation == generation)
            {
                d.error = Some(reason);
            }
            return;
        }
    };
    draft.write().as_mut().unwrap().pending = Some(attempt.clone());
    dioxus::core::spawn_forever(async move {
        let fixture = super::owner_delete_fixture::worker_fixture::enabled(&state.peek());
        let ctx = if fixture {
            None
        } else {
            attempt.request(&state.peek())
        };
        let fresh = if fixture {
            super::owner_delete_fixture::worker_fixture::fresh(&state.peek())
        } else {
            if crate::fixture::is_fixture() {
                reject(
                    state,
                    draft,
                    generation,
                    &attempt,
                    "The disconnected canvas fixture is unavailable.",
                );
                return;
            }
            let Some(ctx) = ctx.as_ref() else {
                reject(
                    state,
                    draft,
                    generation,
                    &attempt,
                    "This canvas changed before saving.",
                );
                return;
            };
            match api::fetch_canvas_collab(
                &ctx.transport,
                &ctx.client,
                &ctx.session,
                attempt.snapshot.file_id(),
            )
            .await
            {
                Ok(s) => Arc::new(s),
                Err(_) => {
                    reject(
                        state,
                        draft,
                        generation,
                        &attempt,
                        "Could not check the paragraph before saving. Cancel and refresh.",
                    );
                    return;
                }
            }
        };
        if !owned(&state.peek(), &draft.peek(), generation, &attempt) {
            reject(
                state,
                draft,
                generation,
                &attempt,
                "This canvas changed before saving.",
            );
            return;
        }
        let Some(now) = checklist::save::now_usec() else {
            return;
        };
        let prepared = match prepare(&attempt, fresh.clone(), captured.3, now).await {
            Ok(p) => p,
            Err(_) => {
                reject(
                    state,
                    draft,
                    generation,
                    &attempt,
                    "This paragraph or your access changed. Cancel and refresh.",
                );
                return;
            }
        };
        if !owned(&state.peek(), &draft.peek(), generation, &attempt) {
            reject(
                state,
                draft,
                generation,
                &attempt,
                "This canvas changed before saving.",
            );
            return;
        }
        let Some((bound, prepared, checkpoint)) = prepared.publish(&mut state.write()) else {
            reject(
                state,
                draft,
                generation,
                &attempt,
                "This canvas changed before saving.",
            );
            return;
        };
        attempt = bound;
        if let Some(d) = draft
            .write()
            .as_mut()
            .filter(|d| d.generation == generation)
        {
            d.pending = Some(attempt.clone());
        }
        if fixture {
            if let Err(reason) = super::owner_delete_fixture::worker_fixture::dispatch(
                state, &attempt, prepared, checkpoint,
            )
            .await
            {
                reject(state, draft, generation, &attempt, reason);
            }
            return;
        }
        let ctx = ctx.unwrap();
        let sealed = match api::prepare_canvas_structure_request(
            ctx.client.clone(),
            ctx.session.clone(),
            fresh,
            prepared,
        )
        .await
        {
            Ok(s) => s,
            Err(_) => {
                reject(
                    state,
                    draft,
                    generation,
                    &attempt,
                    "This paragraph or your access changed. Cancel and refresh.",
                );
                return;
            }
        };
        if !owned(&state.peek(), &draft.peek(), generation, &attempt) {
            reject(
                state,
                draft,
                generation,
                &attempt,
                "This canvas changed before saving.",
            );
            return;
        }
        let ticket = match checklist::recovery::reserve_structure_worker(
            state, &attempt, checkpoint,
        )
        .await
        {
            Ok(t) => t,
            Err(reason) => {
                reject(state, draft, generation, &attempt, reason);
                return;
            }
        };
        if !owned(&state.peek(), &draft.peek(), generation, &attempt)
            || !checklist::structure_ops::mark_dispatched(
                &mut state.write(),
                &attempt,
                sealed.edit(),
            )
        {
            let _ = checklist::recovery::complete(state, &ticket).await;
            reject(
                state,
                draft,
                generation,
                &attempt,
                "This canvas changed before saving.",
            );
            return;
        }
        let result = api::execute_prepared_canvas_structure_edit(&ctx.transport, sealed).await;
        let not_sent_cleared = if matches!(&result, Err(CanvasEditFailure::NotSent(_))) {
            checklist::recovery::complete(state, &ticket).await
        } else {
            false
        };
        {
            let mut shell = state.write();
            match result {
                Ok(a) => {
                    checklist::structure_ops::finish_structure(&mut shell, &attempt, a);
                }
                Err(CanvasEditFailure::NotSent(_)) if not_sent_cleared => {
                    checklist::finish(
                        &mut shell,
                        &attempt,
                        checklist::Outcome::Rejected(
                            "The paragraph change was not sent. Refresh before trying again.",
                        ),
                    );
                }
                Err(_) => {
                    checklist::finish(&mut shell, &attempt, checklist::Outcome::Uncertain);
                }
            }
        }
        let read = {
            let mut shell = state.write();
            checklist::pending_attempt(&shell)
                .filter(|p| p.serial == attempt.serial)
                .and_then(|_| checklist::begin_read(&mut shell))
        };
        if let Some(read) = read {
            super::super::loader::read_pair(state, reader, read).await;
        }
    });
}
fn owned(
    shell: &ShellState,
    draft: &Option<Draft>,
    generation: u64,
    attempt: &checklist::Attempt,
) -> bool {
    checklist::structure_ops::preparation_current(shell, attempt)
        && draft.as_ref().is_some_and(|d| {
            d.generation == generation
                && d.visit == attempt.visit()
                && d.pending
                    .as_ref()
                    .is_some_and(|a| a.serial == attempt.serial)
        })
}
fn reject(
    mut state: Signal<ShellState>,
    mut draft: Signal<Option<Draft>>,
    generation: u64,
    attempt: &checklist::Attempt,
    reason: &'static str,
) {
    checklist::cancel_preflight(&mut state.write(), attempt);
    if let Some(d) = draft
        .write()
        .as_mut()
        .filter(|d| d.generation == generation && d.visit == attempt.visit())
    {
        d.pending = None;
        d.reading = false;
        d.error = Some(reason);
    }
}
