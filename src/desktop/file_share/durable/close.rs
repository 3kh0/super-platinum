use super::*;

/// Explicit transition from a failed editable save to a retained readonly copy.
/// Read the current DOM again because the user may have edited after failure.
pub(crate) fn review_current(mut signal: Signal<ShellState>, id: String) {
    let prepared = {
        let mut shell = signal.write();
        if !has_capture(&shell, &id) {
            return;
        }
        let destination = visible(&shell).unwrap().destination.clone();
        match capture::Capture::prepare(&mut shell, &id) {
            Ok(capture) => {
                let d = shell.file_share.dialog.as_mut().unwrap();
                d.closing = true;
                d.phase = Phase::Preparing;
                Some((capture, destination))
            }
            Err(_) => {
                shell.file_share.dialog.as_mut().unwrap().error = Some(SAVE_ERROR);
                None
            }
        }
    };
    let Some((capture, destination)) = prepared else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let visit = capture.visit.clone();
        let observation = capture.ticket.clone();
        let predecessor = capture.prior_task.clone();
        let note = delivery::read_note(&id).await;
        let ticket = note
            .ok_or(Refusal::Note)
            .and_then(|note| capture.retain(note, destination));
        let ticket = match ticket {
            Ok(ticket) => {
                ticket.wait().await;
                ticket
            }
            Err(_) => {
                let mut shell = signal.write();
                if let Some(d) = shell.file_share.dialog.as_mut().filter(|d| {
                    same(d, &id, &visit)
                        && d.draft_task.as_ref().is_some_and(|t| t.same(&observation))
                }) {
                    d.closing = false;
                    d.phase = Phase::Editing;
                    d.draft_task = predecessor;
                    d.error = Some(Refusal::Note.message());
                }
                return;
            }
        };
        let mut shell = signal.write();
        if !visible(&shell).is_some_and(|d| same(d, &id, &visit)) {
            return;
        }
        let d = shell.file_share.dialog.as_mut().unwrap();
        d.closing = false;
        d.phase = Phase::DraftReview;
        d.draft_local = true;
        d.saved_draft_stale = true;
        d.draft_task = Some(ticket);
        d.error =
            Some("This captured copy is not saved to disk. You can keep it for this session.");
    });
}

pub(crate) async fn dialog(mut signal: Signal<ShellState>, id: String) {
    let (focus, epoch, selected, capture, loading) = {
        let mut shell = signal.write();
        let Some(dialog) = visible(&shell).filter(|d| d.id == id && !d.closing) else {
            return;
        };
        let focus = dialog.return_focus.clone();
        let selected = dialog.destination.clone();
        let epoch = shell.core.account_epoch;
        let needs_capture =
            matches!(dialog.phase, Phase::Editing | Phase::Preparing) && dialog.receipt.is_none();
        let capture = if needs_capture {
            match capture::Capture::prepare(&mut shell, &id) {
                Ok(capture) => Some(capture),
                Err(_) => {
                    shell.file_share.dialog.as_mut().unwrap().error = Some(SAVE_ERROR);
                    return;
                }
            }
        } else {
            None
        };
        let loading = if shell.file_share.dialog.as_ref().unwrap().phase == Phase::Loading {
            let queue = match shell.file_share.custody.lock() {
                Ok(h) => h.draft_io.clone(),
                Err(_) => return,
            };
            shell
                .file_share
                .dialog
                .as_ref()
                .unwrap()
                .draft_task
                .clone()
                .map(|ticket| {
                    ticket.cancel_load();
                    (queue, ticket)
                })
        } else {
            None
        };
        let dialog = shell.file_share.dialog.as_mut().unwrap();
        dialog.closing = true;
        if capture.is_some() {
            dialog.phase = Phase::Preparing;
        }
        (focus, epoch, selected, capture, loading)
    };
    if let Some((queue, ticket)) = loading {
        ticket.wait().await;
        let claim = pending_claim(&ticket).cloned();
        let mut shell = signal.write();
        if !visible(&shell).is_some_and(|d| d.id == id && d.closing && d.phase == Phase::Loading) {
            drop(shell);
            cancel_load(queue, ticket);
            return;
        }
        if let Some(claim) = claim {
            let visit = match drafts::Visit::review(&ticket.visit().key.0, &claim) {
                Ok(v) => v,
                Err(_) => {
                    let d = shell.file_share.dialog.as_mut().unwrap();
                    d.closing = false;
                    d.error = Some(SAVE_ERROR);
                    return;
                }
            };
            let d = shell.file_share.dialog.as_mut().unwrap();
            d.note = claim.content.note.clone();
            d.destination = destination(claim.content.destination.as_ref());
            d.durable = Some(claim);
            d.draft_storage = Some(ticket.storage());
            d.draft_visit = Some(visit);
            d.draft_task = Some(ticket);
            d.phase = Phase::DraftReview;
            d.editor_ready = true;
            d.draft_local = false;
            d.saved_draft_stale = true;
            d.closing = false;
            drop(shell);
            // Exact claim is now in dialog custody. This synchronously releases
            // its old queue slot before reserving Discard; pressure keeps it open.
            saved::discard(signal, id);
            return;
        }
        shell.file_share.dialog.as_mut().unwrap().draft_task = None;
        drop(shell);
        let _ = queue.acknowledge(&ticket);
    }
    if let Some(capture) = capture {
        // Waiting here cannot let a newer capture overtake this reservation.
        // The earlier Share task will fail its !closing fence before Begin.
        let previous = capture.predecessor().await;
        let note = delivery::read_note(&id).await;
        let visit = capture.visit.clone();
        let queue = capture.queue.clone();
        let observation = capture.ticket.clone();
        let Some(note) = note else {
            let predecessor = capture.prior_task.clone();
            drop(capture);
            reader_failed(
                &mut signal.write(),
                &id,
                &visit,
                &observation,
                previous,
                predecessor,
            );
            return;
        };
        // Destination was frozen by the Close lock; use captured dialog input,
        // never a replacement dialog's destination after an await.
        let ticket = match capture.persist(note, selected, true).await {
            Ok(ticket) => ticket,
            Err(_) => {
                let mut shell = signal.write();
                if let Some(d) = shell
                    .file_share
                    .dialog
                    .as_mut()
                    .filter(|d| same(d, &id, &visit))
                {
                    d.phase = Phase::Editing;
                    d.closing = false;
                    d.draft_task = None;
                    d.error = Some(Refusal::Note.message());
                }
                return;
            }
        };
        let outcome = ticket.wait().await;
        let committed = matches!(outcome, Outcome::Saved(_) | Outcome::Discarded);
        let shown = {
            let mut shell = signal.write();
            if !visible(&shell).is_some_and(|d| same(d, &id, &visit)) {
                false
            } else {
                let applied = apply_capture(&mut shell, &id, &visit, &ticket, &outcome);
                if !committed && applied {
                    shell.file_share.dialog.as_mut().unwrap().closing = false;
                }
                applied
            }
        };
        if committed {
            let _ = queue.acknowledge(&ticket);
        } else {
            return;
        }
        if !shown {
            return;
        }
    }
    hide(signal, &id, focus, epoch).await;
}

/// No current input was read. Never replace that still-mounted editor with an
/// older captured ticket's readonly body, even when its predecessor conflicted.
pub(in crate::file_share) fn reader_failed(
    shell: &mut ShellState,
    id: &str,
    visit: &Arc<drafts::Visit>,
    observation: &Ticket,
    previous: Result<Draft, Refusal>,
    prior: Option<Ticket>,
) {
    if !visible(shell).is_some_and(|d| {
        same(d, id, visit) && d.draft_task.as_ref().is_some_and(|t| t.same(observation))
    }) {
        return;
    }
    let d = shell.file_share.dialog.as_mut().unwrap();
    if let Ok(version) = previous {
        d.durable = Some(version);
    }
    d.closing = false;
    d.phase = Phase::Editing;
    d.draft_local = false;
    d.saved_draft_stale = false;
    d.draft_task = prior;
    d.error = Some(Refusal::Note.message());
}
