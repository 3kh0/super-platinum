use super::*;

fn context(shell: &ShellState, serial: u64) -> Result<admission::Context, Refusal> {
    if visible(shell).is_some()
        || !recovery_visible(shell).is_some_and(|r| r.serial == serial && !r.loading)
    {
        return Err(Refusal::Changed);
    }
    let context = admission::Context::capture(shell).ok_or(Refusal::Owner)?;
    if !shell
        .file_share
        .context
        .as_ref()
        .is_some_and(|c| c.current(shell))
    {
        return Err(Refusal::Owner);
    }
    Ok(context)
}
pub(crate) fn review(
    shell: &mut ShellState,
    serial: u64,
    expected: &Draft,
) -> Result<String, Refusal> {
    let context = context(shell, serial)?;
    if expected.scope.team != context.key.team
        || expected.scope.user != context.key.user
        || !shell
            .file_share
            .recovery
            .drafts
            .iter()
            .any(|d| d == expected)
    {
        return Err(Refusal::Changed);
    }
    if let Some(receipt) = shell
        .file_share
        .recovery
        .receipts
        .iter()
        .find(|r| r.scope.file == expected.scope.file)
        .cloned()
    {
        return recovery::show_receipt(shell, receipt, "saved-file-shares-open".into(), None);
    }
    let ticket = shell
        .file_share
        .custody
        .lock()
        .map_err(|_| Refusal::Storage)?
        .draft_io
        .retained()?
        .into_iter()
        .find(|t| t.visit().key.0 == context.key && pending_claim(t) == Some(expected));
    let storage = match &ticket {
        Some(t) => t.storage(),
        None => Storage::capture(&context)?,
    };
    let visit = drafts::Visit::review(&context.key, expected)?;
    show(
        shell,
        expected.content.clone(),
        Some(expected.clone()),
        Some(storage),
        visit,
        ticket,
    )
}
pub(crate) fn review_local(
    shell: &mut ShellState,
    serial: u64,
    job: u64,
) -> Result<String, Refusal> {
    let context = context(shell, serial)?;
    let ticket = shell
        .file_share
        .recovery
        .local
        .iter()
        .find(|t| t.id() == job && t.visit().key.0 == context.key)
        .cloned()
        .ok_or(Refusal::Changed)?;
    let queue = shell
        .file_share
        .custody
        .lock()
        .map_err(|_| Refusal::Storage)?
        .draft_io
        .clone();
    if !queue.retained()?.iter().any(|t| t.same(&ticket)) || !unsaved(&ticket) {
        return Err(Refusal::Changed);
    }
    let value = captured(&ticket).ok_or(Refusal::Changed)?;
    let _ = ticket.outcome();
    let storage = ticket.storage();
    let visit = ticket.visit().clone();
    show(shell, value, None, Some(storage), visit, Some(ticket))
}
fn show(
    shell: &mut ShellState,
    value: Content,
    durable: Option<Draft>,
    storage: Option<Storage>,
    visit: Arc<drafts::Visit>,
    ticket: Option<Ticket>,
) -> Result<String, Refusal> {
    let id = next_id()?;
    let local = durable.is_none() && ticket.is_some();
    shell.file_share.dialog = Some(Dialog {
        id: id.clone(),
        file_id: visit.key.1.clone(),
        file_title: value.title,
        note: value.note,
        destination: destination(value.destination.as_ref()),
        phase: Phase::DraftReview,
        error: Some(if local { CONFLICT } else { REVIEW }),
        receipt: None,
        return_focus: "saved-file-shares-open".into(),
        editor_ready: true,
        closing: false,
        saved_draft_stale: true,
        slack_url: None,
        admission: None,
        draft_visit: Some(visit),
        durable,
        draft_storage: storage,
        draft_task: ticket,
        draft_local: local,
    });
    shell.file_share.recovery.open = false;
    shell.overlay = Some(Overlay::FileShare);
    Ok(id)
}
pub(crate) fn discard(mut signal: Signal<ShellState>, id: String) {
    let prepared = {
        let mut shell = signal.write();
        prepare_discard(&mut shell, &id)
    };
    let (focus, epoch, work) = match prepared {
        Ok(work) => work,
        Err(error) => {
            let mut shell = signal.write();
            if let Some(d) = shell.file_share.dialog.as_mut().filter(|d| d.id == id) {
                d.closing = false;
                d.error = Some(error.message());
            }
            return;
        }
    };
    dioxus::core::spawn_forever(async move {
        if let Some((queue, ticket, visit)) = work {
            let outcome = ticket.wait().await;
            let committed = matches!(outcome, Outcome::Discarded);
            let mut shell = signal.write();
            let current = visible(&shell).is_some_and(|d| same(d, &id, &visit));
            if current {
                let dialog = shell.file_share.dialog.as_mut().unwrap();
                if committed {
                    dialog.durable = None;
                    dialog.draft_task = None;
                } else {
                    dialog.closing = false;
                    dialog.error = Some(match outcome {
                        Outcome::Blocked(_) => {
                            "An existing share still uses this saved message. Review saved file shares first."
                        }
                        Outcome::Conflict(_) => {
                            "The saved message changed. Refresh saved file shares before discarding it."
                        }
                        _ => "The saved message could not be discarded. Try again later.",
                    });
                }
            }
            if committed {
                let _ = queue.acknowledge(&ticket);
            } else {
                return;
            }
            if !current {
                return;
            }
        }
        hide(signal, &id, focus, epoch).await;
    });
}
type DiscardWork = Option<(Queue, Ticket, Arc<drafts::Visit>)>;
pub(in crate::file_share) fn prepare_discard(
    shell: &mut ShellState,
    id: &str,
) -> Result<(String, u64, DiscardWork), Refusal> {
    let dialog = visible(shell)
        .filter(|d| d.id == id && d.phase == Phase::DraftReview && !d.closing)
        .ok_or(Refusal::Changed)?;
    let visit = dialog.draft_visit.clone().ok_or(Refusal::Changed)?;
    let focus = dialog.return_focus.clone();
    let epoch = shell.core.account_epoch;
    let queue = shell
        .file_share
        .custody
        .lock()
        .map_err(|_| Refusal::Storage)?
        .draft_io
        .clone();
    let local = dialog.draft_local;
    let expected = dialog.durable.clone();
    let storage = dialog.draft_storage.clone();
    let previous = dialog.draft_task.clone();
    let work = if local {
        if let Some(ticket) = dialog.draft_task.clone() {
            let _ = ticket.outcome();
            if !unsaved(&ticket) || !queue.acknowledge(&ticket)? {
                return Err(Refusal::Changed);
            }
        } else {
            // Legacy process-only input has its own exact original Visit/body.
            let mut held = shell
                .file_share
                .custody
                .lock()
                .map_err(|_| Refusal::Storage)?;
            if !held.drafts.get(&visit.key).is_some_and(|d| {
                Arc::ptr_eq(&d.visit, &visit)
                    && d.note == dialog.note
                    && d.destination == dialog.destination
            }) {
                return Err(Refusal::Changed);
            }
            held.drafts.remove(&visit.key);
        }
        None
    } else {
        let expected = expected.ok_or(Refusal::Changed)?;
        let storage = storage.ok_or(Refusal::Storage)?;
        // The readonly dialog already owns this full version and Storage. Free
        // only its observed load operation before reserving exact cleanup.
        if let Some(previous) = previous.filter(|t| pending_claim(t) == Some(&expected)) {
            let _ = previous.outcome();
            if !queue.acknowledge(&previous)? {
                return Err(Refusal::Changed);
            }
            shell.file_share.dialog.as_mut().unwrap().draft_task = None;
        }
        // Reconstruct only a readonly stored identity, never an admission Source.
        let review = drafts::Visit::review(&visit.key.0, &expected)?;
        let ticket = queue
            .reserve(storage, review.clone())?
            .submit(Command::Discard { expected })
            .map_err(|r| r.reason)?;
        Some((queue, ticket, review))
    };
    let dialog = shell.file_share.dialog.as_mut().unwrap();
    dialog.closing = true;
    if let Some((_, ticket, visit)) = &work {
        dialog.draft_task = Some(ticket.clone());
        dialog.draft_visit = Some(visit.clone());
    }
    Ok((focus, epoch, work))
}
