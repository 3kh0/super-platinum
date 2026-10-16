//! Explicit disposal of the current editor's earlier failed capture. No I/O.
use super::*;
use draft_io::Ticket;

#[derive(Clone)]
pub(crate) struct Preview {
    context: admission::Context,
    dialog_id: String,
    visit: Arc<drafts::Visit>,
    ticket: Ticket,
    pub note: Vec<Value>,
    pub destination: Option<String>,
}
impl Preview {
    pub(crate) fn id(&self) -> u64 {
        self.ticket.id()
    }
}

fn matches(shell: &ShellState, preview: &Preview) -> bool {
    preview.context.current(shell)
        && preview.visit.active()
        && preview.visit.writable()
        && visible(shell).is_some_and(|d| {
            d.id == preview.dialog_id
                && d.phase == Phase::Editing
                && !d.closing
                && d.draft_visit
                    .as_ref()
                    .is_some_and(|v| Arc::ptr_eq(v, &preview.visit))
                && d.draft_task
                    .as_ref()
                    .is_some_and(|t| t.same(&preview.ticket))
        })
}
pub(crate) fn current(shell: &ShellState, preview: &Preview) -> bool {
    if !matches(shell, preview) || !durable::unsaved(&preview.ticket) {
        return false;
    }
    shell
        .file_share
        .custody
        .lock()
        .ok()
        .and_then(|h| h.draft_io.retained().ok())
        .is_some_and(|items| items.iter().any(|t| t.same(&preview.ticket)))
}
pub(crate) fn capture(shell: &ShellState, id: &str) -> Result<Preview, Refusal> {
    let d = visible(shell)
        .filter(|d| d.id == id && d.phase == Phase::Editing && !d.closing)
        .ok_or(Refusal::Changed)?;
    let context = admission::Context::capture(shell).ok_or(Refusal::Owner)?;
    let visit = d.draft_visit.clone().ok_or(Refusal::Changed)?;
    let ticket = d
        .draft_task
        .clone()
        .filter(durable::unsaved)
        .ok_or(Refusal::Changed)?;
    if ticket.visit().key.0 != context.key || !Arc::ptr_eq(ticket.visit(), &visit) {
        return Err(Refusal::Changed);
    }
    let value = durable::captured(&ticket).ok_or(Refusal::Changed)?;
    let preview = Preview {
        context,
        dialog_id: id.into(),
        visit,
        ticket,
        note: value.note,
        destination: value.destination.map(|d| d.name),
    };
    if !current(shell, &preview) {
        return Err(Refusal::Changed);
    }
    // Observation alone does not remove custody; only explicit discard can ack.
    let _ = preview.ticket.outcome();
    Ok(preview)
}
pub(crate) fn discard(shell: &mut ShellState, preview: &Preview) -> Result<(), Refusal> {
    if !current(shell, preview) {
        return Err(Refusal::Changed);
    }
    let queue = shell
        .file_share
        .custody
        .lock()
        .map_err(|_| Refusal::Storage)?
        .draft_io
        .clone();
    let _ = preview.ticket.outcome();
    if !queue.acknowledge(&preview.ticket)? {
        return Err(Refusal::Changed);
    }
    let d = shell.file_share.dialog.as_mut().ok_or(Refusal::Changed)?;
    // Exact identity was checked in this serial turn. Never read, replace or
    // acknowledge the current DOM; its authoritative expected version stays.
    if d.draft_task
        .as_ref()
        .is_some_and(|t| t.same(&preview.ticket))
    {
        d.draft_task = None;
    }
    Ok(())
}
