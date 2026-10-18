//! Original-owner FIFO intake for effective group metadata. No network work.
//! Legacy raw label refresh remains the producer's separate fallback path.
use crate::{directory_member_fetches::Context, state::ShellState};
use dioxus::prelude::*;
pub(crate) use source_core::{NativeMembers, Operation, Origin};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use super_platinum_core::{slack::Error, state::directory_usergroups as source_core};
mod render;
#[cfg(test)]
mod tests;

static NEXT: AtomicU64 = AtomicU64::new(0);
fn unavailable() -> Error {
    Error::Api("usergroup_source_unavailable".into())
}
/// Unavailable grants no native write authority. An already guarded producer
/// may retain its existing raw-label fallback, but must not initialize history.
pub(crate) enum Admission {
    Tracked(Work),
    Unavailable,
}
/// One accepted operation, its original request owner and its exact FIFO ticket.
/// Dropping an unfinished Work retires source availability through Ticket Drop;
/// followers cannot silently skip the accepted operation and claim completeness.
pub(crate) struct Work {
    owner: Arc<Context>,
    ticket: Option<source_core::Ticket>,
}
pub(crate) fn enqueue(
    shell: &mut ShellState,
    owner: Arc<Context>,
    operation: Operation,
) -> Admission {
    if !owner.current(shell) {
        return Admission::Unavailable;
    }
    let Ok(serial) = NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
    else {
        return Admission::Unavailable;
    };
    let Some(workspace) = shell.core.workspaces.get_mut(&owner.session.team_id) else {
        return Admission::Unavailable;
    };
    let generation = format!("usergroup-source-{serial}");
    match workspace.directory_usergroups.enqueue(
        &owner.account,
        &owner.session,
        owner.epoch,
        &generation,
        &workspace.usergroups,
        operation,
    ) {
        Ok(ticket) => Admission::Tracked(Work {
            owner,
            ticket: Some(ticket),
        }),
        Err(_) => Admission::Unavailable,
    }
}
impl Work {
    fn current(&self, shell: &ShellState, current: &impl Fn(&ShellState) -> bool) -> bool {
        self.owner.current(shell)
            && current(shell)
            && shell
                .core
                .workspaces
                .get(&self.owner.session.team_id)
                .is_some_and(|w| {
                    self.ticket.as_ref().is_some_and(|ticket| {
                        ticket.current(
                            &w.directory_usergroups,
                            &self.owner.account,
                            &self.owner.session,
                            self.owner.epoch,
                        )
                    })
                })
    }
    fn next(
        &self,
        shell: &ShellState,
        current: &impl Fn(&ShellState) -> bool,
    ) -> Result<Option<source_core::Pending>, Error> {
        if !self.current(shell, current) {
            return Err(unavailable());
        }
        let owner = &self.owner;
        let w = shell
            .core
            .workspaces
            .get(&owner.session.team_id)
            .ok_or_else(unavailable)?;
        w.directory_usergroups
            .next_for(
                self.ticket.as_ref().ok_or_else(unavailable)?,
                &owner.account,
                &owner.session,
                owner.epoch,
                &w.usergroups,
            )
            .map_err(|_| unavailable())
    }
    fn pending_current(
        &self,
        shell: &ShellState,
        pending: &source_core::Pending,
        current: &impl Fn(&ShellState) -> bool,
    ) -> bool {
        self.current(shell, current)
            && shell
                .core
                .workspaces
                .get(&self.owner.session.team_id)
                .is_some_and(|w| {
                    pending.current(
                        &w.directory_usergroups,
                        &self.owner.account,
                        &self.owner.session,
                        self.owner.epoch,
                        &w.usergroups,
                    )
                })
    }
    fn finish(
        &mut self,
        shell: &mut ShellState,
        pending: source_core::Pending,
        reply: source_core::RendererReply,
        current: &impl Fn(&ShellState) -> bool,
    ) -> Result<(), Error> {
        if !self.pending_current(shell, &pending, current) {
            return Err(unavailable());
        }
        let owner = &self.owner;
        let w = shell
            .core
            .workspaces
            .get_mut(&owner.session.team_id)
            .ok_or_else(unavailable)?;
        let commit = w
            .directory_usergroups
            .commit(
                pending,
                reply,
                &owner.account,
                &owner.session,
                owner.epoch,
                &w.usergroups,
            )
            .map_err(|_| unavailable())?;
        // No await or callback between paired publication and ticket retirement.
        let (source, native) = commit.into_parts();
        w.directory_usergroups = source;
        w.usergroups = native;
        self.ticket.take();
        shell.refresh_from_core();
        Ok(())
    }
    /// `current` retains the original GroupRead/panel/query guard. A successor
    /// request cannot lend this accepted operation a fresh account/socket owner.
    pub(crate) async fn run(
        mut self,
        mut state: Signal<ShellState>,
        current: impl Fn(&ShellState) -> bool,
    ) -> Result<(), Error> {
        let pending = loop {
            if let Some(pending) = self.next(&state.peek(), &current)? {
                break pending;
            }
            self.ticket
                .as_mut()
                .ok_or_else(unavailable)?
                .wait()
                .await
                .map_err(|_| unavailable())?;
        };
        let lowered = render::lower(
            state,
            |shell| self.pending_current(shell, &pending, &current),
            pending.generation(),
            pending.fields(),
        )
        .await?;
        if !self.pending_current(&state.peek(), &pending, &current) {
            return Err(unavailable());
        }
        self.finish(
            &mut state.write(),
            pending,
            source_core::RendererReply {
                generation: lowered.generation,
                fields: lowered.fields,
                lowered: lowered.lowered,
                locale: lowered.locale,
            },
            &current,
        )?;
        if self.owner.current(&state.peek()) {
            crate::bootstrap::persist_workspace_metadata(&state, &self.owner.session.team_id);
        }
        Ok(())
    }
}
/// Call only after releasing the producer's Signal write borrow. Synchronous
/// accepted socket operations need no consumer guard beyond their retained owner.
pub(crate) fn resume(state: Signal<ShellState>, work: Work) {
    dioxus::core::spawn_forever(async move {
        let _ = work.run(state, |_| true).await;
    });
}
