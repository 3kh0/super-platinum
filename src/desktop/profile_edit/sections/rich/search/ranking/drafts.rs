//! Same h/H5 independent metadata branches as the installed Activity consumer.
//! Reads update only the shared Presence store; no composer contents or writes.
use super::*;
use std::{cell::RefCell, rc::Rc};
use super_platinum_core::drafts::presence::{self, Read, Step};

/// Initial ranking uses the current store immediately. Actual lazy changes may
/// refresh that same Input; they never create a replacement query or selection.
pub(in crate::profile_edit::sections::rich::search) fn schedule(
    mut state: Signal<ShellState>,
    search: Arc<Search>,
    on_changed: impl Fn(Signal<ShellState>, Result<(), Error>) + Clone + 'static,
) -> Result<(), Error> {
    let (read, purge) = {
        let mut shell = state.write();
        if !search.can_dispatch(&shell) {
            return Err(unavailable());
        }
        let ws = shell
            .core
            .workspaces
            .get_mut(search.lease.team())
            .ok_or_else(unavailable)?;
        let read = Read::begin(&mut ws.draft_presence);
        let purge = read.needs_purge();
        (Rc::new(RefCell::new(read)), purge)
    };
    for ids in [false, true].into_iter().filter(|ids| !ids || purge) {
        let (search, read, on_changed) = (search.clone(), read.clone(), on_changed.clone());
        spawn(async move {
            let result = pages(state, &search, read, ids, &on_changed).await;
            on_changed(state, result);
        });
    }
    Ok(())
}
async fn pages(
    mut state: Signal<ShellState>,
    search: &Search,
    read: Rc<RefCell<Read>>,
    ids: bool,
    on_changed: &impl Fn(Signal<ShellState>, Result<(), Error>),
) -> Result<(), Error> {
    let ctx = &search.context;
    let mut cursor = None;
    loop {
        {
            let shell = state.peek();
            if !search.can_dispatch(&shell)
                || !shell
                    .core
                    .workspaces
                    .get(search.lease.team())
                    .is_some_and(|w| read.borrow().current(&w.draft_presence))
            {
                return Err(unavailable());
            }
        }
        let request = if ids {
            presence::active_ids_request(&ctx.client, &ctx.session, cursor.as_deref())
        } else {
            presence::active_request(&ctx.client, &ctx.session, cursor.as_deref())
        };
        let raw = search.request(state, request).await?;
        let (step, changed) = {
            let mut shell = state.write();
            if !search.current(&shell) {
                return Err(unavailable());
            }
            let ws = shell
                .core
                .workspaces
                .get_mut(search.lease.team())
                .ok_or_else(unavailable)?;
            let before = ws.draft_presence.revision();
            let step = if ids {
                read.borrow_mut().active_ids(&mut ws.draft_presence, &raw)?
            } else {
                read.borrow_mut().active(&mut ws.draft_presence, &raw)?
            };
            (step, before != ws.draft_presence.revision())
        };
        if changed {
            crate::bootstrap::persist_workspace_metadata(&state, search.lease.team());
        }
        match step {
            Step::Done => return Ok(()),
            Step::More(next) => {
                if changed {
                    on_changed(state, Ok(()));
                }
                cursor = Some(next);
            }
        }
    }
}
