//! Disconnected structural worker adapter for explicitly owned saved-pair families.
use super::*;
pub(in crate::canvas::text) fn enabled(shell: &ShellState) -> bool {
    (gap::requested() || child_delete::requested() || owner_first::requested())
        && super::enabled(shell)
}
pub(in crate::canvas::text) fn fresh(shell: &ShellState) -> Arc<CanvasCollabSnapshot> {
    let source = snapshot(shell);
    if phase() != "stale" {
        return source;
    }
    let mut fresh = (*source).clone();
    let row = fresh
        .sections
        .iter_mut()
        .find(|s| s.id() == "neighbor")
        .unwrap();
    *row = Section::from_raw(row.raw().replace_fields(2, &[n(2, 2000)]).unwrap()).unwrap();
    Arc::new(fresh)
}
pub(in crate::canvas::text) async fn dispatch(
    mut state: Signal<ShellState>,
    attempt: &checklist::Attempt,
    prepared: PreparedStructureEdit,
    checkpoint: super_platinum_core::canvas::edit::structure::recovery::Checkpoint,
) -> Result<(), &'static str> {
    if !enabled(&state.peek())
        || !checklist::structure_ops::preparation_current(&state.peek(), attempt)
    {
        return Err("This canvas changed before saving.");
    }
    let source = snapshot(&state.peek());
    let write = json!({"session":prepared.session_id(),"sequence":prepared.sequence(),"documentData":STANDARD.encode(prepared.document_data().raw()),"checkpoint":checkpoint,"httpFactoryExecuted":false});
    let ticket = checklist::recovery::reserve_structure_worker(state, attempt, checkpoint).await?;
    if !same_source(&state.peek(), &source)
        || !checklist::structure_ops::preparation_current(&state.peek(), attempt)
        || !checklist::structure_ops::mark_dispatched(&mut state.write(), attempt, &prepared)
    {
        let _ = checklist::recovery::complete(state, &ticket).await;
        return Err("This canvas changed before saving.");
    }
    let mut shell = state.write();
    let f = fixture(&mut shell);
    f.reservations += 1;
    f.writes.push(write);
    f.pending = Some((attempt.clone(), prepared));
    Ok(())
}
