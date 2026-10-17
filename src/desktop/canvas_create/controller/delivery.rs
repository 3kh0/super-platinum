use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::canvas_create) enum Action {
    Close,
    Create,
    Discard,
    Review,
}

pub(in crate::canvas_create) fn capture(
    mut state: Signal<ShellState>,
    expected: &str,
    action: Action,
) {
    if crate::fixture::is_fixture() && !fixture::requested() {
        let mut shell = state.write();
        let Some(d) = shell
            .canvas_create
            .as_mut()
            .filter(|d| d.fixture && d.visit == expected)
        else {
            return;
        };
        match action {
            Action::Create => d.error = Some("Reconnect to create this canvas. This disconnected fixture does not send requests.".into()),
            Action::Discard => { shell.canvas_create = None; shell.overlay = None; },
            Action::Close => shell.overlay = None,
            Action::Review => {},
        }
        return;
    }
    let (editor, visit, mount, token, root) = {
        let mut shell = state.write();
        let root = shell.canvas_creation.clone();
        let Some(d) = shell
            .canvas_create
            .as_mut()
            .filter(|d| d.visit == expected && !d.busy && !d.fixture)
        else {
            return;
        };
        if !matches!(action, Action::Close | Action::Review) && !d.editable() {
            return;
        }
        let (Some(editor), Some(mount)) = (d.editor.clone(), d.native_visit.clone()) else {
            return;
        };
        let token = match editor.reserve_capture() {
            Ok(id) => id,
            Err(_) => {
                d.close_requested = false;
                d.pending_departure = None;
                d.error =
                    Some("A previous capture is still retained. Wait for its local result.".into());
                return;
            }
        };
        d.busy = true;
        d.close_requested = false;
        (editor, d.visit.clone(), mount, token, root)
    };
    adapter::retain(async move {
        let content = match native::capture(&mount, &token).await {
            Ok(content) => content,
            Err(message) => {
                editor.release_capture(&token);
                native::release(&mount, &token);
                update_error(state, &visit, message);
                return;
            }
        };
        // Mandatory custody transfer happens before any UI/owner/visit check.
        let serial = match editor.retain_capture(&token, content) {
            Ok(serial) => serial,
            Err(_) => {
                update_error(
                    state,
                    &visit,
                    "The capture remains retained; local recovery is required.",
                );
                return;
            }
        };
        if let Some(d) = state
            .write()
            .canvas_create
            .as_mut()
            .filter(|d| d.visit == visit)
        {
            let captured = editor.snapshot().content;
            d.title = captured.title().into();
            d.markdown = captured.markdown().into();
        }
        let base = match editor.saved(serial).await {
            Ok(base) => base,
            Err(_) => {
                // A queued destination is not authorized by an uncertain save or
                // an alternate conflict. Keep the editor for explicit recovery.
                if matches!(action, Action::Close)
                    && super::super::departure::pending(&state.peek(), &visit)
                {
                    editor.release_capture(&token);
                    native::release(&mount, &token);
                    update_error(
                        state,
                        &visit,
                        "Your text is retained. Save or review it before navigating.",
                    );
                    return;
                }
                if matches!(action, Action::Close | Action::Review)
                    && editor.retain_for_review().await.is_ok()
                {
                    editor.snapshot().base
                } else {
                    editor.release_capture(&token);
                    native::release(&mount, &token);
                    update_error(
                        state,
                        &visit,
                        "Your text is retained. Retry its local save before leaving.",
                    );
                    return;
                }
            }
        };
        let current = {
            let shell = state.peek();
            owner(&shell).as_ref() == Some(base.owner())
                && shell.overlay == Some(Overlay::CanvasCreate)
                && shell
                    .canvas_create
                    .as_ref()
                    .is_some_and(|d| d.visit == visit && d.native_visit.as_deref() == Some(&mount))
        };
        if !current {
            editor.release_capture(&token);
            root.release(&editor);
            return;
        }
        match action {
            Action::Review => {
                super::review::load(state, &visit, &editor, &mount, &token).await;
            }
            Action::Close => {
                editor.release_capture(&token);
                let next = match take_saved_departure(state, &visit, &editor, &base) {
                    Ok(next) => next,
                    Err(()) => {
                        native::release(&mount, &token);
                        return;
                    }
                };
                if next.is_some() && !root.release(&editor) {
                    native::release(&mount, &token);
                    update_error(
                        state,
                        &visit,
                        "Your latest input is still retained. Finish saving before navigating.",
                    );
                    return;
                }
                close_exact(state, &visit);
                root.release(&editor);
                if let Some(next) = next {
                    next.run(state);
                }
            }
            Action::Discard => {
                let result = editor.storage.run(move |store| store.discard(&base)).await;
                editor.release_capture(&token);
                if result.is_ok() {
                    close_exact(state, &visit);
                    root.release(&editor);
                } else {
                    native::release(&mount, &token);
                    update_error(
                        state,
                        &visit,
                        "The draft changed or could not be discarded. It remains retained.",
                    );
                }
            }
            Action::Create => {
                let Some(source) =
                    fixture::DeliverySource::capture(state).filter(|s| s.owner() == base.owner())
                else {
                    editor.release_capture(&token);
                    native::release(&mount, &token);
                    update_error(
                        state,
                        &visit,
                        "Reconnect to create this canvas. Its draft is saved locally.",
                    );
                    return;
                };
                let ticket = match root
                    .delivery
                    .reserve_saved(editor.storage.clone(), base.clone())
                {
                    Ok(t) => t,
                    Err(_) => {
                        editor.release_capture(&token);
                        native::release(&mount, &token);
                        update_error(
                            state,
                            &visit,
                            "Local recovery is full. No request was sent.",
                        );
                        return;
                    }
                };
                let visible = {
                    let shell = state.peek();
                    shell.overlay == Some(Overlay::CanvasCreate)
                        && shell.canvas_create.as_ref().is_some_and(|d| {
                            d.visit == visit
                                && d.native_visit.as_deref() == Some(&mount)
                                && !d.close_requested
                        })
                };
                if !visible {
                    root.delivery.acknowledge(&ticket);
                    editor.release_capture(&token);
                    native::release(&mount, &token);
                    update_error(
                        state,
                        &visit,
                        "The view changed. The draft is saved; Create was not started.",
                    );
                    return;
                }
                // After this call, outcome custody is independent of UI lifetime.
                if ticket.create(source).is_err() {
                    root.delivery.acknowledge(&ticket);
                    editor.release_capture(&token);
                    native::release(&mount, &token);
                    update_error(
                        state,
                        &visit,
                        "This account changed. No new creation was started.",
                    );
                    return;
                }
                editor.stop_editing();
                editor.release_capture(&token);
                // A clean old editor is no longer editable against its pre-reserve
                // snapshot. Reopening loads the actual durable delivery receipt.
                root.release(&editor);
                watch_delivery(state, visit, mount, token, editor, ticket, root).await;
                // No auto-navigation grant survives dismissal or another visit.
            }
        }
    });
}
async fn watch_delivery(
    mut state: Signal<ShellState>,
    visit: String,
    mount: String,
    token: String,
    editor: Arc<Editor>,
    ticket: super::adapter::Ticket,
    root: super::Root,
) {
    let mut changes = ticket.changes();
    let attempt = loop {
        match ticket.phase() {
            Phase::Delivering | Phase::Saving => {
                if changes.changed().await.is_err() {
                    return;
                }
            }
            Phase::Saved(saved) => {
                // Cancellation before the retained worker's first admission
                // leaves exactly the already-saved revision, not a new attempt.
                let current = {
                    let shell = state.peek();
                    owner(&shell).as_ref() == Some(saved.owner())
                        && shell.overlay == Some(Overlay::CanvasCreate)
                        && shell
                            .canvas_create
                            .as_ref()
                            .is_some_and(|d| d.visit == visit)
                };
                if current {
                    if let Ok(next) = root.install(editor.storage.clone(), saved, true) {
                        if let Some(d) = state
                            .write()
                            .canvas_create
                            .as_mut()
                            .filter(|d| d.visit == visit)
                        {
                            d.editor = Some(next);
                            d.busy = false;
                            d.saved = true;
                            apply_attempt(d, None);
                        }
                        native::finish(&mount, &token, true);
                    }
                }
                root.delivery.acknowledge(&ticket);
                return;
            }
            Phase::Settled(a) | Phase::Canceled(a) => break a,
            Phase::Held(_) => {
                // Permit another read-only capture for Close/review while
                // retaining the ambiguous delivery in root custody.
                native::finish(&mount, &token, false);
                if let Some(d) = state
                    .write()
                    .canvas_create
                    .as_mut()
                    .filter(|d| d.visit == visit)
                {
                    d.uncertain = true;
                }
                update_error(
                    state,
                    &visit,
                    "The delivery result is retained in memory. Retry local recovery; do not send again.",
                );
                return;
            }
            _ => {
                update_error(
                    state,
                    &visit,
                    "This delivery needs explicit local review. It was not replayed.",
                );
                return;
            }
        }
    };
    let load_owner = attempt.owner().clone();
    let id = attempt.draft_id().to_owned();
    let loaded = editor
        .storage
        .run(move |store| store.draft(&load_owner, &id)?.ok_or(domain::Fault::Stale))
        .await;
    // Result is already durable; only its original still-visible visit may update.
    let current = {
        let shell = state.peek();
        owner(&shell).as_ref() == Some(attempt.owner())
            && shell.overlay == Some(Overlay::CanvasCreate)
            && shell
                .canvas_create
                .as_ref()
                .is_some_and(|d| d.visit == visit)
    };
    if current {
        if let Ok(base) = loaded {
            let editable = matches!(
                attempt.delivery(),
                domain::Delivery::CanceledBeforeAdmission | domain::Delivery::RateLimited { .. }
            );
            if let Ok(next) = root.install(editor.storage.clone(), base, editable) {
                if let Some(d) = state
                    .write()
                    .canvas_create
                    .as_mut()
                    .filter(|d| d.visit == visit)
                {
                    d.editor = Some(next);
                    d.busy = false;
                    d.saved = true;
                    apply_attempt(d, Some(attempt));
                }
                native::finish(&mount, &token, editable);
            }
        } else {
            native::finish(&mount, &token, false);
            update_error(
                state,
                &visit,
                "The delivery is saved, but its draft could not be reloaded. Close and reopen to review it.",
            );
        }
    }
    root.delivery.acknowledge(&ticket);
}
