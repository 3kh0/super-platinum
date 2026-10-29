//! Read-only Later page ownership, exact retry and app-owned task lifetime.
use super::{Context, LaterFilter, MainView, ShellState};
use crate::bootstrap::ReadOwner;
use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use std::{future::Future, sync::Arc};
use super_platinum_core::{later::SavedPage, slack::Error};
#[derive(Default)]
pub(crate) struct State {
    active: Option<Arc<Attempt>>,
    failed: Option<Arc<Failed>>,
}
enum Authority {
    Live {
        ctx: Arc<Context>,
        owner: Arc<ReadOwner>,
    },
    Fixture(super::message_fixture::Visit),
}
impl Authority {
    fn current(&self, shell: &ShellState) -> bool {
        match self {
            Self::Live { ctx, owner } => {
                owner.current(&shell.core)
                    && shell.core.active_team.as_ref() == Some(&ctx.session.team_id)
            }
            Self::Fixture(visit) => visit.current(shell),
        }
    }
}
pub(crate) struct Frame {
    authority: Authority,
    scope: Option<(u64, String)>,
    generation: u64,
    filter: LaterFilter,
    cursor: Option<String>,
    loading: bool,
    can_more: bool,
    view: MainView,
    visit: u64,
    failed: Option<Arc<Failed>>,
}
impl Frame {
    pub(crate) fn capture(shell: &ShellState) -> Option<Arc<Self>> {
        let authority = if let Some(ctx) = Context::capture(shell) {
            let owner = ReadOwner::capture(&shell.core, &ctx.transport, &ctx.session)?;
            Authority::Live {
                ctx: Arc::new(ctx),
                owner: Arc::new(owner),
            }
        } else {
            Authority::Fixture(super::message_fixture::Visit::capture(shell)?)
        };
        let later = &shell.core.later;
        Some(Arc::new(Self {
            authority,
            scope: later.scope.clone(),
            generation: later.generation,
            filter: later.filter,
            cursor: later.next_cursor.clone(),
            loading: later.loading,
            can_more: super::visible_state(shell).is_some()
                && later
                    .next_cursor
                    .as_ref()
                    .is_some_and(|cursor| !cursor.is_empty()),
            view: shell.main_view,
            visit: shell.thread_generation,
            failed: shell
                .later_read
                .failed
                .as_ref()
                .filter(|failed| failed.current(shell))
                .cloned(),
        }))
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.authority.current(shell)
            && self.scope == shell.core.later.scope
            && self.generation == shell.core.later.generation
            && self.filter == shell.core.later.filter
            && self.cursor == shell.core.later.next_cursor
            && self.view == shell.main_view
            && self.visit == shell.thread_generation
    }
    pub(crate) fn replace(self: &Arc<Self>, filter: LaterFilter) -> Source {
        Source {
            frame: self.clone(),
            op: Operation::Replace(filter),
        }
    }
    pub(crate) fn more(self: &Arc<Self>) -> Option<Source> {
        (self.can_more && !self.loading).then(|| Source {
            frame: self.clone(),
            op: Operation::More,
        })
    }
    pub(crate) fn retry(self: &Arc<Self>) -> Option<Source> {
        (!self.loading)
            .then(|| {
                self.failed.as_ref().map(|failed| Source {
                    frame: self.clone(),
                    op: Operation::Retry(failed.clone()),
                })
            })
            .flatten()
    }
}
#[derive(Clone)]
pub(crate) struct Source {
    frame: Arc<Frame>,
    op: Operation,
}
#[derive(Clone)]
enum Operation {
    Replace(LaterFilter),
    More,
    Retry(Arc<Failed>),
}
struct Failed {
    attempt: Arc<Attempt>,
}
impl Failed {
    fn current(&self, shell: &ShellState) -> bool {
        self.attempt.current_core(shell)
            && self.attempt.continuation == shell.core.later.next_cursor
    }
}
struct Attempt {
    ctx: Arc<Context>,
    owner: Arc<ReadOwner>,
    generation: u64,
    filter: LaterFilter,
    append: bool,
    cursor: Option<String>,
    continuation: Option<String>,
    dock_revision: Option<u64>,
}
impl Attempt {
    fn current_core(&self, shell: &ShellState) -> bool {
        current(shell, &self.ctx, &self.owner, self.generation)
            && shell.core.later.filter == self.filter
    }
    fn current(self: &Arc<Self>, shell: &ShellState) -> bool {
        self.current_core(shell)
            && shell
                .later_read
                .active
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(active, self))
    }
}
pub(super) fn current(
    shell: &ShellState,
    ctx: &Context,
    owner: &ReadOwner,
    generation: u64,
) -> bool {
    owner.current(&shell.core)
        && ctx.current(shell)
        && shell.core.active_team.as_ref() == Some(&ctx.session.team_id)
        && shell.core.later.generation == generation
}
struct Lease {
    state: Signal<ShellState>,
    attempt: Arc<Attempt>,
}
impl Lease {
    fn begin(mut state: Signal<ShellState>, source: Source) -> Option<Self> {
        let mut shell = state.write();
        if !source.frame.current(&shell) {
            return None;
        }
        let Authority::Live { ctx, owner } = &source.frame.authority else {
            return None;
        };
        let (filter, append, cursor) = match source.op {
            Operation::Replace(filter) => (filter, false, None),
            Operation::More => (source.frame.filter, true, source.frame.cursor.clone()),
            Operation::Retry(failed) => {
                if !failed.current(&shell)
                    || !shell
                        .later_read
                        .failed
                        .as_ref()
                        .is_some_and(|current| Arc::ptr_eq(current, &failed))
                {
                    return None;
                }
                (
                    failed.attempt.filter,
                    failed.attempt.append,
                    failed.attempt.cursor.clone(),
                )
            }
        };
        if append
            && (shell.core.later.loading
                || cursor.is_none()
                || cursor != shell.core.later.next_cursor)
        {
            return None;
        }
        let retain = shell
            .core
            .later
            .matches_scope(ctx.epoch, &ctx.session.team_id)
            && shell.core.later.filter == filter;
        let continuation = retain
            .then(|| shell.core.later.next_cursor.clone())
            .flatten();
        let dock_revision = shell
            .core
            .workspaces
            .get(&ctx.session.team_id)
            .map(|workspace| workspace.dock_counts.revision());
        shell
            .core
            .later
            .ensure_scope(ctx.epoch, &ctx.session.team_id);
        let generation = shell.core.later.begin(filter, append);
        // A successful replacement owns the new continuation. Until then keep
        // the existing one, disabled while busy; errors/cancellation lose none.
        if !append && retain {
            shell.core.later.next_cursor = continuation;
        }
        let attempt = Arc::new(Attempt {
            ctx: ctx.clone(),
            owner: owner.clone(),
            generation,
            filter,
            append,
            cursor,
            continuation: shell.core.later.next_cursor.clone(),
            dock_revision,
        });
        shell.later_read.failed = None;
        shell.later_read.active = Some(attempt.clone());
        drop(shell);
        Some(Self { state, attempt })
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        // Best-effort early-drop cleanup. UI reads are app-owned and expose no
        // abort control; arbitrary cancellation under a conflicting signal borrow
        // is not an unconditional cleanup guarantee.
        let Ok(mut shell) = self.state.try_write() else {
            return;
        };
        if shell
            .later_read
            .active
            .as_ref()
            .is_some_and(|active| Arc::ptr_eq(active, &self.attempt))
        {
            shell.later_read.active = None;
            if shell.core.later.generation == self.attempt.generation {
                shell.core.later.loading = false;
            }
        }
    }
}
struct Fetch {
    ctx: Arc<Context>,
    filter: LaterFilter,
    cursor: Option<String>,
}
struct Accepted {
    ctx: Arc<Context>,
    owner: Arc<ReadOwner>,
    generation: u64,
}
async fn run_with<F, Fut>(
    mut state: Signal<ShellState>,
    source: Source,
    lookup: F,
) -> Option<Accepted>
where
    F: FnOnce(Fetch) -> Fut,
    Fut: Future<Output = Result<SavedPage, Error>>,
{
    let lease = Lease::begin(state, source)?;
    let attempt = &lease.attempt;
    if !attempt.current(&state.peek()) {
        return None;
    }
    let result = lookup(Fetch {
        ctx: attempt.ctx.clone(),
        filter: attempt.filter,
        cursor: attempt.cursor.clone(),
    })
    .await;
    let mut shell = state.write();
    if !attempt.current(&shell) {
        return None;
    }
    let accepted = match result {
        Ok(page) => {
            let overdue = page.counts.uncompleted_overdue_count;
            if !shell.core.later.apply_page(
                attempt.generation,
                page,
                attempt.append,
                attempt.cursor.as_deref(),
            ) {
                return None;
            }
            if let (Some(count), Some(workspace)) = (
                overdue,
                shell.core.workspaces.get_mut(&attempt.ctx.session.team_id),
            ) {
                if Some(workspace.dock_counts.revision()) == attempt.dock_revision {
                    workspace.dock_counts.apply_overdue_count(count);
                }
            }
            shell.refresh_dock_badge_for(&attempt.ctx.session.team_id);
            Some(Accepted {
                ctx: attempt.ctx.clone(),
                owner: attempt.owner.clone(),
                generation: attempt.generation,
            })
        }
        Err(error) => {
            shell.core.later.loading = false;
            shell.core.later.error = Some(
                if error.is_offline() {
                    "Connect to Slack to load saved items. Your loaded items are still available."
                } else {
                    "Could not load saved items. Your loaded items are still available. Try again."
                }
                .into(),
            );
            shell.later_read.failed = Some(Arc::new(Failed {
                attempt: attempt.clone(),
            }));
            None
        }
    };
    shell.later_read.active = None;
    accepted
}
pub(super) async fn run(state: Signal<ShellState>, source: Source) {
    if let Some(accepted) = run_with(state, source, |fetch| async move {
        super::api::fetch_saved(
            &fetch.ctx.transport,
            &fetch.ctx.client,
            &fetch.ctx.session,
            fetch.filter,
            fetch.cursor,
        )
        .await
    })
    .await
    {
        if state.peek().main_view == MainView::Later
            && current(
                &state.peek(),
                &accepted.ctx,
                &accepted.owner,
                accepted.generation,
            )
        {
            super::hydrate(state, &accepted.ctx, &accepted.owner, accepted.generation).await;
        }
    }
}
pub(crate) fn start(state: Signal<ShellState>, source: Source) {
    if !source.frame.current(&state.peek()) {
        return;
    }
    if matches!(&source.frame.authority, Authority::Fixture(_)) {
        let (filter, append) = match source.op {
            Operation::Replace(filter) => (filter, false),
            Operation::More => (source.frame.filter, true),
            Operation::Retry(_) => return,
        };
        super::message_fixture::filter(state, filter, append);
        return;
    }
    // Reads outlive panel scopes. The original source is revalidated before
    // reservation on the task's first poll; credentials are never recaptured.
    dispatch(state, source, run);
}
fn dispatch<F, Fut>(state: Signal<ShellState>, source: Source, drive: F)
where
    F: FnOnce(Signal<ShellState>, Source) -> Fut,
    Fut: Future<Output = ()> + 'static,
{
    if !source.frame.current(&state.peek()) {
        return;
    }
    dioxus::core::spawn_forever(drive(state, source));
}
#[cfg(test)]
mod tests;
