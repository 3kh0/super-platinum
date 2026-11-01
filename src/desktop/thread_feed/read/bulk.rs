//! Explicit workspace Threads read action. No optimistic cursor/count clearing.
use super::*;
use std::sync::atomic::AtomicU8;

const SENDING: u8 = 0;
const UNCERTAIN: u8 = 1;

#[derive(Default)]
pub(super) struct State {
    operation: Option<Arc<Operation>>,
}
struct Operation {
    origin: Arc<()>,
    owner: Arc<crate::bootstrap::ReadOwner>,
    cursor_owner: Arc<crate::realtime::activity_counts::CursorOwner>,
    team: String,
    phase: AtomicU8,
}
impl Operation {
    fn current(&self, shell: &ShellState) -> bool {
        shell.signed_in
            && shell.core.active_team.as_ref() == Some(&self.team)
            && Arc::ptr_eq(&self.origin, &shell.thread_feed_reads.incarnation)
            && self.owner.current(&shell.core)
            && self.cursor_owner.current(&shell.core)
    }
    fn owns(&self, shell: &ShellState) -> bool {
        shell
            .thread_feed_reads
            .bulk
            .operation
            .as_ref()
            .is_some_and(|p| std::ptr::eq(p.as_ref(), self))
    }
}

pub(crate) fn has_unreads(shell: &ShellState) -> bool {
    shell
        .core
        .active_team
        .as_ref()
        .and_then(|team| shell.core.workspaces.get(team))
        .and_then(|workspace| workspace.thread_counts.snapshot.as_ref())
        .is_some_and(|counts| counts.has_unreads)
}
fn operation(shell: &ShellState) -> Option<&Arc<Operation>> {
    shell
        .thread_feed_reads
        .bulk
        .operation
        .as_ref()
        .filter(|p| p.current(shell))
}
pub(crate) fn status(shell: &ShellState) -> Option<&'static str> {
    let p = operation(shell)?;
    Some(if p.phase.load(Ordering::Acquire) == SENDING {
        "Marking threads read…"
    } else {
        "Could not confirm whether threads were marked read. Refresh before trying again."
    })
}

// Only an accepted full refresh resolves an uncertain operation. An empty
// appended page, public vector or failed refresh cannot confirm global state.
pub(super) fn after_refresh(shell: &mut ShellState) {
    if operation(shell).is_some_and(|p| p.phase.load(Ordering::Acquire) == UNCERTAIN) {
        shell.thread_feed_reads.bulk.operation = None;
    }
}

#[derive(Clone)]
pub(crate) struct Action {
    source: Source,
    accepted: Arc<row::Accepted>,
}
impl Action {
    pub(crate) fn capture(shell: &ShellState) -> Option<Self> {
        let action = Self {
            source: Source::capture(shell)?,
            accepted: shell.thread_feed_reads.accepted.clone()?,
        };
        (action.current(shell) && has_unreads(shell) && operation(shell).is_none())
            .then_some(action)
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.source.current(shell)
            && shell.overlay.is_none()
            && self.accepted.current(shell)
            && shell
                .thread_feed_reads
                .accepted
                .as_ref()
                .is_some_and(|p| Arc::ptr_eq(p, &self.accepted))
    }
    fn begin(&self, mut state: Signal<ShellState>) -> Option<Lease> {
        let mut shell = state.try_write().ok()?;
        if !self.current(&shell) || !has_unreads(&shell) || operation(&shell).is_some() {
            return None;
        }
        let p = Arc::new(Operation {
            origin: self.source.root.clone(),
            owner: self.source.owner.clone(),
            cursor_owner: self.source.cursor_owner.clone(),
            team: self.source.team.clone(),
            phase: AtomicU8::new(SENDING),
        });
        shell.thread_feed_reads.bulk.operation = Some(p.clone());
        drop(shell);
        Some(Lease {
            state,
            operation: p,
            settled: false,
        })
    }
    pub(crate) fn start(self, state: Signal<ShellState>) {
        // Admit synchronously in the explicit handler, before spawning.
        let Some(lease) = self.begin(state) else {
            return;
        };
        dioxus::core::spawn_forever(async move {
            if self.run_with(state, lease, send).await {
                // Same original Source and credentials. A changed feed/visit
                // retires this refresh rather than recapturing a successor.
                self.source.run(state, Mode::Refresh).await;
            }
        });
    }
    async fn run_with<F, Fut>(
        &self,
        mut state: Signal<ShellState>,
        mut lease: Lease,
        send: F,
    ) -> bool
    where
        F: FnOnce(Request) -> Fut,
        Fut: Future<Output = Result<(), Error>>,
    {
        let p = lease.operation.clone();
        let request = Request {
            state,
            action: self.clone(),
            operation: p.clone(),
        };
        if !request.current() {
            return false;
        }
        let result = send(request).await;
        let Ok(mut shell) = state.try_write() else {
            return false;
        };
        if !p.owns(&shell) || !self.current(&shell) {
            return false;
        }
        lease.settled = true;
        match result {
            Ok(()) => {
                shell.thread_feed_reads.bulk.operation = None;
                true
            }
            Err(error) => {
                p.phase.store(UNCERTAIN, Ordering::Release);
                shell.report_failure(&error, "Threads need a connection", || {
                    "Could not confirm whether threads were marked read. Refresh to check.".into()
                });
                false
            }
        }
    }
}

struct Request {
    state: Signal<ShellState>,
    action: Action,
    operation: Arc<Operation>,
}
impl Request {
    fn current(&self) -> bool {
        self.state.try_peek().is_ok_and(|shell| {
            self.action.current(&shell)
                && self.operation.owns(&shell)
                && self.operation.phase.load(Ordering::Acquire) == SENDING
        })
    }
}
async fn send(request: Request) -> Result<(), Error> {
    let source = &request.action.source;
    source
        .transport
        .execute_current(
            api::clear_thread_unreads_request(&source.client, &source.session),
            || request.current(),
        )
        .await?;
    Ok(())
}

struct Lease {
    state: Signal<ShellState>,
    operation: Arc<Operation>,
    settled: bool,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        // The refresh requirement must survive a conflicting signal borrow.
        // The operation is immutable ownership plus an atomic phase, so this
        // cannot release or mutate a successor even if UI cleanup must wait.
        self.operation.phase.store(UNCERTAIN, Ordering::Release);
        if let Ok(mut shell) = self.state.try_write() {
            if self.operation.owns(&shell) {
                if !self.operation.current(&shell) {
                    shell.thread_feed_reads.bulk.operation = None;
                }
            }
        }
    }
}

#[cfg(all(test, feature = "protected-native-fixture"))]
mod tests;
