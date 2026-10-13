//! VIP writes outlive a profile pane, but never their authenticated account.
use crate::state::ShellState;
use dioxus::prelude::*;
use std::{collections::HashMap, sync::Arc};
use super_platinum_core::slack::{Transport, api};

type Key = (u64, String, String);
#[derive(Default)]
pub(crate) struct Pending(HashMap<Key, Option<Arc<Transport>>>);

fn same_transport(a: &Option<Arc<Transport>>, b: &Option<Arc<Transport>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

pub(crate) fn busy(shell: &ShellState, user: &str) -> bool {
    let Some(team) = &shell.core.active_team else {
        return false;
    };
    shell
        .profile_vip_pending
        .0
        .get(&(shell.core.account_epoch, team.clone(), user.into()))
        .is_some_and(|transport| same_transport(transport, &shell.core.transport))
}

struct Change {
    key: Key,
    transport: Option<Arc<Transport>>,
    desired: bool,
    revision: u64,
    profile: u64,
}

impl Change {
    fn begin(shell: &mut ShellState, user: String) -> Option<Self> {
        let team = shell.core.active_team.clone()?;
        let workspace = shell.core.workspaces.get(&team)?;
        if workspace.self_user_id == user
            || workspace.users.get(&user)?.deleted
            || busy(shell, &user)
        {
            return None;
        }
        let change = Self {
            key: (shell.core.account_epoch, team, user.clone()),
            transport: shell.core.transport.clone(),
            desired: !workspace.vip_users.contains(&user),
            revision: workspace.vip_revision(&user),
            profile: shell.profile_generation,
        };
        shell
            .profile_vip_pending
            .0
            .retain(|(epoch, _, _), transport| {
                *epoch == change.key.0 && same_transport(transport, &change.transport)
            });
        shell
            .profile_vip_pending
            .0
            .insert(change.key.clone(), change.transport.clone());
        Some(change)
    }
    fn account_current(&self, shell: &ShellState) -> bool {
        shell.core.account_epoch == self.key.0
            && same_transport(&self.transport, &shell.core.transport)
            && shell.core.workspaces.contains_key(&self.key.1)
    }
    fn visible(&self, shell: &ShellState) -> bool {
        self.account_current(shell)
            && shell.core.active_team.as_ref() == Some(&self.key.1)
            && shell.profile_generation == self.profile
            && (shell.profile_user.as_ref() == Some(&self.key.2)
                || shell
                    .profile_hover
                    .as_ref()
                    .is_some_and(|hover| hover.user_id == self.key.2))
    }
    fn release(&self, shell: &mut ShellState) -> bool {
        if !shell
            .profile_vip_pending
            .0
            .get(&self.key)
            .is_some_and(|transport| same_transport(transport, &self.transport))
        {
            return false;
        }
        shell.profile_vip_pending.0.remove(&self.key);
        true
    }
    fn complete(&self, shell: &mut ShellState, success: bool) -> bool {
        if !self.release(shell) || !self.account_current(shell) || !success {
            return false;
        }
        let workspace = shell.core.workspaces.get_mut(&self.key.1).unwrap();
        workspace.apply_vip_change_since(&self.key.2, self.desired, self.revision);
        let applied = workspace.vip_users.contains(&self.key.2) == self.desired;
        if self.visible(shell) && applied {
            shell.show_toast(if self.desired {
                "Added to VIPs"
            } else {
                "Removed from VIPs"
            });
        }
        if shell.core.active_team.as_ref() == Some(&self.key.1) {
            shell.refresh_from_core();
        }
        true
    }
}

pub(crate) fn fixture_pending(shell: &mut ShellState, user: &str) {
    let _ = Change::begin(shell, user.into());
}

pub fn toggle_profile_vip(mut state: Signal<ShellState>, user: String) {
    let Some((transport, client, sessions)) = crate::bootstrap::credentials(&state) else {
        state.write().show_toast("Sign in to update VIPs.");
        return;
    };
    let Some(session) = sessions
        .into_iter()
        .find(|s| state.peek().core.active_team.as_ref() == Some(&s.team_id))
    else {
        return;
    };
    let Some(change) = Change::begin(&mut state.write(), user) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        if !change.account_current(&state.peek()) {
            change.release(&mut state.write());
            return;
        }
        let result = if change.desired {
            api::add_priority_user(&transport, &client, &session, change.key.2.clone()).await
        } else {
            api::remove_priority_user(&transport, &client, &session, change.key.2.clone()).await
        };
        let mut shell = state.write();
        let visible = change.visible(&shell);
        let persist = change.complete(&mut shell, result.is_ok());
        if let Err(error) = result
            && visible
        {
            shell.report_failure(&error, "Connect to Slack to update VIPs.", || {
                "Could not update VIPs. Try again.".into()
            });
        }
        drop(shell);
        if persist {
            crate::bootstrap::persist_workspace(&state, &change.key.1);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn shell() -> ShellState {
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "profile-pane")
    }
    #[test]
    fn closing_and_reopening_does_not_duplicate_or_cancel_a_persons_write() {
        let mut shell = shell();
        let change = Change::begin(&mut shell, "U1".into()).unwrap();
        assert!(busy(&shell, "U1"));
        assert!(!busy(&shell, "U2"));
        shell.close_profile();
        shell.profile_user = Some("U1".into());
        assert!(Change::begin(&mut shell, "U1".into()).is_none());
        assert!(change.complete(&mut shell, true));
        assert_eq!(
            shell.core.workspaces["T1"].vip_users.contains("U1"),
            change.desired
        );
        assert!(!busy(&shell, "U1"));
        assert!(shell.toast.is_none());
    }
    #[test]
    fn concurrent_people_have_independent_loading_and_cleanup() {
        let mut shell = shell();
        let first = Change::begin(&mut shell, "U1".into()).unwrap();
        let second = Change::begin(&mut shell, "U2".into()).unwrap();
        assert!(first.complete(&mut shell, true));
        assert!(busy(&shell, "U2"));
        assert!(!second.complete(&mut shell, false));
        assert!(!busy(&shell, "U2"));
    }
    #[test]
    fn stale_account_and_transport_cannot_mutate_or_clear_new_requests() {
        for change_account in [true, false] {
            let mut shell = shell();
            let old = Change::begin(&mut shell, "U1".into()).unwrap();
            if change_account {
                shell.core.account_epoch += 1;
            } else {
                shell.core.transport = Some(Arc::new(Transport::new("fixture").unwrap()));
            }
            let new = Change::begin(&mut shell, "U1".into()).unwrap();
            let before = shell.core.workspaces["T1"].vip_users.clone();
            assert!(!old.complete(&mut shell, true));
            assert_eq!(shell.core.workspaces["T1"].vip_users, before);
            assert!(busy(&shell, "U1"));
            assert!(new.complete(&mut shell, true));
        }
    }
    #[test]
    fn completion_updates_only_the_original_workspace_after_switching_away() {
        let mut shell = shell();
        let change = Change::begin(&mut shell, "U1".into()).unwrap();
        let mut other = shell.core.workspaces["T1"].clone();
        other.team_id = "OTHER".into();
        let before = other.vip_users.clone();
        shell.core.workspaces.insert("OTHER".into(), other);
        shell.core.active_team = Some("OTHER".into());
        assert!(change.complete(&mut shell, true));
        assert_eq!(shell.core.workspaces["OTHER"].vip_users, before);
        assert_eq!(
            shell.core.workspaces["T1"].vip_users.contains("U1"),
            change.desired
        );
        assert!(shell.toast.is_none());
    }
    #[test]
    fn newer_realtime_preference_wins_over_the_success_response() {
        let mut shell = shell();
        let change = Change::begin(&mut shell, "U1".into()).unwrap();
        let w = shell.core.workspaces.get_mut("T1").unwrap();
        let initial = w.vip_users.clone();
        let mut changed = initial.clone();
        if !changed.remove("U1") {
            changed.insert("U1".into());
        }
        w.apply_vip_snapshot(changed);
        w.apply_vip_snapshot(initial.clone());
        assert!(change.complete(&mut shell, true));
        assert_eq!(shell.core.workspaces["T1"].vip_users, initial);
        assert!(shell.toast.is_none());
    }
}
