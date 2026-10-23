//! Bounded active-workspace autocomplete warming, independent of Share/Home UI.
//! Concrete accepted lifecycle events may schedule once for an original owner.
//! No timer, automatic failure retry, persisted observation or permission grant.
use super::observations::{self, Startup};
use crate::{connection::autocomplete::Observation, scheduled::Context, state::ShellState};
use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use std::sync::Arc;
use super_platinum_core::{
    slack::events::RtEvent, state::posting_metadata::authority::Authority,
    workflow_share::search::Query,
};

struct Original {
    context: Arc<Context>,
    authority: Authority,
    connection: Observation,
    reservation: Startup,
}
impl Original {
    fn capture(shell: &mut ShellState, team: &str) -> Option<Self> {
        let context = Arc::new(Context::capture(shell)?);
        if context.session.team_id != team {
            return None;
        }
        // This is an account read, not a query routing decision. The existing
        // observation supplies the actual raw connectivity and auth/socket fence.
        let connection = Observation::capture(shell, team, &Query::new("").ok()?)?;
        if !connection.online() {
            return None;
        }
        let authority = Authority::capture(&shell.core, team).ok()?;
        let reservation = Startup::reserve(shell, context.clone(), authority.clone()).ok()??;
        Some(Self {
            context,
            authority,
            connection,
            reservation,
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.reservation.current(shell)
            && self.context.current(shell)
            && self.authority.current(&shell.core)
            && self.connection.current(shell)
            && self.connection.online()
    }
}

/// Call only after an accepted boot/socket/initialization or relevant event.
/// Capture and reserve synchronously so repeated callbacks cannot queue jobs for
/// the same owner before the first future is polled. Missing facts simply defer.
pub(crate) fn schedule(mut state: Signal<ShellState>, team: &str) {
    let Some(original) = Original::capture(&mut state.write(), team) else {
        return;
    };
    // ShellState is root-lived; this job must not depend on an About/Share mount.
    dioxus::core::spawn_forever(async move {
        if !original.current(&state.peek()) {
            return;
        }
        let current = |s: &ShellState| original.current(s);
        let online =
            |s: &ShellState| original.connection.current(s) && original.connection.online();
        // The shared producer owns bounded reads, raw receipts, Retry-After and
        // complete-only publication. Failure must not block startup or retry here.
        let _ = observations::acquire(
            state,
            original.context.clone(),
            original.authority.clone(),
            &current,
            &online,
        )
        .await;
    });
}

/// Capture relevance before the admitted frame is consumed by the reducer;
/// scheduling happens afterward, using the newly accepted account facts.
pub(crate) fn relevant_event(
    shell: &ShellState,
    team: &str,
    generation: u64,
    event: &RtEvent,
) -> bool {
    shell.core.workspaces.get(team).is_some_and(|w| {
        w.rt_generation == generation
            && match event {
                RtEvent::PreferencesChanged { .. } => true,
                RtEvent::UserChanged(user) => user.id == w.self_user_id,
                RtEvent::Unknown(raw) => matches!(
                    raw.kind.as_str(),
                    "team_pref_change" | "enterprise_prefs_updated" | "team_join" | "team_leave"
                ),
                _ => false,
            }
    })
}

#[cfg(test)]
mod tests;
