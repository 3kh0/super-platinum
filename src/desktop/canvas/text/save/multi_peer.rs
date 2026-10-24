//! Multi-peer text saves keep exclusive actor custody across all worker awaits.
use super::*;
use checklist::session_worker::Lease;
use super_platinum_core::canvas::edit::{Error, text::PreparedTextEdit};
const FAILED: &str =
    "This canvas edit session is unavailable. Sign out and sign in again before editing.";
pub(in crate::canvas::text) async fn prepare(
    mut lease: Lease,
    intent: Arc<TextIntent>,
    fresh: Arc<api::CanvasCollabSnapshot>,
    desired: TextDraft,
    now: u64,
) -> Result<(Lease, Result<PreparedTextEdit, Error>), ()> {
    tokio::task::spawn_blocking(move || {
        let prepared = lease.prepare(&intent, &fresh, &desired, now);
        (lease, prepared)
    })
    .await
    .map_err(|_| ())
}
pub(super) fn submit(
    mut state: Signal<ShellState>,
    reader: Signal<Reader>,
    mut draft: Signal<Option<Draft>>,
    generation: u64,
    attempt: checklist::Attempt,
    intent: Arc<TextIntent>,
) {
    if super::super::split_fixture::enabled(&state.peek()) {
        dioxus::core::spawn_forever(async move {
            if let Err(reason) =
                super::super::split_fixture::dispatch_text_multi(state, &attempt, intent).await
            {
                reject(state, draft, generation, &attempt, reason);
            }
        });
        return;
    }
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
    dioxus::core::spawn_forever(async move {
        let Some(ctx) = attempt.request(&state.peek()) else {
            reject(
                state,
                draft,
                generation,
                &attempt,
                "This canvas changed before saving.",
            );
            return;
        };
        let fresh = match api::fetch_canvas_collab(
            &ctx.transport,
            &ctx.client,
            &ctx.session,
            attempt.snapshot.file_id(),
        )
        .await
        {
            Ok(value) => Arc::new(value),
            Err(_) => {
                reject(
                    state,
                    draft,
                    generation,
                    &attempt,
                    "Could not check this text before saving. Cancel and refresh.",
                );
                return;
            }
        };
        let acquired = {
            let mut shell = state.write();
            checklist::session_worker::acquire(&mut shell, &attempt)
        };
        let lease = match acquired {
            Ok(lease) => lease,
            Err(reason) => {
                reject(state, draft, generation, &attempt, reason);
                return;
            }
        };
        let claim = lease.claim();
        let Some(now) = checklist::save::now_usec() else {
            lease.restore(&mut state.write());
            reject(
                state,
                draft,
                generation,
                &attempt,
                "Check your system clock before editing.",
            );
            return;
        };
        let result = prepare(
            lease,
            intent,
            fresh.clone(),
            checklist::text_ops::draft(&attempt).unwrap().clone(),
            now,
        )
        .await;
        let (lease, prepared) = match result {
            Ok(result) => result,
            Err(()) => {
                claim.poison(&mut state.write());
                reject(state, draft, generation, &attempt, FAILED);
                return;
            }
        };
        if !claim.preparing(&state.peek(), &attempt) {
            lease.restore(&mut state.write());
            reject(
                state,
                draft,
                generation,
                &attempt,
                "This canvas changed before saving.",
            );
            return;
        }
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                lease.restore(&mut state.write());
                if error == Error::Unchanged {
                    cancel(state, draft, generation);
                } else {
                    reject(
                        state,
                        draft,
                        generation,
                        &attempt,
                        "This text or your access changed. Cancel and refresh.",
                    );
                }
                return;
            }
        };
        // All expensive HTTP validation/serialization finishes before the final
        // owner fence. The consuming API has no worker await before transport.
        let sealed = api::prepare_canvas_text_request(
            ctx.client.clone(),
            ctx.session.clone(),
            fresh,
            prepared,
        )
        .await;
        let sealed = match sealed {
            Ok(sealed) => sealed,
            Err(_) => {
                lease.restore(&mut state.write());
                reject(
                    state,
                    draft,
                    generation,
                    &attempt,
                    "This text or your access changed. Cancel and refresh.",
                );
                return;
            }
        };
        if !claim.preparing(&state.peek(), &attempt)
            || !checklist::text_ops::mark_dispatched(&mut state.write(), &attempt, sealed.edit())
        {
            lease.restore(&mut state.write());
            reject(
                state,
                draft,
                generation,
                &attempt,
                "This canvas changed before saving.",
            );
            return;
        }
        let result = api::execute_prepared_canvas_text_edit(&ctx.transport, sealed).await;
        match result {
            Ok(accepted) => {
                let mut lease = lease;
                let remembered = tokio::task::spawn_blocking(move || {
                    let memo = lease.remember(accepted);
                    (lease, memo)
                })
                .await;
                match remembered {
                    Ok((lease, memo)) => {
                        let mut shell = state.write();
                        lease.restore(&mut shell);
                        checklist::text_ops::finish_text_with_memo(&mut shell, &attempt, memo);
                    }
                    Err(_) => {
                        let mut shell = state.write();
                        claim.poison(&mut shell);
                        checklist::finish(&mut shell, &attempt, checklist::Outcome::Uncertain);
                    }
                }
            }
            Err(error) => {
                let mut shell = state.write();
                lease.restore(&mut shell);
                checklist::finish(
                    &mut shell,
                    &attempt,
                    match error {
                        CanvasEditFailure::NotSent(_) => checklist::Outcome::Rejected(
                            "The text edit was not sent. Refresh before trying again.",
                        ),
                        CanvasEditFailure::Uncertain(_) => checklist::Outcome::Uncertain,
                    },
                );
            }
        }
        if checklist::presentation(&state.peek()).is_none_or(|v| v.visit != attempt.visit()) {
            return;
        }
        let read = {
            let mut shell = state.write();
            checklist::pending_attempt(&shell)
                .filter(|pending| pending.serial == attempt.serial)
                .and_then(|_| checklist::begin_read(&mut shell))
        };
        if let Some(read) = read {
            checklist::reader::load(state, reader, read).await;
        } else if let Ok(mut value) = draft.try_write() {
            if let Some(d) = value.as_mut().filter(|d| d.generation == generation) {
                d.pending = None;
                d.error =
                    Some("The text edit was not sent. Cancel and refresh before trying again.");
            }
        }
    });
}
