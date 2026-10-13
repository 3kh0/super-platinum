//! Canonical preflight, one dispatch, and retained text receipt settlement.
use super::*;
use super_platinum_core::{
    canvas::edit::text::TextDraft,
    slack::{api, transport::CanvasEditFailure},
};

pub(super) fn submit(
    mut state: Signal<ShellState>,
    reader: Signal<Reader>,
    mut draft: Signal<Option<Draft>>,
    generation: u64,
    value: TextDraft,
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
        if value == *d.intent.seed() {
            None
        } else {
            Some((d.visit, d.read, d.section.clone(), Arc::clone(&d.intent)))
        }
    };
    let Some(captured) = captured else {
        draft.set(None);
        return;
    };
    let attempt = match checklist::text_ops::begin_text(
        &mut state.write(),
        captured.0,
        captured.1,
        &captured.2,
        Arc::new(value),
    ) {
        Ok(attempt) => attempt,
        Err(reason) => {
            if let Some(d) = draft.write().as_mut() {
                d.error = Some(reason);
            }
            return;
        }
    };
    if let Some(d) = draft.write().as_mut() {
        d.pending = Some(attempt.clone());
    }
    if super::fixture::enabled(&state.peek()) {
        super::fixture::dispatch(&mut state.write(), &attempt, captured.3);
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
        let prepared = {
            let mut shell = state.write();
            checklist::save::now_usec()
                .and_then(|now| {
                    checklist::with_edit_session(&mut shell, &attempt, |session| {
                        captured.3.prepare(
                            &fresh,
                            session,
                            checklist::text_ops::draft(&attempt).expect("text attempt"),
                            now,
                        )
                    })
                })
                .transpose()
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
        if !checklist::text_ops::mark_dispatched(&mut state.write(), &attempt, &prepared) {
            reject(
                state,
                draft,
                generation,
                &attempt,
                "This canvas view changed before the edit was sent.",
            );
            return;
        }
        let result = api::execute_canvas_text_edit(
            &ctx.transport,
            &ctx.client,
            &ctx.session,
            &fresh,
            prepared,
        )
        .await;
        let accepted = matches!(&result, Ok(_));
        {
            let mut shell = state.write();
            match result {
                Ok(accepted) => {
                    checklist::text_ops::finish_text(&mut shell, &attempt, accepted);
                }
                Err(CanvasEditFailure::NotSent(_)) => {
                    checklist::finish(
                        &mut shell,
                        &attempt,
                        checklist::Outcome::Rejected(
                            "The paragraph edit was not sent. Refresh before trying again.",
                        ),
                    );
                }
                Err(CanvasEditFailure::Uncertain(_)) => {
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
                        "The paragraph edit was not sent. Cancel and refresh before trying again.",
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
            checklist::reader::load(state, reader, read).await;
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
