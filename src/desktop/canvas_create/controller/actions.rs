use super::*;

pub(in crate::canvas_create) fn retry(mut state: Signal<ShellState>, expected: &str) {
    let reload = {
        let shell = state.peek();
        let Some(d) = shell.canvas_create.as_ref().filter(|d| {
            d.visit == expected
                && d.current(&shell)
                && !d.loading
                && !d.busy
                && shell.overlay == Some(Overlay::CanvasCreate)
        }) else {
            return;
        };
        d.editor.is_none() && d.owner.is_some()
    };
    if reload {
        // The initial account read failed before an Editor or Ticket existed.
        // Explicitly reload that still-current owner; no blank fallback, save,
        // request preparation or dispatch is authorized by this retry.
        close_exact(state, expected);
        open(state);
        return;
    }
    {
        let mut shell = state.write();
        if let Some(d) = shell
            .canvas_create
            .as_mut()
            .filter(|d| d.visit == expected && d.native_failed)
        {
            d.native_failed = false;
            d.error = None;
            return;
        }
    }
    let (root, owner, editor) = {
        let shell = state.peek();
        let Some(d) = shell.canvas_create.as_ref().filter(|d| d.visit == expected) else {
            return;
        };
        (
            shell.canvas_creation.clone(),
            d.owner.clone(),
            d.editor.clone(),
        )
    };
    if let Some(editor) = &editor {
        let _ = editor.retry();
    }
    if let Some(owner) = owner {
        for ticket in root.delivery.tickets(&owner) {
            if ticket.retry_storage().is_ok() {
                let custody = root.delivery.clone();
                let root = root.clone();
                let editor = editor.clone();
                let visit = expected.to_owned();
                let owner = owner.clone();
                let serial = editor.as_ref().map(|e| e.snapshot().serial);
                adapter::retain(async move {
                    let mut changes = ticket.changes();
                    loop {
                        match ticket.phase() {
                            Phase::Saving | Phase::Delivering => {
                                if changes.changed().await.is_err() {
                                    return;
                                }
                            }
                            Phase::Held(_) => return,
                            _ => {
                                // This action retries local storage only. Once
                                // its durable outcome is observed, reload the
                                // current owner inventory instead of reusing a
                                // pre-reservation editor revision.
                                custody.acknowledge(&ticket);
                                let current = {
                                    let shell = state.peek();
                                    super::owner(&shell).as_ref() == Some(&owner)
                                        && shell.overlay == Some(Overlay::CanvasCreate)
                                        && shell.canvas_create.as_ref().is_some_and(|d| {
                                            d.visit == visit
                                                && match (&d.editor, &editor) {
                                                    (Some(a), Some(b)) => {
                                                        Arc::ptr_eq(a, b)
                                                            && Some(b.snapshot().serial) == serial
                                                    }
                                                    _ => false,
                                                }
                                        })
                                };
                                if current {
                                    if let Some(editor) = &editor {
                                        let released = root.release(editor);
                                        if released || root.find(&owner).is_none() {
                                            close_exact(state, &visit);
                                            open(state);
                                        }
                                    }
                                }
                                return;
                            }
                        }
                    }
                });
            }
        }
    }
}
pub(in crate::canvas_create) fn cancel_reserved(mut state: Signal<ShellState>, expected: &str) {
    let (editor, attempt, visit, root) = {
        let mut shell = state.write();
        let root = shell.canvas_creation.clone();
        let Some(d) = shell
            .canvas_create
            .as_mut()
            .filter(|d| d.visit == expected && !d.busy)
        else {
            return;
        };
        let (Some(editor), Some(attempt)) = (d.editor.clone(), d.attempt.clone()) else {
            return;
        };
        if !matches!(attempt.delivery(), domain::Delivery::Reserved) {
            return;
        }
        d.busy = true;
        (editor, attempt, d.visit.clone(), root)
    };
    adapter::retain(async move {
        let current = attempt.clone();
        let result = editor
            .storage
            .run(move |store| {
                let reservation = store.reservation(current.owner(), current.id())?;
                if reservation.attempt() != &current {
                    return Err(domain::Fault::Stale);
                }
                store.cancel_reserved(&reservation)
            })
            .await;
        match result {
            Ok(canceled) => {
                let base = editor.snapshot().base;
                root.release(&editor);
                if let Ok(next) = root.install(editor.storage.clone(), base, true) {
                    let mut shell = state.write();
                    if owner(&shell).as_ref() != Some(canceled.owner()) {
                        return;
                    }
                    if let Some(d) = shell.canvas_create.as_mut().filter(|d| d.visit == visit) {
                        d.editor = Some(next);
                        d.busy = false;
                        apply_attempt(d, Some(canceled));
                        if let Some(mount) = &d.native_visit {
                            native::editable(mount, true);
                        }
                    }
                }
            }
            Err(_) => update_error(
                state,
                &visit,
                "The reservation changed or recovery failed. No request was sent by this action.",
            ),
        }
    });
}
pub(in crate::canvas_create) fn archive(mut state: Signal<ShellState>, expected: &str) {
    let (editor, attempt, visit, root) = {
        let mut shell = state.write();
        let root = shell.canvas_creation.clone();
        let Some(d) = shell
            .canvas_create
            .as_mut()
            .filter(|d| d.visit == expected && !d.busy)
        else {
            return;
        };
        let (Some(editor), Some(attempt)) = (d.editor.clone(), d.attempt.clone()) else {
            return;
        };
        d.busy = true;
        (editor, attempt, d.visit.clone(), root)
    };
    adapter::retain(async move {
        let reason = match attempt.delivery() {
            domain::Delivery::Created { .. } => domain::ArchiveReason::CreatedReviewed,
            domain::Delivery::Started | domain::Delivery::Uncertain => {
                domain::ArchiveReason::AbandonedOutcomeUnknown
            }
            _ => {
                update_error(
                    state,
                    &visit,
                    "Review this receipt before starting another canvas.",
                );
                return;
            }
        };
        let decision = match editor.decision(attempt, reason) {
            Ok(d) => d,
            Err(_) => {
                update_error(
                    state,
                    &visit,
                    "The draft is still saving. Wait before archiving it.",
                );
                return;
            }
        };
        let operation = decision.operation.clone();
        let result = editor
            .storage
            .run(move |store| {
                store.archive_and_new(
                    &decision.base,
                    &decision.attempt,
                    decision.reason,
                    &decision.operation,
                    &decision.writer,
                    adapter::now()?,
                )
            })
            .await;
        if result.is_ok() {
            editor.decision_saved(&operation);
            let current = {
                let shell = state.peek();
                owner(&shell).as_ref() == Some(editor.storage.owner())
                    && shell.overlay == Some(Overlay::CanvasCreate)
                    && shell
                        .canvas_create
                        .as_ref()
                        .is_some_and(|d| d.visit == visit)
            };
            close_exact(state, &visit);
            root.release(&editor);
            if current {
                open(state);
            }
        } else {
            update_error(
                state,
                &visit,
                "The retained receipt changed or could not be archived. No new request was sent.",
            );
        }
    });
}
pub(in crate::canvas_create) fn open_created(mut state: Signal<ShellState>, expected: &str) {
    let (target, editor, root) = {
        let shell = state.peek();
        let Some(d) = shell
            .canvas_create
            .as_ref()
            .filter(|d| d.visit == expected && d.current(&shell))
        else {
            return;
        };
        let Some(id) = d.created.clone() else {
            return;
        };
        (
            crate::state::CanvasTarget {
                id,
                title: d.title.clone(),
                permalink: None,
            },
            d.editor.clone(),
            shell.canvas_creation.clone(),
        )
    };
    close_exact(state, expected);
    if let Some(editor) = editor {
        root.release(&editor);
    }
    crate::canvas::open_target(state, target);
}
pub(in crate::canvas_create) fn check_files(mut state: Signal<ShellState>, expected: &str) {
    let (editor, root) = {
        let shell = state.peek();
        if !shell
            .canvas_create
            .as_ref()
            .is_some_and(|d| d.visit == expected && d.current(&shell) && d.uncertain)
        {
            return;
        }
        (
            shell.canvas_create.as_ref().and_then(|d| d.editor.clone()),
            shell.canvas_creation.clone(),
        )
    };
    close_exact(state, expected);
    if let Some(editor) = editor {
        root.release(&editor);
    }
    {
        let mut shell = state.write();
        if let Some(team) = shell.core.active_team.clone() {
            let epoch = shell.core.account_epoch;
            shell.core.files.ensure_scope(epoch, &team);
            shell.core.files.begin(
                super_platinum_core::files::FilesQuery {
                    kind: super_platinum_core::files::FileKind::Canvases,
                    owner: super_platinum_core::files::FileOwner::Yours,
                    sort: super_platinum_core::files::FileSort::Newest,
                    ..Default::default()
                },
                1,
            );
        }
    }
    adapter::retain(crate::bootstrap::load_main_view(
        state,
        crate::state::MainView::Files,
    ));
}
