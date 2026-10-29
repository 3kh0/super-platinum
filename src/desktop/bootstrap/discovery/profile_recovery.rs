//! Re-drive only a failed, idle, ordinary canonical profile after restoration.
use crate::{bootstrap::profile_navigation::ProfileVisit, state::ShellState};
use dioxus::prelude::*;

struct Source {
    profile: ProfileVisit,
    reconnect: crate::bootstrap::reconnect::Visit,
    user: String,
}

fn eligible(shell: &ShellState) -> Option<String> {
    let user = shell.profile_user.as_ref()?;
    if shell.profile_search.is_some()
        || crate::canvas::comments::profile_active(shell)
        || crate::channel_workflows::details::visible(shell)
    {
        return None;
    }
    let team = shell.core.active_team.as_ref()?;
    if !shell.core.workspaces.get(team)?.users.contains_key(user) {
        return None;
    }
    let pane = shell.core.profile_pane.as_ref()?;
    (pane.user == *user && !pane.loading && pane.error.is_some()).then(|| user.clone())
}

impl Source {
    fn capture(shell: &ShellState, reconnect: crate::bootstrap::reconnect::Visit) -> Option<Self> {
        let user = eligible(shell)?;
        let profile = ProfileVisit::capture(shell)?;
        (reconnect.current(shell) && profile.current(shell)).then_some(Self {
            profile,
            reconnect,
            user,
        })
    }
    fn begin(self, shell: &mut ShellState) -> Option<String> {
        if !self.reconnect.current(shell)
            || !self.profile.current(shell)
            || eligible(shell).as_deref() != Some(self.user.as_str())
        {
            return None;
        }
        let pane = shell.core.profile_pane.as_mut()?;
        pane.loading = true;
        pane.error = None;
        Some(self.user)
    }
}

async fn run(mut state: Signal<ShellState>, source: Source) {
    let user = {
        let Ok(mut shell) = state.try_write() else {
            return;
        };
        let Some(user) = source.begin(&mut shell) else {
            return;
        };
        user
    };
    // No await or detached handoff between admission and construction: the
    // existing reader binds this same profile before its first network await.
    super::refresh_open_profile(state, user).await;
}

pub(in crate::bootstrap) fn schedule_after_outage(
    state: Signal<ShellState>,
    reconnect: crate::bootstrap::reconnect::Visit,
) {
    let Some(source) = state
        .try_read()
        .ok()
        .and_then(|s| Source::capture(&s, reconnect))
    else {
        return;
    };
    dioxus::core::spawn_forever(run(state, source));
}

#[cfg(test)]
mod tests;
