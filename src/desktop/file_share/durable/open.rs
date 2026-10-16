use super::*;

pub(in crate::file_share) struct Load {
    pub id: String,
    source: admission::Source,
    queue: Queue,
    ticket: Option<Ticket>,
    error: Option<Refusal>,
}
pub(crate) fn start(
    mut signal: Signal<ShellState>,
    target: crate::file_actions::Target,
    focus: String,
) -> Result<String, Refusal> {
    let load = {
        let mut shell = signal.write();
        if visible(&shell).is_some_and(uncaptured) {
            return Err(Refusal::Busy);
        }
        let source = admission::Source::capture(&shell, &target).ok_or(Refusal::Source)?;
        prepare(&mut shell, source, focus)?
    };
    let id = load.id.clone();
    dioxus::core::spawn_forever(load.run(signal));
    Ok(id)
}
pub(crate) fn retry(mut signal: Signal<ShellState>, id: String) {
    let load = {
        let mut shell = signal.write();
        let Some(dialog) =
            visible(&shell).filter(|d| d.id == id && d.phase == Phase::LoadFailed && !d.closing)
        else {
            return;
        };
        let Some(source) = dialog.admission.clone().filter(|s| s.current(&shell)) else {
            return;
        };
        let focus = dialog.return_focus.clone();
        prepare(&mut shell, source, focus)
    };
    match load {
        Ok(load) => {
            dioxus::core::spawn_forever(load.run(signal));
        }
        Err(error) => signal.write().show_toast(error.message()),
    }
}
pub(super) fn prepare(
    shell: &mut ShellState,
    source: admission::Source,
    focus: String,
) -> Result<Load, Refusal> {
    let captured = Storage::capture(&source.context);
    prepare_captured(shell, source, focus, captured)
}
pub(in crate::file_share) fn prepare_captured(
    shell: &mut ShellState,
    source: admission::Source,
    focus: String,
    captured: Result<Storage, Refusal>,
) -> Result<Load, Refusal> {
    if visible(shell).is_some_and(uncaptured) {
        return Err(Refusal::Busy);
    }
    let id = next_id()?;
    let mut state = std::mem::take(&mut shell.file_share);
    state.sync(shell);
    let prepared = (|| {
        let (visit, queue) = {
            let mut held = state.custody.lock().map_err(|_| Refusal::Storage)?;
            (held.visit(&source)?, held.draft_io.clone())
        };
        let storage = captured.as_ref().ok().cloned();
        let initial = Content {
            source: source.file.clone(),
            title: source.title.clone(),
            note: vec![],
            destination: None,
        };
        let submitted = captured
            .and_then(|s| queue.reserve(s, visit.clone()))
            .and_then(|r| {
                r.submit(Command::LoadClaim { initial })
                    .map_err(|r| r.reason)
            });
        let (ticket, error) = match submitted {
            Ok(t) => (Some(t), None),
            Err(e) => (None, Some(e)),
        };
        state.recovery.open = false;
        state.dialog = Some(Dialog {
            id: id.clone(),
            file_id: source.id.clone(),
            file_title: source.title.clone(),
            note: vec![],
            destination: None,
            phase: Phase::Loading,
            error: None,
            receipt: None,
            return_focus: focus,
            editor_ready: false,
            closing: false,
            saved_draft_stale: false,
            slack_url: source.link.clone(),
            admission: Some(source.clone()),
            draft_visit: Some(visit),
            durable: None,
            draft_storage: storage,
            draft_task: ticket.clone(),
            draft_local: false,
        });
        Ok(Load {
            id,
            source,
            queue,
            ticket,
            error,
        })
    })();
    shell.file_share = state;
    if prepared.is_ok() {
        shell.overlay = Some(Overlay::FileShare);
    }
    prepared
}
impl Load {
    pub(in crate::file_share) async fn outcome(&self) -> Outcome {
        match &self.ticket {
            Some(ticket) => ticket.wait().await,
            None => Outcome::Failed(self.error.unwrap_or(Refusal::Storage)),
        }
    }
    fn current(&self, shell: &ShellState, outcome: &Outcome) -> bool {
        self.source.context.current(shell)
            && (matches!(outcome, Outcome::Blocked(_)) || self.source.current(shell))
            && visible(shell)
                .is_some_and(|d| d.id == self.id && d.phase == Phase::Loading && !d.closing)
    }
    async fn run(self, mut signal: Signal<ShellState>) {
        let outcome = match &self.ticket {
            Some(ticket) => ticket.wait().await,
            None => Outcome::Failed(self.error.unwrap_or(Refusal::Storage)),
        };
        let mut shell = signal.write();
        if !self.current(&shell, &outcome) {
            if self.source.context.current(&shell) {
                if let Some(dialog) = shell
                    .file_share
                    .dialog
                    .as_mut()
                    .filter(|d| d.id == self.id && d.phase == Phase::Loading && !d.closing)
                {
                    dialog.phase = Phase::LoadFailed;
                    dialog.error = Some(
                        "This file changed while opening. Close this window and reopen Share from the current file.",
                    );
                    dialog.draft_task = None;
                }
            }
            if let Some(ticket) = self.ticket {
                cancel_load(self.queue, ticket);
            }
            return;
        }
        self.publish(&mut shell, outcome);
    }
    pub(in crate::file_share) fn publish(&self, shell: &mut ShellState, outcome: Outcome) {
        if !self.current(shell, &outcome) {
            return;
        }
        if let Outcome::Blocked(receipt) = outcome {
            let focus = shell
                .file_share
                .dialog
                .as_ref()
                .unwrap()
                .return_focus
                .clone();
            let _ = recovery::show_receipt(shell, receipt, focus, self.source.link.clone());
            if let Some(ticket) = &self.ticket {
                let _ = self.queue.acknowledge(ticket);
            }
            return;
        }
        // Failed earlier captures are independent root-owned records. A new
        // claim does not authorize overwriting them or hiding their input.
        let local = self.queue.retained().ok().and_then(|items| {
            items.into_iter().rev().find(|t| {
                t.visit().key == (self.source.context.key.clone(), self.source.id.clone())
                    && unsaved(t)
            })
        });
        if let Some(ticket) = local {
            if let Some(value) = captured(&ticket) {
                show_local(shell, &self.id, value, Some(ticket));
            }
            if let Some(ticket) = self.ticket.clone() {
                cancel_load(self.queue.clone(), ticket);
            }
            return;
        }
        let legacy = {
            let held = shell.file_share.custody.lock().ok();
            held.as_ref()
                .and_then(|h| {
                    h.drafts
                        .get(&(self.source.context.key.clone(), self.source.id.clone()))
                })
                .map(|d| (d.note.clone(), d.destination.clone(), d.visit.clone()))
        };
        if let Some((note, destination, visit)) =
            legacy.filter(|(note, destination, _)| !note.is_empty() || destination.is_some())
        {
            let value = Content {
                source: visit.source.clone(),
                title: self.source.title.clone(),
                note,
                destination: destination.map(|d| {
                    super_platinum_core::file_sharing::store::drafts::Destination {
                        id: d.id,
                        name: d.name,
                        private: d.private,
                    }
                }),
            };
            show_local(shell, &self.id, value, None);
            shell.file_share.dialog.as_mut().unwrap().draft_visit = Some(visit);
            if let Some(ticket) = self.ticket.clone() {
                cancel_load(self.queue.clone(), ticket);
            }
            return;
        }
        let dialog = shell.file_share.dialog.as_mut().unwrap();
        match outcome {
            Outcome::Claimed { draft, .. } => {
                dialog.note = draft.content.note.clone();
                dialog.destination = destination(draft.content.destination.as_ref());
                dialog.durable = Some(draft);
                dialog.phase = Phase::Editing;
                dialog.editor_ready = true;
                dialog.error = None;
                dialog.draft_task = None;
            }
            Outcome::Review(draft) => {
                dialog.note = draft.content.note.clone();
                dialog.destination = destination(draft.content.destination.as_ref());
                dialog.draft_visit = drafts::Visit::review(&self.source.context.key, &draft).ok();
                dialog.durable = Some(draft);
                dialog.phase = Phase::DraftReview;
                dialog.editor_ready = true;
                dialog.saved_draft_stale = true;
                dialog.error = Some(REVIEW);
                dialog.draft_task = None;
            }
            Outcome::Conflict(_) => {
                dialog.phase = Phase::LoadFailed;
                dialog.error = Some(
                    "The saved message changed while opening. Try again to load its current version.",
                );
                dialog.draft_task = None;
            }
            _ => {
                dialog.phase = Phase::LoadFailed;
                dialog.error = Some(LOAD_ERROR);
                dialog.draft_task = None;
            }
        }
        if let Some(ticket) = &self.ticket {
            let _ = self.queue.acknowledge(ticket);
        }
    }
}
fn show_local(shell: &mut ShellState, id: &str, value: Content, ticket: Option<Ticket>) {
    let Some(dialog) = shell.file_share.dialog.as_mut().filter(|d| d.id == id) else {
        return;
    };
    dialog.note = value.note;
    dialog.destination = destination(value.destination.as_ref());
    dialog.durable = None;
    if let Some(ticket) = &ticket {
        dialog.draft_visit = Some(ticket.visit().clone());
    }
    dialog.draft_task = ticket;
    dialog.draft_local = true;
    dialog.phase = Phase::DraftReview;
    dialog.editor_ready = true;
    dialog.saved_draft_stale = true;
    dialog.error =
        Some("This captured message has not been saved. Review it before discarding it.");
}
