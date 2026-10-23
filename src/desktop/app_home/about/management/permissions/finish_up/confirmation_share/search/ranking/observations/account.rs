//! Account complete reuse retains the original Share query/native admission.
use super::*;
use crate::app_home::autocomplete::observations as shared;
pub(super) use shared::Acquired;

pub(super) async fn capture(
    state: Signal<ShellState>,
    q: &Query,
    authority: &Authority,
    online: bool,
    guard: &impl Fn(&ShellState) -> bool,
) -> Result<Acquired, Error> {
    let current = |s: &ShellState| q.current(s) && guard(s);
    if !current(&state.peek()) || !q.authorized(state).await || !current(&state.peek()) {
        return Err(unavailable());
    }
    // Capture real connectivity once; its currentness fences subsequent link
    // transitions, and online is checked again before each producer read.
    let plan = super_platinum_core::workflow_share::search::Query::new(
        q.expected["query"].as_str().ok_or_else(unavailable)?,
    )?;
    let connection = crate::connection::autocomplete::Observation::capture(
        &state.peek(),
        &q.ctx().session.team_id,
        &plan,
    )
    .ok_or_else(unavailable)?;
    let current = |s: &ShellState| current(s) && connection.current(s);
    let connected = |s: &ShellState| online && connection.online() && connection.current(s);
    let read = |request| super::super::request(state, q, request, &current);
    let result = shared::acquire_with(
        state,
        q.original.owner.ctx.clone(),
        authority.clone(),
        &current,
        &connected,
        &read,
    )
    .await?;
    if !current(&state.peek())
        || !q.authorized(state).await
        || !current(&state.peek())
        || !result.store.current(&state.peek())
    {
        return Err(unavailable());
    }
    Ok(result)
}
