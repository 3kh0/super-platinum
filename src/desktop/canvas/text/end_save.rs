//! Native paragraph-end Enter, durable pair custody and complete settlement.
use super::*;
use super_platinum_core::{
    canvas::edit::structure::EndWitness,
    slack::{api, transport::CanvasEditFailure},
};

pub(super) fn submit(
    mut state: Signal<ShellState>,
    reader: Signal<Reader>,
    mut draft: Signal<Option<Draft>>,
    generation: u64,
    witness: EndWitness,
) {
    let captured = {
        let mut d = draft.write();
        let Some(d) = d
            .as_mut()
            .filter(|d| d.generation == generation && d.pending.is_none())
        else {
            return;
        };
        d.reading = false;
        if !current(&state.peek(), d) || reader.peek().loading {
            d.error = Some("This canvas changed. Cancel and refresh before saving.");
            return;
        }
        if !matches!(&d.kind, Kind::Text(intent) if intent.heading_level().is_none()) {
            return;
        }
        (d.visit, d.read, d.section.clone())
    };
    let Some(now) = checklist::save::now_usec() else {
        return;
    };
    let (attempt, intent) = match checklist::end_ops::begin_end(
        &mut state.write(),
        captured.0,
        captured.1,
        &captured.2,
        witness,
        now,
    ) {
        Ok(value) => value,
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
    if let Some(d) = draft.write().as_mut() {
        d.pending = Some(attempt.clone());
    }
    if super::split_fixture::enabled(&state.peek()) {
        dioxus::core::spawn_forever(async move {
            if let Err(reason) = super::split_fixture::dispatch_end(state, &attempt, intent).await {
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
                "This canvas view changed before the edit was sent.",
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
            Ok(value) => value,
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
        };
        let prepared = if checklist::dispatchable(&state.peek(), &attempt) {
            checklist::save::now_usec()
                .map(|now| {
                    intent.prepare(
                        &fresh,
                        checklist::end_ops::witness(&attempt).expect("end attempt"),
                        now,
                    )
                })
                .transpose()
        } else {
            Ok(None)
        };
        let prepared = match prepared {
            Ok(Some(prepared)) => prepared,
            Err(super_platinum_core::canvas::edit::Error::Unchanged) => {
                cancel(state, draft, generation);
                return;
            }
            _ => {
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
        let ticket = match checklist::recovery::reserve_end(state, &attempt, &prepared).await {
            Ok(ticket) => ticket,
            Err(reason) => {
                reject(state, draft, generation, &attempt, reason);
                return;
            }
        };
        if !checklist::end_ops::mark_dispatched(&mut state.write(), &attempt, &prepared) {
            let _ = checklist::recovery::complete(state, &ticket).await;
            reject(
                state,
                draft,
                generation,
                &attempt,
                "This canvas view changed before the edit was sent.",
            );
            return;
        }
        let result = api::execute_canvas_end_edit(
            &ctx.transport,
            &ctx.client,
            &ctx.session,
            &fresh,
            prepared,
        )
        .await;
        let accepted = matches!(&result, Ok(_));
        // A definite transport NotSent releases the durable reservation first.
        // A failed disk commit preserves uncertainty and keeps the file locked.
        let not_sent_cleared = if matches!(&result, Err(CanvasEditFailure::NotSent(_))) {
            checklist::recovery::complete(state, &ticket).await
        } else {
            false
        };
        {
            let mut shell = state.write();
            match result {
                Ok(accepted) => {
                    checklist::end_ops::finish_end(&mut shell, &attempt, accepted);
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
                Err(CanvasEditFailure::Uncertain(_) | CanvasEditFailure::NotSent(_)) => {
                    checklist::finish(&mut shell, &attempt, checklist::Outcome::Uncertain);
                }
            }
        }
        let owned = checklist::presentation(&state.peek())
            .is_some_and(|view| view.visit == attempt.visit());
        if !owned {
            return;
        }
        if !accepted
            && checklist::presentation(&state.peek()).is_some_and(|view| view.operation.is_none())
        {
            if let Ok(mut d) = draft.try_write() {
                if let Some(d) = d.as_mut().filter(|d| d.generation == generation) {
                    d.pending = None;
                    d.error = Some(
                        "The paragraph change was not sent. Cancel and refresh before trying again.",
                    );
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
fn reject(
    mut state: Signal<ShellState>,
    mut draft: Signal<Option<Draft>>,
    generation: u64,
    attempt: &checklist::Attempt,
    reason: &'static str,
) {
    checklist::cancel_preflight(&mut state.write(), attempt);
    if checklist::presentation(&state.peek()).is_none_or(|view| view.visit != attempt.visit()) {
        return;
    }
    if let Ok(mut d) = draft.try_write() {
        if let Some(d) = d.as_mut().filter(|d| d.generation == generation) {
            d.pending = None;
            d.reading = false;
            d.error = Some(reason);
        }
    }
}
