//! Activity-owned reads into the one workspace unified-draft metadata store.
//! Shared a9af/3b42 source chain must be composed explicitly. No composer
//! contents or send authority are created by these ranking observations.
use super::*;
use std::{cell::RefCell, rc::Rc};
use super_platinum_core::drafts::presence::{self, Read, Step};

pub(super) struct Receipt {
    generation: u64,
    revision: u64,
}
impl Receipt {
    pub(super) fn current(&self, s: &ShellState, q: &Query) -> bool {
        self.generation == q.generation
            && q.current(s)
            && s.core
                .workspaces
                .get(&q.owner.ctx.session.team_id)
                .is_some_and(|w| w.draft_presence.revision() == self.revision)
    }
}

pub(super) fn snapshot(s: &ShellState, q: &Query) -> Result<Receipt, Error> {
    if !q.current(s) {
        return Err(unavailable());
    }
    let w = s
        .core
        .workspaces
        .get(&q.owner.ctx.session.team_id)
        .ok_or_else(unavailable)?;
    Ok(Receipt {
        generation: q.generation,
        revision: w.draft_presence.revision(),
    })
}

/// Actual h/H5 branches publish independently. Initial ranking consumes the
/// current metadata immediately and never awaits a held sibling.
pub(super) fn schedule(mut state: Signal<ShellState>, q: Arc<Query>) -> Result<(), Error> {
    let (read, purge) = {
        let mut s = state.write();
        if !q.current(&s) || (q.fixture && !fixture::enabled(&s)) {
            return Err(unavailable());
        }
        let w = s
            .core
            .workspaces
            .get_mut(&q.owner.ctx.session.team_id)
            .ok_or_else(unavailable)?;
        let read = Read::begin(&mut w.draft_presence);
        let purge = read.needs_purge();
        (Rc::new(RefCell::new(read)), purge)
    };
    for ids in [false, true].into_iter().filter(|ids| !ids || purge) {
        let q = q.clone();
        let read = read.clone();
        spawn(async move {
            let result = pages(state, &q, read, ids).await;
            if let Err(error) = result {
                // Keep each original failure even if presentation retired.
                q.draft_failures.lock().unwrap().push(error);
                let mut s = state.write();
                if q.current(&s) {
                    if let Some(d) = document_mut(&mut s, q.visit) {
                        d.activity.member_notice =
                            Some("Some ranking information could not be refreshed.".into());
                    }
                    lookup::refresh_drafts(&mut s, &q);
                }
            }
        });
    }
    Ok(())
}

async fn pages(
    mut state: Signal<ShellState>,
    q: &Query,
    read: Rc<RefCell<Read>>,
    ids: bool,
) -> Result<(), Error> {
    let ctx = &q.owner.ctx;
    let mut cursor = None;
    loop {
        {
            let s = state.peek();
            if !q.current(&s)
                || !s
                    .core
                    .workspaces
                    .get(&ctx.session.team_id)
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
        let raw = read::request(state, q, request).await?;
        let (step, changed) = {
            let mut s = state.write();
            if !q.current(&s) {
                return Err(unavailable());
            }
            let w = s
                .core
                .workspaces
                .get_mut(&ctx.session.team_id)
                .ok_or_else(unavailable)?;
            let before = w.draft_presence.revision();
            let step = if ids {
                read.borrow_mut().active_ids(&mut w.draft_presence, &raw)?
            } else {
                read.borrow_mut().active(&mut w.draft_presence, &raw)?
            };
            (step, before != w.draft_presence.revision())
        };
        if changed {
            crate::bootstrap::persist_workspace_metadata(&state, &ctx.session.team_id);
            lookup::refresh_drafts(&mut state.write(), q);
        }
        match step {
            Step::Done => return Ok(()),
            Step::More(next) => cursor = Some(next),
        }
    }
}
