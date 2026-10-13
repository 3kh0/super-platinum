//! One explicit toggle, fresh canonical preflight, and a retained send receipt.
use super::*;
use dioxus::prelude::*;
use super_platinum_core::{
    canvas::edit::CheckboxIntent,
    slack::{api, transport::CanvasEditFailure},
};

pub(in crate::canvas) fn now_usec() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_micros()).ok())
}

pub(in crate::canvas) fn toggle(
    mut state: Signal<ShellState>,
    reader: Signal<super::super::Reader>,
    section: String,
    visit: u64,
    read: u64,
) {
    let intent = {
        let displayed = reader.peek();
        if displayed.checklist.owner != Some((visit, read)) {
            return;
        }
        let Some(item) = displayed.checklist.items.get(&section) else {
            return;
        };
        let Some(intent) = item.intent.clone() else {
            return;
        };
        (intent, !item.checked)
    };
    let attempt = match begin(&mut state.write(), visit, read, &section, intent.1) {
        Ok(attempt) => attempt,
        Err(_) => return,
    };
    if super::fixture::enabled(&state.peek()) {
        super::fixture::dispatch(&mut state.write(), &attempt, intent.0);
        return;
    }
    // A navigation may cancel a component task after HTTP dispatch. The shell
    // owns this task so its exact operation receipt still settles after close.
    dioxus::core::spawn_forever(async move {
        let Some(ctx) = attempt.request(&state.peek()) else {
            cancel_preflight(&mut state.write(), &attempt);
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
            Ok(fresh) => fresh,
            Err(_) => {
                reject_preflight(
                    &mut state.write(),
                    &attempt,
                    "Could not check this canvas before saving. Refresh and try again.",
                );
                return;
            }
        };
        let prepared_result =
            { prepare(&mut state.write(), &attempt, &intent.0, &fresh, now_usec()) };
        let prepared = match prepared_result {
            Ok(prepared) => prepared,
            Err(reason) => {
                reject_preflight(&mut state.write(), &attempt, reason);
                return;
            }
        };
        // No await between the final immutable scope guard and this exact-path,
        // no-hold, one-attempt API. It alone classifies NotSent versus Uncertain.
        if !mark_dispatched(&mut state.write(), &attempt, &prepared) {
            cancel_preflight(&mut state.write(), &attempt);
            return;
        }
        let result = api::execute_canvas_checkbox_edit(
            &ctx.transport,
            &ctx.client,
            &ctx.session,
            &fresh,
            prepared,
        )
        .await;
        let outcome = match result {
            Ok(accepted) => Outcome::Accepted(accepted),
            Err(CanvasEditFailure::NotSent(_)) => {
                Outcome::Rejected("The checklist change was not sent. Refresh before trying again.")
            }
            Err(CanvasEditFailure::Uncertain(_)) => Outcome::Uncertain,
        };
        // Settlement is serial-owned even if the old account/visit is gone.
        // Local Reader signals, however, may only be touched in their own visit.
        let read = {
            let mut shell = state.write();
            finish(&mut shell, &attempt, outcome);
            followup_read(&mut shell, &attempt)
        };
        if let Some(read) = read {
            super::reader::load(state, reader, read).await;
        }
    });
}

fn followup_read(shell: &mut ShellState, attempt: &Attempt) -> Option<Read> {
    if presentation(shell).is_some_and(|view| view.visit == attempt.visit)
        && pending_attempt(shell).is_some_and(|pending| pending.serial == attempt.serial)
    {
        begin_read(shell)
    } else {
        None
    }
}

fn prepare(
    shell: &mut ShellState,
    attempt: &Attempt,
    intent: &CheckboxIntent,
    fresh: &CanvasCollabSnapshot,
    now: Option<u64>,
) -> Result<PreparedCheckboxEdit, &'static str> {
    let now = now.ok_or("Check your system clock before editing this canvas.")?;
    with_edit_session(shell, attempt, |session| {
        intent.prepare(fresh, session, now)
    })
    .ok_or("This canvas view changed. Refresh before editing.")?
    .map_err(|_| "This checklist item or your access changed. Refresh before trying again.")
}
fn reject_preflight(shell: &mut ShellState, attempt: &Attempt, reason: &'static str) {
    let visible = dispatchable(shell, attempt);
    cancel_preflight(shell, attempt);
    if visible {
        if let Some(view) = shell.canvas_checklist.view.as_mut() {
            view.stale = true;
            view.error = Some(reason);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::checklist::tests::fixtures;
    #[test]
    fn stale_preflight_cannot_prepare_or_paint_another_visit() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.canvas = Some(crate::state::CanvasTarget {
            id: "FCANVAS".into(),
            title: "Test".into(),
            permalink: None,
        });
        shell.overlay = Some(Overlay::Canvas);
        open_visit(&mut shell);
        let read = begin_read(&mut shell).unwrap();
        let snapshot = fixtures::snapshot(false, 1000);
        finish_read(&mut shell, &read, Ok(snapshot.clone()));
        let intent = CheckboxIntent::capture(&snapshot, "item", true, 10).unwrap();
        let view = presentation(&shell).unwrap();
        let attempt = begin(&mut shell, view.visit, view.read, "item", true).unwrap();
        shell.shortcut_generation += 1;
        open_visit(&mut shell);
        assert!(prepare(&mut shell, &attempt, &intent, &snapshot, Some(10)).is_err());
        reject_preflight(&mut shell, &attempt, "Old failure");
        assert!(presentation(&shell).unwrap().error.is_none());
        assert!(presentation(&shell).unwrap().operation.is_none());
    }
    #[test]
    fn accepted_receipt_after_reopen_waits_for_the_new_visits_own_read() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.canvas = Some(crate::state::CanvasTarget {
            id: "FCANVAS".into(),
            title: "Test".into(),
            permalink: None,
        });
        shell.overlay = Some(Overlay::Canvas);
        open_visit(&mut shell);
        let read = begin_read(&mut shell).unwrap();
        let snapshot = fixtures::snapshot(false, 1000);
        finish_read(&mut shell, &read, Ok(snapshot.clone()));
        let view = presentation(&shell).unwrap();
        let attempt = begin(&mut shell, view.visit, view.read, "item", true).unwrap();
        let prepared = fixtures::prepare(&mut shell, &attempt);
        assert!(mark_dispatched(&mut shell, &attempt, &prepared));
        shell.dismiss_overlay();
        shell.shortcut_generation += 1;
        shell.overlay = Some(Overlay::Canvas);
        open_visit(&mut shell);
        assert!(finish(
            &mut shell,
            &attempt,
            Outcome::Accepted(fixtures::accepted(&prepared))
        ));
        assert!(
            followup_read(&mut shell, &attempt).is_none(),
            "old task must not target a replacement Reader"
        );
        let read = begin_read(&mut shell).unwrap();
        assert!(finish_read(
            &mut shell,
            &read,
            Ok(fixtures::snapshot(true, 2000))
        ));
        assert!(confirm_readback(&mut shell, &attempt, &read));
        assert!(presentation(&shell).unwrap().operation.is_none());
    }
}
